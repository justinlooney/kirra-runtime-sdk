//! CERT-006 — durable, signed production sink for `ComparatorDivergence` events.
//!
//! [`crate::comparator::InMemoryDivergenceSink`] is ephemeral + unsigned (dev /
//! test only). A production deployment MUST wire a sink that persists every
//! divergence to a tamper-evident record — this module provides it.
//!
//! [`AuditChainLinkerDivergenceSink`] holds the SDK's [`VerifierStore`] (the
//! handle that owns the hash-chained `audit_log_chain` ledger + the Ed25519
//! signing key) and records each divergence via
//! [`VerifierStore::save_posture_event_chained`], which appends through
//! `AuditChainLinker::append_audit_event_tx` — the same signed, hash-linked
//! ledger the verifier service writes — with event type `"ComparatorDivergence"`
//! and the JSON-serialised [`DivergenceEvent`] as the body.
//!
//! NOTE: `save_posture_event_chained` also writes a `posture_events` row in the
//! same transaction; that row is incidental — the authoritative, contract-
//! specified artifact is the signed `audit_log_chain` entry.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use kirra_runtime_sdk::verifier_store::VerifierStore;

use crate::comparator::{DivergenceEvent, DivergenceEventSink};

/// The audit-log event type for a comparator divergence (the doc-spec name).
pub const COMPARATOR_DIVERGENCE_EVENT_TYPE: &str = "ComparatorDivergence";

/// Durable, signed [`DivergenceEventSink`] (CERT-006).
///
/// Persists every divergence to the SDK's hash-chained, Ed25519-signed audit
/// ledger. `record` is infallible by the trait contract — but a divergence that
/// is *detected yet not durably recorded* is itself safety-relevant, so a
/// persistence failure is never silently swallowed: it increments the
/// operator-observable [`write_failures`](Self::write_failures) counter and logs
/// loudly to stderr (matching the in-crate sink convention).
pub struct AuditChainLinkerDivergenceSink {
    store: Arc<Mutex<VerifierStore>>,
    write_failures: AtomicU64,
}

impl AuditChainLinkerDivergenceSink {
    /// Build a sink over an SDK store. The store MUST own the audit chain (it
    /// does — `VerifierStore::new` creates `audit_log_chain`) and a signing key
    /// (set via `VerifierStore::set_signing_key` / `admit_signing_key`) for the
    /// entries to be signed.
    pub fn new(store: Arc<Mutex<VerifierStore>>) -> Self {
        Self {
            store,
            write_failures: AtomicU64::new(0),
        }
    }

    /// Number of divergences that were DETECTED but could NOT be durably +
    /// signed. MUST be `0` in a healthy deployment; a non-zero value means the
    /// tamper-evident record is MISSING for that many divergences — observe it.
    pub fn write_failures(&self) -> u64 {
        self.write_failures.load(Ordering::SeqCst)
    }

    fn note_failure(&self) {
        self.write_failures.fetch_add(1, Ordering::SeqCst);
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

impl DivergenceEventSink for AuditChainLinkerDivergenceSink {
    fn record(&self, event: DivergenceEvent) {
        let body = match serde_json::to_string(&event) {
            Ok(s) => s,
            Err(e) => {
                self.note_failure();
                eprintln!(
                    "[CERT-006] ComparatorDivergence NOT recorded — JSON serialization failed: \
                     {e} (divergence is UNAUDITED)"
                );
                return;
            }
        };

        let outcome = match self.store.lock() {
            Ok(mut store) => store.save_posture_event_chained(
                "governor_comparator",
                COMPARATOR_DIVERGENCE_EVENT_TYPE,
                &body,
                None,
                Self::now_ms(),
            ),
            Err(_) => {
                self.note_failure();
                eprintln!(
                    "[CERT-006] ComparatorDivergence NOT recorded — audit store mutex poisoned \
                     (divergence is UNAUDITED)"
                );
                return;
            }
        };

        if let Err(e) = outcome {
            self.note_failure();
            eprintln!(
                "[CERT-006] AUDIT-CHAIN WRITE FAILED for ComparatorDivergence: {e} — \
                 divergence detected but NOT in the tamper-evident log"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn sample_event() -> DivergenceEvent {
        DivergenceEvent {
            primary_lin: 3.0,
            shadow_lin: 0.0,
            delta_lin: 3.0,
            primary_ang: 0.1,
            shadow_ang: 0.0,
            delta_ang: 0.1,
            accumulator: 7,
            current_speed_mps: Some(2.5),
            reconciled_lin: 0.0,
            reconciled_ang: 0.0,
            escalated_to_lockout: true,
        }
    }

    /// TASK 2 — a real signing key + file-backed audit chain: the recorded
    /// divergence is DURABLE, hash-linked, and its signature VERIFIES (distinct
    /// from the in-memory emission test, which only proves buffering).
    #[test]
    fn divergence_is_durably_recorded_signed_and_hash_linked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("divergence_audit.sqlite");
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let vk = key.verifying_key();

        let mut store = VerifierStore::new(db.to_str().unwrap()).expect("store");
        store.set_signing_key(key);
        let store = Arc::new(Mutex::new(store));

        let sink = AuditChainLinkerDivergenceSink::new(Arc::clone(&store));
        sink.record(sample_event());
        assert_eq!(sink.write_failures(), 0, "the divergence must have been durably recorded");

        let guard = store.lock().unwrap();

        // Durable + hash-linked + SIGNED (verifies under the real key).
        let v = guard.verify_audit_chain_full(Some(&vk)).expect("verify");
        assert!(v.chain_intact, "audit chain must be hash-intact");
        assert!(v.signature_valid, "the signature must verify under the signing key");
        assert!(v.signed_entries >= 1, "the divergence entry must be signed, got {}", v.signed_entries);

        // The entry is a `ComparatorDivergence` carrying the event body.
        let events = guard.load_all_posture_events().expect("load events");
        let div = events
            .iter()
            .find(|e| e["event_type"] == COMPARATOR_DIVERGENCE_EVENT_TYPE)
            .expect("a ComparatorDivergence audit entry must exist");
        assert_eq!(div["posture"]["escalated_to_lockout"], true);
        assert_eq!(div["posture"]["accumulator"], 7);
    }

    /// A persistence failure (poisoned store) is surfaced via `write_failures`,
    /// never silently swallowed — a detected-but-unaudited divergence is itself
    /// safety-relevant.
    #[test]
    fn persistence_failure_is_surfaced_not_swallowed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("divergence_audit.sqlite");
        let store = Arc::new(Mutex::new(VerifierStore::new(db.to_str().unwrap()).expect("store")));

        // Poison the store mutex so the audit write cannot land.
        let s = Arc::clone(&store);
        let _ = std::thread::spawn(move || {
            let _g = s.lock().unwrap();
            panic!("poison the audit store for the failure test");
        })
        .join();

        let sink = AuditChainLinkerDivergenceSink::new(Arc::clone(&store));
        sink.record(sample_event());
        assert_eq!(
            sink.write_failures(),
            1,
            "a divergence that could not be durably recorded MUST be counted, not swallowed"
        );
    }
}
