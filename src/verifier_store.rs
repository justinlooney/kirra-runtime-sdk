// src/verifier_store.rs

use std::collections::HashMap;
use rusqlite::{params, Connection, Result};
use crate::verifier::{NodeTrustState, RegisteredNode};

pub struct VerifierStore {
    conn: Connection,
}

impl VerifierStore {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS nodes (
                node_id                    TEXT PRIMARY KEY,
                status_json                TEXT NOT NULL,
                registered_at_ms           INTEGER NOT NULL DEFAULT 0,
                last_trust_update_ms       INTEGER NOT NULL DEFAULT 0,
                ak_public_pem              TEXT,
                expected_pcr16_digest_hex  TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS dependencies (
                node_id  TEXT NOT NULL,
                dep_id   TEXT NOT NULL,
                PRIMARY KEY (node_id, dep_id)
            )",
            [],
        )?;

        // v0.9.7: time-series posture event log for observability and flap detection.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS posture_events (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                node_id        TEXT    NOT NULL,
                event_type     TEXT    NOT NULL,
                posture_json   TEXT    NOT NULL,
                reason         TEXT,
                created_at_ms  INTEGER NOT NULL
            )",
            [],
        )?;

        Self::init_audit_chain_schema(&conn)?;

        Ok(Self { conn })
    }

    pub fn save_node(&self, node: &RegisteredNode) -> Result<()> {
        let status_json = serde_json::to_string(&node.status)
            .map_err(|_| rusqlite::Error::InvalidQuery)?;

        self.conn.execute(
            "INSERT OR REPLACE INTO nodes
             (node_id, status_json, registered_at_ms, last_trust_update_ms,
              ak_public_pem, expected_pcr16_digest_hex)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                node.node_id,
                status_json,
                node.registered_at_ms as i64,
                node.last_trust_update_ms as i64,
                node.ak_public_pem,
                node.expected_pcr16_digest_hex,
            ],
        )?;

        Ok(())
    }

    pub fn load_nodes(&self) -> Result<Vec<RegisteredNode>> {
        let mut stmt = self.conn.prepare(
            "SELECT node_id, status_json, registered_at_ms, last_trust_update_ms,
                    ak_public_pem, expected_pcr16_digest_hex
             FROM nodes",
        )?;

        let rows = stmt.query_map([], |row| {
            let status_json: String = row.get(1)?;
            let status: NodeTrustState = serde_json::from_str(&status_json)
                .unwrap_or(NodeTrustState::Unknown);

            Ok(RegisteredNode {
                node_id: row.get(0)?,
                status,
                registered_at_ms: row.get::<_, i64>(2)? as u64,
                last_trust_update_ms: row.get::<_, i64>(3)? as u64,
                ak_public_pem: row.get(4)?,
                expected_pcr16_digest_hex: row.get(5)?,
                // Nonces are purposely ephemeral — not persisted, reset to None on boot.
            })
        })?;

        rows.collect()
    }

    pub fn save_dependencies(&self, node_id: &str, deps: &[String]) -> Result<()> {
        self.conn.execute(
            "DELETE FROM dependencies WHERE node_id = ?1",
            params![node_id],
        )?;

        for dep in deps {
            self.conn.execute(
                "INSERT OR REPLACE INTO dependencies (node_id, dep_id) VALUES (?1, ?2)",
                params![node_id, dep],
            )?;
        }

        Ok(())
    }

    pub fn load_dependencies(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut stmt = self.conn.prepare("SELECT node_id, dep_id FROM dependencies")?;
        let mut map: HashMap<String, Vec<String>> = HashMap::new();

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (node_id, dep_id) = row?;
            map.entry(node_id).or_default().push(dep_id);
        }

        Ok(map)
    }

    // --- v0.9.7 posture event log -------------------------------------------------

    /// Append a single posture event to the time-series log.
    pub fn save_posture_event(
        &self,
        node_id: &str,
        event_type: &str,
        posture_json: &str,
        reason: Option<&str>,
        created_at_ms: u64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO posture_events
             (node_id, event_type, posture_json, reason, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![node_id, event_type, posture_json, reason, created_at_ms as i64],
        )?;
        Ok(())
    }

    /// Return the full event history for a node, newest first.
    pub fn load_node_history(&self, node_id: &str) -> Result<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT event_type, posture_json, reason, created_at_ms
             FROM posture_events
             WHERE node_id = ?1
             ORDER BY created_at_ms DESC",
        )?;

        let rows = stmt.query_map(params![node_id], |row| {
            let event_type: String = row.get(0)?;
            let posture_json: String = row.get(1)?;
            let reason: Option<String> = row.get(2)?;
            let created_at_ms: i64 = row.get(3)?;

            let posture: serde_json::Value = serde_json::from_str(&posture_json)
                .unwrap_or(serde_json::Value::Null);

            Ok(serde_json::json!({
                "event_type": event_type,
                "posture": posture,
                "reason": reason,
                "created_at_ms": created_at_ms as u64,
            }))
        })?;

        rows.collect()
    }

    /// Count events for a node with `created_at_ms >= since_ms`.
    /// Used by the flap-detection analytic.
    pub fn count_recent_posture_events(&self, node_id: &str, since_ms: u64) -> Result<u64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM posture_events
             WHERE node_id = ?1 AND created_at_ms >= ?2",
            params![node_id, since_ms as i64],
            |row| row.get(0),
        )
    }

    // --- v0.9.8 HA probes & backup export ---

    /// Lightweight liveness check: proves the SQLite connection is alive.
    pub fn health_check(&self) -> Result<()> {
        self.conn.query_row("SELECT 1", [], |_| Ok(()))
    }

    /// Return every posture event in ascending timestamp order (full backup scan).
    pub fn load_all_posture_events(&self) -> Result<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT node_id, event_type, posture_json, reason, created_at_ms
             FROM posture_events
             ORDER BY created_at_ms ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            let node_id: String = row.get(0)?;
            let event_type: String = row.get(1)?;
            let posture_json: String = row.get(2)?;
            let reason: Option<String> = row.get(3)?;
            let created_at_ms: i64 = row.get(4)?;

            let posture: serde_json::Value = serde_json::from_str(&posture_json)
                .unwrap_or(serde_json::Value::Null);

            Ok(serde_json::json!({
                "node_id": node_id,
                "event_type": event_type,
                "posture": posture,
                "reason": reason,
                "created_at_ms": created_at_ms as u64,
            }))
        })?;

        rows.collect()
    }

    // --- v1.1 tamper-evident audit chain -------------------------------------

    fn init_audit_chain_schema(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS audit_log_chain (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type        TEXT NOT NULL,
                event_json        TEXT NOT NULL,
                previous_hash_hex TEXT NOT NULL,
                record_hash_hex   TEXT NOT NULL,
                created_at_ms     INTEGER NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS federated_trust_reports (
                id                   INTEGER PRIMARY KEY AUTOINCREMENT,
                source_controller_id TEXT NOT NULL,
                asset_id             TEXT NOT NULL,
                posture_json         TEXT NOT NULL,
                issued_at_ms         INTEGER NOT NULL,
                expires_at_ms        INTEGER NOT NULL,
                received_at_ms       INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    pub fn save_posture_event_chained(
        &mut self,
        node_id: &str,
        event_type: &str,
        posture_json: &str,
        reason: Option<&str>,
        created_at_ms: u64,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO posture_events
             (node_id, event_type, posture_json, reason, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![node_id, event_type, posture_json, reason, created_at_ms as i64],
        )?;
        crate::audit_chain::AuditChainLinker::append_audit_event_tx(
            &tx, event_type, posture_json, created_at_ms as i64,
        )?;
        tx.commit()
    }

    pub fn save_federated_report_chained(
        &mut self,
        source_controller_id: &str,
        asset_id: &str,
        posture_json: &str,
        issued_at_ms: u64,
        expires_at_ms: u64,
        received_at_ms: u64,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO federated_trust_reports
             (source_controller_id, asset_id, posture_json, issued_at_ms, expires_at_ms, received_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_controller_id, asset_id, posture_json,
                issued_at_ms as i64, expires_at_ms as i64, received_at_ms as i64,
            ],
        )?;
        let audit = serde_json::json!({
            "source_controller_id": source_controller_id,
            "asset_id": asset_id,
            "posture": posture_json,
            "issued_at_ms": issued_at_ms,
            "expires_at_ms": expires_at_ms,
            "received_at_ms": received_at_ms,
        });
        crate::audit_chain::AuditChainLinker::append_audit_event_tx(
            &tx,
            "FEDERATED_TRUST_REPORT_ACCEPTED",
            &audit.to_string(),
            received_at_ms as i64,
        )?;
        tx.commit()
    }

    pub fn load_federated_reports_for_asset(
        &self,
        asset_id: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT source_controller_id, asset_id, posture_json, issued_at_ms, expires_at_ms
             FROM federated_trust_reports
             WHERE asset_id = ?1
             ORDER BY received_at_ms DESC",
        )?;
        let rows = stmt.query_map(params![asset_id], |row| {
            let source: String = row.get(0)?;
            let aid: String = row.get(1)?;
            let posture_json: String = row.get(2)?;
            let issued: i64 = row.get(3)?;
            let expires: i64 = row.get(4)?;
            Ok(serde_json::json!({
                "source_controller_id": source,
                "asset_id": aid,
                "posture": posture_json,
                "issued_at_ms": issued as u64,
                "expires_at_ms": expires as u64,
            }))
        })?;
        rows.collect()
    }

    pub fn verify_audit_chain_integrity(&self) -> Result<bool> {
        let mut stmt = self.conn.prepare(
            "SELECT event_json, previous_hash_hex, record_hash_hex, created_at_ms
             FROM audit_log_chain
             ORDER BY id ASC",
        )?;

        let mut expected_previous_hash = "0".repeat(64);
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let event_json: String = row.get(0)?;
            let previous_hash_hex: String = row.get(1)?;
            let record_hash_hex: String = row.get(2)?;
            let created_at_ms: i64 = row.get(3)?;

            if previous_hash_hex != expected_previous_hash {
                return Ok(false);
            }
            let recalc = crate::audit_chain::AuditChainLinker::compute_record_hash(
                &previous_hash_hex,
                &event_json,
                created_at_ms,
            );
            if recalc != record_hash_hex {
                return Ok(false);
            }
            expected_previous_hash = record_hash_hex;
        }

        Ok(true)
    }
}
