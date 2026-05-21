// ============================================================================
// PATCH: src/verifier_store.rs
//
// v2.2.0 — AV subsystem node registration
//
// DESIGN DECISION — why we do NOT add av_node_registry / av_node_dependencies:
//
// The milestone doc proposed two new tables (av_node_registry, av_node_dependencies)
// to track AV sensor assets. This was rejected for a critical reason:
//
//   AppState::recursive_calculate (the gray/black DAG traversal, invariant #4)
//   reads from AppState.nodes (DashMap) and AppState.deps (DashMap). It does NOT
//   read from SQLite at traversal time — SQLite is the persistence layer, DashMap
//   is the runtime layer. Any nodes registered only in av_node_registry would be
//   INVISIBLE to the posture engine. Sensor faults on those nodes would never
//   propagate through the DAG and would never affect FleetPosture.
//
// The correct approach: AV subsystem nodes are registered through the EXISTING
// node registration path (POST /attestation/register → nodes table → AppState.nodes
// DashMap). The av_subsystem_meta table below is additive metadata only — it
// annotates existing node_ids with AV-specific context without forking the DAG.
//
// AV node dependencies are registered through the EXISTING POST /fleet/dependencies
// path (dependencies table → AppState.deps DashMap), which feeds recursive_calculate.
//
// This means:
//   lidar_front → trusted/untrusted in AppState.nodes (existing)
//   lidar_front → perception_fusion dependency edge in AppState.deps (existing)
//   recursive_calculate sees both and propagates posture correctly (existing)
//   av_subsystem_meta holds AV classification + telemetry metadata (new, additive)
//
// ============================================================================

// ----------------------------------------------------------------------------
// ADD to the schema initialization transaction in VerifierStore::new() or
// VerifierStore::initialize_schema(), after the existing table creation blocks.
//
// Only av_subsystem_meta is new. It annotates existing node_ids — it does not
// replace or shadow the nodes/dependencies tables that feed the DAG engine.
// ----------------------------------------------------------------------------

/*
// AV subsystem classification metadata.
// node_id REFERENCES nodes(node_id) — must be registered via /attestation/register first.
// subsystem_class: 'Perception' | 'Planning' | 'Actuation' | 'Positioning'
// hardware_serial: physical device identifier for audit trail
// confidence_floor: minimum acceptable confidence score before node is marked Untrusted
// last_telemetry_ms: last received health report timestamp
tx.execute(
    "CREATE TABLE IF NOT EXISTS av_subsystem_meta (
        node_id            TEXT PRIMARY KEY,
        subsystem_class    TEXT NOT NULL CHECK(subsystem_class IN ('Perception','Planning','Actuation','Positioning')),
        hardware_serial    TEXT NOT NULL,
        confidence_floor   REAL NOT NULL DEFAULT 0.70,
        last_telemetry_ms  INTEGER NOT NULL DEFAULT 0,
        FOREIGN KEY(node_id) REFERENCES nodes(node_id)
    );",
    [],
)?;
*/

// ----------------------------------------------------------------------------
// NEW STORE METHODS — append to impl VerifierStore in src/verifier_store.rs
// ----------------------------------------------------------------------------

// Note: VerifierStore is NOT Mutex-wrapped in AppState. These methods are called
// directly on the store reference, which is accessed via svc.app.store.
// The store uses rusqlite in WAL mode; concurrent reads are safe, writes serialize
// via rusqlite's internal connection management.

/*
/// Registers AV subsystem metadata for an existing node.
///
/// The node MUST already exist in the `nodes` table (registered via
/// /attestation/register). This method adds the AV classification layer on top.
///
/// Disk-first ordering (invariant #12): SQLite write completes before any
/// in-memory state is updated by the caller.
pub fn register_av_subsystem_meta(
    &self,
    node_id: &str,
    subsystem_class: &str,
    hardware_serial: &str,
    confidence_floor: f64,
    now_ms: u64,
) -> rusqlite::Result<()> {
    self.conn.execute(
        "INSERT INTO av_subsystem_meta (node_id, subsystem_class, hardware_serial, confidence_floor, last_telemetry_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(node_id) DO UPDATE SET
             subsystem_class   = excluded.subsystem_class,
             hardware_serial   = excluded.hardware_serial,
             confidence_floor  = excluded.confidence_floor,
             last_telemetry_ms = excluded.last_telemetry_ms",
        rusqlite::params![node_id, subsystem_class, hardware_serial, confidence_floor, now_ms as i64],
    )?;
    Ok(())
}

/// Updates the last telemetry timestamp for an AV node.
/// Called on every received sensor health report.
pub fn touch_av_telemetry_timestamp(
    &self,
    node_id: &str,
    now_ms: u64,
) -> rusqlite::Result<()> {
    self.conn.execute(
        "UPDATE av_subsystem_meta SET last_telemetry_ms = ?1 WHERE node_id = ?2",
        rusqlite::params![now_ms as i64, node_id],
    )?;
    Ok(())
}

/// Loads the confidence floor for a registered AV subsystem node.
/// Returns None if the node has no AV metadata entry.
pub fn load_av_confidence_floor(
    &self,
    node_id: &str,
) -> rusqlite::Result<Option<f64>> {
    let result = self.conn.query_row(
        "SELECT confidence_floor FROM av_subsystem_meta WHERE node_id = ?1",
        rusqlite::params![node_id],
        |row| row.get::<_, f64>(0),
    );
    match result {
        Ok(floor) => Ok(Some(floor)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}
*/
