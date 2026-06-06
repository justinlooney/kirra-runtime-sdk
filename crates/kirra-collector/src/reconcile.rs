// crates/kirra-collector/src/reconcile.rs
//
// The reconciliation report (docs/COLLECTOR_DESIGN.md §4): the audit that the
// dataset faithfully represents the run. It accounts for every record — in,
// kept after sampling, joined, orphaned — and the applied pass rate, so a bench
// session can be validated ("every clamp/deny/MRC present; pass count matches
// the sampling rate; join hit-rate reported; orphans flagged"). If the orphan
// rate exceeds the configured ceiling, the run fails loud rather than silently
// emitting a half-joined dataset.

use serde::Serialize;

/// A full accounting of one collector run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reconciliation {
    /// Total deduplicated capture records read.
    pub records_in: usize,
    /// Duplicate `(source, decision_seq)` lines dropped at ingest.
    pub duplicates_dropped: usize,
    pub records_in_command_gateway: usize,
    pub records_in_slow_loop_trajectory: usize,
    /// Records that are interventions (outcome != ALLOW) — never sampled out.
    pub interventions_in: usize,
    /// Records that are passes (outcome == ALLOW) — subject to sampling.
    pub passes_in: usize,
    /// Records kept after stratified sampling (interventions + sampled passes).
    pub kept_after_sampling: usize,
    pub interventions_kept: usize,
    pub passes_kept: usize,
    /// Kept records that joined to a bus message.
    pub joined: usize,
    /// Kept records with no bus match in the window.
    pub orphans: usize,
    /// The pass rate actually applied.
    pub applied_pass_rate: f64,
    /// `orphans / kept_after_sampling` (0.0 when nothing was kept).
    pub orphan_rate: f64,
}

impl Reconciliation {
    #[must_use]
    pub fn orphan_rate(&self) -> f64 {
        if self.kept_after_sampling == 0 {
            0.0
        } else {
            self.orphans as f64 / self.kept_after_sampling as f64
        }
    }

    /// Human-readable one-block summary for the CLI.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "reconciliation:\n  \
             records_in            = {} (gateway {}, trajectory {}; {} duplicate(s) dropped)\n  \
             interventions / passes= {} / {}\n  \
             kept_after_sampling   = {} (interventions {}, passes {}; pass_rate {:.3})\n  \
             joined / orphans      = {} / {} (orphan_rate {:.3})",
            self.records_in,
            self.records_in_command_gateway,
            self.records_in_slow_loop_trajectory,
            self.duplicates_dropped,
            self.interventions_in,
            self.passes_in,
            self.kept_after_sampling,
            self.interventions_kept,
            self.passes_kept,
            self.applied_pass_rate,
            self.joined,
            self.orphans,
            self.orphan_rate,
        )
    }
}
