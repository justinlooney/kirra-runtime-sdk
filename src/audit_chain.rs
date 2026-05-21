// src/audit_chain.rs

use rusqlite::{params, Transaction, Result};
use sha2::{Sha256, Digest};

pub struct AuditChainLinker;

impl AuditChainLinker {
    pub fn compute_record_hash(
        previous_hash: &str,
        canonical_json: &str,
        created_at_ms: i64,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(previous_hash.as_bytes());
        hasher.update(canonical_json.as_bytes());
        hasher.update(created_at_ms.to_string().as_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn append_audit_event_tx(
        tx: &Transaction,
        event_type: &str,
        event_json_payload: &str,
        created_at_ms: i64,
    ) -> Result<()> {
        let previous_hash: String = tx
            .query_row(
                "SELECT record_hash_hex FROM audit_log_chain ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "0".repeat(64));

        let record_hash = Self::compute_record_hash(&previous_hash, event_json_payload, created_at_ms);

        tx.execute(
            "INSERT INTO audit_log_chain
             (event_type, event_json, previous_hash_hex, record_hash_hex, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![event_type, event_json_payload, previous_hash, record_hash, created_at_ms],
        )?;

        Ok(())
    }
}
