// src/posture_engine.rs
//
// Fleet posture engine: DAG traversal, generation counter, cache broadcast.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::verifier::{AppState, FleetPosture};
use crate::posture_cache::{now_ms, CachedFleetPosture, SharedPostureCache};

// Re-export so external consumers can import POSTURE_CACHE_TTL_MS from here.
pub use crate::posture_cache::POSTURE_CACHE_TTL_MS;

/// Monotonically increasing generation counter for the posture cache.
/// Initialized to 0; first emitted generation is 1.
pub static POSTURE_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Returns the next generation number (atomically incremented).
pub fn next_generation() -> u64 {
    POSTURE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

/// Derives the fleet-wide worst-case posture by traversing the full DAG.
///
/// Iterates all registered nodes, calls calculate_posture() for each,
/// and returns the most severe propagated_status observed.
pub fn derive_fleet_posture(app: &AppState) -> FleetPosture {
    let mut worst = FleetPosture::Nominal;
    for entry in app.nodes.iter() {
        let posture = app.calculate_posture(entry.key());
        match posture.propagated_status {
            FleetPosture::LockedOut => return FleetPosture::LockedOut,
            FleetPosture::Degraded => {
                worst = FleetPosture::Degraded;
            }
            FleetPosture::Nominal => {}
        }
    }
    worst
}

/// Recalculates fleet posture and atomically replaces the shared cache entry.
///
/// Called synchronously by ScenarioRunner (test mode) and via the posture
/// engine worker (production) in posture_engine_v2.rs.
pub fn recalculate_and_broadcast(app: &Arc<AppState>, cache: &SharedPostureCache) {
    let fleet_posture = derive_fleet_posture(app);
    let generation = next_generation();
    let ts = now_ms();
    let entry = CachedFleetPosture::new_with_generation(fleet_posture, generation, ts);
    if let Ok(mut guard) = cache.write() {
        *guard = Some(entry);
    }
}

#[cfg(test)]
mod posture_engine_tests {
    use super::*;

    #[test]
    fn test_posture_generation_is_monotonically_increasing() {
        let g1 = next_generation();
        let g2 = next_generation();
        let g3 = next_generation();
        assert!(g1 < g2);
        assert!(g2 < g3);
    }

    #[test]
    fn test_posture_cache_ttl_ms_is_positive() {
        assert!(POSTURE_CACHE_TTL_MS > 0);
    }

    #[test]
    fn test_recalculate_and_broadcast_writes_to_cache() {
        use std::sync::Arc;
        use crate::verifier::{AppState, VerifierOperationMode};
        use crate::verifier_store::VerifierStore;

        let store = VerifierStore::new(":memory:").unwrap();
        let app = Arc::new(AppState::new(store, VerifierOperationMode::Active));
        let cache: SharedPostureCache = Arc::new(std::sync::RwLock::new(None));

        recalculate_and_broadcast(&app, &cache);

        let guard = cache.read().unwrap();
        assert!(guard.is_some(), "cache must be populated after recalculate");
        let entry = guard.as_ref().unwrap();
        assert_eq!(entry.propagated_status, FleetPosture::Nominal);
        assert!(entry.generation > 0);
    }
}
