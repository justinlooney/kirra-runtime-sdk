// src/posture_cache.rs — CachedFleetPosture, SharedPostureCache, ServiceState, now_ms

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::verifier::{AppState, FleetNodePosture, FleetPosture};

/// Returns the current time as milliseconds since UNIX epoch.
/// Exported for use by service binary and test infrastructure.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Posture cache entries older than this are treated as stale (fail-closed).
pub const POSTURE_CACHE_TTL_MS: u64 = 5_000;

/// A complete, immutable snapshot of the fleet posture at a point in time.
///
/// Atomically replaced (never field-mutated) by recalculate_and_broadcast.
/// Readers always observe a consistent snapshot.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CachedFleetPosture {
    /// The current aggregated system-wide posture derived from the DAG.
    pub propagated_status: FleetPosture,

    /// Absolute timestamp (ms since UNIX epoch) when this snapshot was computed.
    pub generated_at_ms: u64,

    /// Staleness TTL in milliseconds. Set from POSTURE_CACHE_TTL_MS by the engine.
    pub ttl_ms: u64,

    /// Monotonically increasing generation counter from next_generation().
    pub generation: u64,
}

impl CachedFleetPosture {
    /// Engine-assigned constructor. Supply generation from next_generation()
    /// and timestamp from now_ms().
    pub fn new_with_generation(posture: FleetPosture, generation: u64, ts: u64) -> Self {
        Self {
            propagated_status: posture,
            generated_at_ms: ts,
            ttl_ms: POSTURE_CACHE_TTL_MS,
            generation,
        }
    }

    /// Convenience constructor for tests and cold-start initialization.
    /// Uses generation=1 and the current system time.
    pub fn new(posture: FleetPosture) -> Self {
        Self {
            propagated_status: posture,
            generated_at_ms: now_ms(),
            ttl_ms: POSTURE_CACHE_TTL_MS,
            generation: 1,
        }
    }

    /// Constructs a cache entry from a per-node FleetNodePosture snapshot.
    /// Used by the service binary's get_node_posture handler.
    pub fn from_posture(posture: &FleetNodePosture, ts: u64) -> Self {
        Self {
            propagated_status: posture.propagated_status.clone(),
            generated_at_ms: ts,
            ttl_ms: POSTURE_CACHE_TTL_MS,
            generation: 1,
        }
    }

    /// Returns true if this entry has exceeded its TTL relative to `now_ms`.
    pub fn is_stale(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.generated_at_ms) >= self.ttl_ms
    }
}

/// The shared posture cache type.
///
/// Uses std::sync::RwLock (sync, not tokio) to match the service binary.
/// None = cold start / cache cleared (fail-closed in middleware).
pub type SharedPostureCache = Arc<std::sync::RwLock<Option<CachedFleetPosture>>>;

/// Shared service state threaded through all axum handlers.
///
/// Security invariant #11: All handlers MUST use State<Arc<ServiceState>>,
/// never State<Arc<AppState>>.
pub struct ServiceState {
    pub app: Arc<AppState>,
    pub posture_cache: SharedPostureCache,
}

#[cfg(test)]
mod posture_cache_tests {
    use super::*;
    use crate::verifier::FleetPosture;

    #[test]
    fn test_new_entry_is_not_stale() {
        let entry = CachedFleetPosture::new(FleetPosture::Nominal);
        assert!(!entry.is_stale(now_ms()), "brand-new entry must not be stale");
    }

    #[test]
    fn test_entry_beyond_ttl_is_stale() {
        let old_ts = now_ms().saturating_sub(POSTURE_CACHE_TTL_MS + 1);
        let entry = CachedFleetPosture {
            propagated_status: FleetPosture::Nominal,
            generated_at_ms: old_ts,
            ttl_ms: POSTURE_CACHE_TTL_MS,
            generation: 1,
        };
        assert!(entry.is_stale(now_ms()), "entry older than TTL must be stale");
    }

    #[test]
    fn test_entry_exactly_at_ttl_boundary_is_stale() {
        let boundary_ts = now_ms().saturating_sub(POSTURE_CACHE_TTL_MS);
        let entry = CachedFleetPosture {
            propagated_status: FleetPosture::Nominal,
            generated_at_ms: boundary_ts,
            ttl_ms: POSTURE_CACHE_TTL_MS,
            generation: 1,
        };
        assert!(entry.is_stale(now_ms()));
    }

    #[test]
    fn test_new_with_generation_sets_all_fields() {
        let ts = now_ms();
        let entry = CachedFleetPosture::new_with_generation(FleetPosture::Degraded, 42, ts);
        assert_eq!(entry.propagated_status, FleetPosture::Degraded);
        assert_eq!(entry.generation, 42);
        assert_eq!(entry.generated_at_ms, ts);
        assert_eq!(entry.ttl_ms, POSTURE_CACHE_TTL_MS);
    }

    #[test]
    fn test_new_convenience_constructor_uses_generation_1() {
        let entry = CachedFleetPosture::new(FleetPosture::Nominal);
        assert_eq!(entry.generation, 1);
        assert_eq!(entry.ttl_ms, POSTURE_CACHE_TTL_MS);
    }

    #[test]
    fn test_cached_posture_is_serializable() {
        let entry = CachedFleetPosture::new(FleetPosture::Nominal);
        let json = serde_json::to_string(&entry).expect("must serialize");
        let rt: CachedFleetPosture = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(entry.propagated_status, rt.propagated_status);
        assert_eq!(entry.generation, rt.generation);
    }

    #[test]
    fn test_from_posture_copies_propagated_status() {
        use crate::verifier::{FleetNodePosture, NodeTrustState};
        let fp = FleetNodePosture {
            node_id: "test".to_string(),
            local_status: NodeTrustState::Trusted,
            propagated_status: FleetPosture::Degraded,
            blocked_by: vec![],
        };
        let ts = now_ms();
        let cached = CachedFleetPosture::from_posture(&fp, ts);
        assert_eq!(cached.propagated_status, FleetPosture::Degraded);
        assert_eq!(cached.generated_at_ms, ts);
    }
}
