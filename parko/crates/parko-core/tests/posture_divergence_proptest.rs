// crates/parko-core/tests/posture_divergence_proptest.rs
//
// Property-based tests asserting that KirraGovernor always produces output
// within the Kirra profile velocity ceiling for each SafetyPosture. This is
// the core correctness invariant for the governor integration.
//
// Kirra profile ceilings (kirra_runtime_sdk::gateway::kinematics_contract):
//   Nominal  → nominal_reference_profile().max_speed_mps = 35.0
//   Degraded → mrc_fallback_profile().max_speed_mps      = 5.0
//   LockedOut → uses the same MRC fallback profile as Degraded (5.0)
//
// Implementation note: the nominal profile has a tighter acceleration rate
// limit than the fallback profile. For inputs between the two speed ceilings
// (5.0–35.0 m/s) with previous=None, nominal output may be lower than
// degraded output due to the stricter rate-of-change clamp. The properties
// below test the per-posture speed ceiling invariant rather than cross-posture
// ordering, which is not a simple monotonicity relationship.
//
// Run with: cargo test -p parko-core

use proptest::prelude::*;

use parko_kirra::KirraGovernor;
use parko_core::{
    commands::ControlCommand,
    safety::{EnforcementAction, SafetyGovernor, SafetyPosture},
};

const NOMINAL_CEILING_MPS: f64 = 35.0;
const FALLBACK_CEILING_MPS: f64 = 5.0;

/// Resolve the governor's EnforcementAction to a concrete linear velocity.
fn effective_linear_velocity(action: EnforcementAction, proposed: f64) -> f64 {
    match action {
        EnforcementAction::Allow => proposed,
        EnforcementAction::ClampLinearVelocity(v) => v,
        EnforcementAction::ClampAngularVelocity(_) => proposed,
        EnforcementAction::Deny { .. } => 0.0,
    }
}

fn evaluate_governor(proposed: f64, posture: SafetyPosture) -> f64 {
    let governor = KirraGovernor::new();
    let cmd = ControlCommand {
        linear_velocity: proposed,
        angular_velocity: 0.0,
        timestamp_ms: 0,
    };
    let action = governor.evaluate(&cmd, None, 0.05, posture);
    effective_linear_velocity(action, proposed)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    /// Nominal posture: for any positive proposed velocity, the governor output
    /// must not exceed the nominal reference profile ceiling (35.0 m/s).
    /// The nominal profile also applies rate-of-change limits; with previous=None
    /// the effective output is bounded by both the speed cap and the acceleration
    /// limit over one tick period.
    #[test]
    fn governor_never_exceeds_nominal_profile_ceiling(
        proposed in 0.0f64..=1000.0f64
    ) {
        prop_assume!(proposed.is_finite());
        let output = evaluate_governor(proposed, SafetyPosture::Nominal);
        prop_assert!(
            output <= NOMINAL_CEILING_MPS,
            "KirraGovernor Nominal output {} > ceiling {} for proposed {}",
            output, NOMINAL_CEILING_MPS, proposed
        );
    }

    /// Degraded posture: for any positive proposed velocity, the governor output
    /// must not exceed the MRC fallback profile ceiling (5.0 m/s).
    #[test]
    fn governor_never_exceeds_degraded_profile_ceiling(
        proposed in 0.0f64..=1000.0f64
    ) {
        prop_assume!(proposed.is_finite());
        let output = evaluate_governor(proposed, SafetyPosture::Degraded);
        prop_assert!(
            output <= FALLBACK_CEILING_MPS,
            "KirraGovernor Degraded output {} > ceiling {} for proposed {}",
            output, FALLBACK_CEILING_MPS, proposed
        );
    }

    /// LockedOut posture: KirraGovernor maps LockedOut to the same MRC fallback
    /// profile as Degraded. Output must not exceed the fallback ceiling (5.0 m/s).
    #[test]
    fn governor_locked_out_uses_fallback_profile_ceiling(
        proposed in 0.0f64..=1000.0f64
    ) {
        prop_assume!(proposed.is_finite());
        let output = evaluate_governor(proposed, SafetyPosture::LockedOut);
        prop_assert!(
            output <= FALLBACK_CEILING_MPS,
            "KirraGovernor LockedOut output {} > fallback ceiling {} for proposed {}",
            output, FALLBACK_CEILING_MPS, proposed
        );
    }

    /// LockedOut and Degraded share the same contract profile: for any input,
    /// both postures must produce identical outputs (same speed cap and rate
    /// limits apply). This verifies structural equivalence of the two postures
    /// at the KirraGovernor level.
    #[test]
    fn locked_out_and_degraded_produce_identical_outputs(
        proposed in 0.0f64..=1000.0f64
    ) {
        prop_assume!(proposed.is_finite());
        let degraded_out = evaluate_governor(proposed, SafetyPosture::Degraded);
        let locked_out = evaluate_governor(proposed, SafetyPosture::LockedOut);
        prop_assert_eq!(
            degraded_out, locked_out,
            "Degraded and LockedOut must use the same fallback contract (got {} vs {}) for proposed {}",
            degraded_out, locked_out, proposed
        );
    }
}
