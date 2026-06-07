// parko/crates/parko-kirra/src/diverse.rs
//
// L2 / CERT-006 — diverse second governor for the `GovernorComparator`
// dual-channel safety architecture.
//
// **Status:** DRAFT diverse implementation + diversity argument
// pending formal safety-engineer review. See
// `docs/safety/COMPARATOR_DIVERSITY.md` for the diversity argument,
// the fault class it covers, and (importantly) what it does NOT
// cover.
//
// **Why this exists.** Before L2 the `GovernorComparator` ran
// IDENTICAL redundancy — two `KirraGovernor` instances built from
// the same config. That catches random / transient faults and
// state divergence, but a **systematic implementation bug** in
// `KirraGovernor` would manifest identically in both copies; the
// comparator would see agreement and pass the wrong answer through.
// `DiverseKirraGovernor` is a structurally different second
// implementation that enforces the SAME safety properties via
// genuinely different computation, so an implementation-level
// systematic bug in one is unlikely to appear identically in the
// other.
//
// **What this CAN catch.** Implementation-level systematic faults —
// algebra errors, off-by-one boundary mistakes, wrong-sign
// arithmetic, mis-dispatched check ordering, dropped clamp returns.
// Most real-world coding bugs land here.
//
// **What this CANNOT catch.** Spec-level systematic faults shared by
// both implementations — if the safety spec says "clamp at 10 m/s"
// and the real-world correct answer is "clamp at 8 m/s", both
// governors will dutifully clamp at the wrong (but spec-conformant)
// 10 m/s. Spec-level coverage requires the full N-version
// alternative (clean-room reimplementation from the spec only), out
// of scope for L2.
//
// **Structural differences from `KirraGovernor` (concrete):**
//
//   1. **Verdict-flag composition vs. early-return cascade.**
//      `KirraGovernor::evaluate` is a cascade of early-returns
//      (LockedOut → RSS → posture match → linear → angular).
//      `DiverseKirraGovernor::evaluate` computes per-axis
//      `Verdict` flags (Allow / Clamp(value) / Deny) first, then
//      composes them into a single `EnforcementAction` at the end.
//      Different control flow; different surface area for a
//      misplaced `return`.
//
//   2. **Linear axis: inline, not delegated.**
//      `KirraGovernor` calls
//      `kirra_runtime_sdk::validate_vehicle_command` (P0-P6 priority
//      pipeline in the SDK). `DiverseKirraGovernor` reimplements
//      the same priorities INLINE here using different algebra:
//        - effective max speed via two `f64::min` calls rather than
//          the SDK's `match`-on-`Option`;
//        - acceleration check via the multiplicative form
//          `dv >= a_max · dt + ε` (no division), avoiding the SDK's
//          `dv / dt > a_max` division-based form;
//        - clamping via `safe = v_min.copysign(v_proposed)` instead
//          of the SDK's `v_max * v.signum()` pattern;
//        - the NaN guard uses `is_nan() || is_infinite()` (two
//          predicates) instead of the SDK's `!is_finite()` (one
//          negated predicate).
//
//   3. **Angular axis: inline ω_max, different fold.**
//      `KirraGovernor` calls
//      `AngularVelocityBound::omega_max(v)` (chained `.min().min()`).
//      `DiverseKirraGovernor` recomputes the bound INLINE using
//      `[rollover, sweep, ftti].iter().fold(f64::INFINITY, |a, &b| a.min(b))`
//      and rearranged algebra:
//        - rollover via `(g / v) * (t / (2.0 * h))` instead of
//          `g * t / (2.0 * h * v)`;
//        - sweep via `v_edge * r_extent.recip()` instead of
//          `v_edge / r_extent`;
//        - the v=0 floor check uses `<` instead of the kernel's `<`
//          inverse (`>=`) so the boundary disposition is computed
//          differently even though the value is identical.
//
//   4. **MRC posture: same-spec, different code path.**
//      `KirraGovernor::apply_mrc_profile` is a single function with
//      both axes done together. `DiverseKirraGovernor::diverse_mrc`
//      computes the linear and angular axes via two independent
//      helpers and joins them with a `match` on the per-axis flag
//      tuple — different decomposition.

use parko_core::commands::ControlCommand;
use parko_core::safety::{EnforcementAction, SafetyGovernor, SafetyPosture};
use parko_core::RssState;

use kirra_runtime_sdk::gateway::kinematics_contract::VehicleKinematicsContract;
use kirra_runtime_sdk::verifier::FleetPosture;

use crate::angular_bound::{AngularVelocityBound, PlatformParams};
use crate::MRC_VELOCITY_CEILING_MPS;

/// Per-axis verdict the diverse governor accumulates before composing
/// the final `EnforcementAction`. Different intermediate type than
/// `KirraGovernor` uses (it composes directly via early-returns), so
/// a bug in either path would produce a different artifact.
#[derive(Debug, Clone, PartialEq)]
enum AxisVerdict {
    Allow,
    Clamp(f64),
    Deny(String),
}

/// Diverse second governor. Same `PlatformParams` + same
/// `VehicleKinematicsContract` as the primary (so the enforced
/// limits are identical); the COMPUTATION is structurally different.
pub struct DiverseKirraGovernor {
    contract: VehicleKinematicsContract,
    nominal_angular: AngularVelocityBound,
    mrc_angular: AngularVelocityBound,
    rss_state: RssState,
}

impl DiverseKirraGovernor {
    /// Construct from explicit config. Holds the same contract +
    /// platform params as the primary it shadows so the SPEC under
    /// enforcement is identical.
    pub fn new(contract: VehicleKinematicsContract, params: PlatformParams) -> Self {
        let nominal_angular = AngularVelocityBound::nominal(params.clone());
        let mrc_angular     = AngularVelocityBound::mrc(params);
        Self {
            contract,
            nominal_angular,
            mrc_angular,
            rss_state: RssState {
                safe: true,
                longitudinal_margin: f64::MAX,
                lateral_margin: f64::MAX,
            },
        }
    }

    /// Reference constructor — mirrors `KirraGovernor::new()` defaults
    /// (nominal Kirra vehicle contract + conservative platform
    /// params). Use for the comparator's diverse-shadow path when no
    /// integrator config has been supplied.
    pub fn new_default() -> Self {
        Self::new(
            VehicleKinematicsContract::nominal_reference_profile(),
            PlatformParams::conservative_default(),
        )
    }

    pub fn update_rss_state(&mut self, state: RssState) {
        self.rss_state = state;
    }

    /// Backward-compat posture-based constructor mirroring
    /// `KirraGovernor::for_posture`. Kept for API parity.
    pub fn for_posture(posture: FleetPosture) -> Self {
        let _ = posture; // diverse doesn't carry a per-posture profile field
        Self::new_default()
    }

    // -- linear axis (inline; does NOT call validate_vehicle_command) --

    /// Effective max speed = `min(max_speed, odd_cap)` if cap present,
    /// else `max_speed`. Diverse form: two `f64::min` calls, no
    /// `Option` `match` (the SDK's primary path matches on the
    /// `Option<f64>`).
    #[inline]
    fn effective_max_speed(&self) -> f64 {
        let cap = self.contract.odd_speed_cap_mps.unwrap_or(f64::INFINITY);
        self.contract.max_speed_mps.min(cap)
    }

    /// Inline P0-P4 linear-axis verdict. Skip P5 / P6 entirely —
    /// the parko bridge always hands steering = 0 to the primary, so
    /// those checks never fire; the diverse impl drops them rather
    /// than computing zeros.
    ///
    /// Diversity-relevant forms:
    /// - NaN guard via `is_nan() || is_infinite()` rather than
    ///   `!is_finite()`.
    /// - Acceleration check `dv >= a_max·dt + ε` (multiplicative)
    ///   rather than `dv/dt > a_max + ε` (divisive).
    /// - Clamp value via `v_max.copysign(v_proposed)` rather than
    ///   `v_max * v.signum()`.
    fn diverse_linear_check(
        &self,
        proposed: f64,
        previous: f64,
        dt: f64,
    ) -> AxisVerdict {
        // P0 — NaN/Inf guard (two predicates, not negated `is_finite`)
        if proposed.is_nan() || proposed.is_infinite() {
            return AxisVerdict::Deny(
                "NAN_INF_LINEAR_VELOCITY".to_string()
            );
        }
        if previous.is_nan() || previous.is_infinite() {
            return AxisVerdict::Deny(
                "NAN_INF_CURRENT_VELOCITY".to_string()
            );
        }
        if dt.is_nan() || dt.is_infinite() {
            return AxisVerdict::Deny(
                "NAN_INF_DELTA_TIME".to_string()
            );
        }
        // P1 — non-positive dt (inverted comparison form vs SDK's `<= 0.0`)
        if !(dt > 0.0) {
            return AxisVerdict::Deny("INVALID_TIME_DELTA".to_string());
        }

        // P2 — velocity hard ceiling. Diverse form: clamp via `copysign`.
        let v_max = self.effective_max_speed();
        let v_abs = proposed.abs();
        if v_abs > v_max {
            let safe = v_max.copysign(proposed);
            return AxisVerdict::Clamp(safe);
        }

        // P3/P4 — implied acceleration / brake ceiling, multiplicative form.
        let dv = proposed - previous;
        let abs_dv = dv.abs();
        // accel: proposed > previous, dv > 0
        if dv > 0.0 {
            let a_budget = self.contract.max_accel_mps2 * dt + 1e-9;
            if abs_dv > a_budget {
                let safe_raw = previous + self.contract.max_accel_mps2 * dt;
                // Re-clamp within ±v_max (same direction as SDK's clamp)
                let safe = safe_raw.clamp(-v_max, v_max);
                return AxisVerdict::Clamp(safe);
            }
        } else if dv < 0.0 {
            let b_budget = self.contract.max_brake_mps2 * dt + 1e-9;
            if abs_dv > b_budget {
                let safe_raw = previous - self.contract.max_brake_mps2 * dt;
                let safe = safe_raw.clamp(-v_max, v_max);
                return AxisVerdict::Clamp(safe);
            }
        }

        AxisVerdict::Allow
    }

    // -- angular axis (inline ω_max, does NOT call AngularVelocityBound::omega_max) --

    /// Recompute ω_max(v) inline using rearranged algebra + a fold
    /// instead of chained `.min().min()`. Returns the bound the
    /// diverse path enforces.
    fn diverse_omega_max(&self, bound: &AngularVelocityBound, v_abs: f64) -> f64 {
        // We have to know the platform params to recompute. Pattern-
        // match — back-compat Scalar variant returns the constant
        // directly.
        let (params, posture_factor) = match bound {
            AngularVelocityBound::Scalar(c) => return *c,
            AngularVelocityBound::Derived { params, posture_factor } => (params, *posture_factor),
        };

        // (a) Rollover — diverse algebra: (g/v) * (t / (2h)). Same value
        // as the primary's (g·t) / (2·h·v); different float
        // intermediate. Floor check inverted: primary uses
        // `v >= FLOOR` to GUARD the formula; diverse uses
        // `v < FLOOR` to MASK it.
        const G: f64 = 9.81;
        const ROLLOVER_FLOOR: f64 = 0.05; // Matches the kernel's ROLLOVER_MIN_LINEAR_VELOCITY_MPS;
                                          // duplicated here intentionally to avoid coupling
                                          // the diverse code to the primary's constants.
        let omega_rollover = if v_abs < ROLLOVER_FLOOR {
            f64::INFINITY
        } else {
            (G / v_abs) * (params.track_width_m / (2.0 * params.cog_height_m))
        };

        // (b) Sweep — diverse algebra: v_edge * r_extent.recip()
        let v_edge_eff = params.v_edge_safe_mps * posture_factor;
        let omega_sweep = v_edge_eff * params.robot_extent_m.recip();

        // (c) FTTI — algebraically the same; the fold below is the
        // composition difference.
        let theta_eff = params.theta_max_rad * posture_factor;
        let omega_ftti = theta_eff / params.ftti_s;

        // Compose via fold over an array rather than chained min.
        [omega_rollover, omega_sweep, omega_ftti]
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min)
    }

    /// Apply ω_max to a proposed angular velocity. Returns
    /// `Allow` or `Clamp(value)`. `Deny` is not possible on the
    /// angular axis — the bound is a clamp, not a hard-stop.
    fn diverse_angular_check(
        &self,
        proposed_angular: f64,
        linear_for_bound: f64,
        bound: &AngularVelocityBound,
    ) -> AxisVerdict {
        let omega_max = self.diverse_omega_max(bound, linear_for_bound.abs());
        let mag = proposed_angular.abs();
        if mag > omega_max {
            // Clamp via copysign (same idiom we used on linear).
            return AxisVerdict::Clamp(omega_max.copysign(proposed_angular));
        }
        AxisVerdict::Allow
    }

    /// MRC profile (Degraded / RSS-unsafe). Diverse composition: two
    /// independent helpers + a tuple match, rather than the primary's
    /// fused both-axes function.
    fn diverse_mrc(&self, proposed: &ControlCommand) -> EnforcementAction {
        // Linear: cap at MRC ceiling. Use direct `if` rather than
        // the primary's `.min(...)`.
        let lin_clamped_value = if proposed.linear_velocity > MRC_VELOCITY_CEILING_MPS {
            Some(MRC_VELOCITY_CEILING_MPS)
        } else {
            None
        };

        // Angular: use the MRC bound (already posture-derated inside
        // the bound).
        let ang = self.diverse_angular_check(
            proposed.angular_velocity,
            // For MRC the linear value passed to ω_max is the
            // post-clamp value (consistent with primary's
            // apply_mrc_profile).
            lin_clamped_value.unwrap_or(proposed.linear_velocity).abs(),
            &self.mrc_angular,
        );
        let ang_value = match ang {
            AxisVerdict::Clamp(v) => Some(v),
            _ => None,
        };

        // Compose via tuple match.
        match (lin_clamped_value, ang_value) {
            (None,    None)    => EnforcementAction::Allow,
            (Some(l), None)    => EnforcementAction::ClampLinearVelocity(l),
            (None,    Some(a)) => EnforcementAction::ClampAngularVelocity(a),
            (Some(l), Some(a)) => EnforcementAction::ClampMotion {
                linear:  Some(l),
                angular: Some(a),
            },
        }
    }

    /// Compose linear + angular verdicts into a single
    /// `EnforcementAction`. Diverse from the primary's nested-match
    /// composition: flat 4-way `match` on a tuple of bool flags + the
    /// per-axis clamp values.
    fn compose(
        linear: AxisVerdict,
        angular: AxisVerdict,
    ) -> EnforcementAction {
        // Deny on linear dominates — same spec as primary, different
        // code path (primary uses early-return inside the verdict
        // mapper; diverse hoists the Deny check to the top here).
        if let AxisVerdict::Deny(reason) = &linear {
            return EnforcementAction::Deny { reason: reason.clone() };
        }
        let lin_v = match linear {
            AxisVerdict::Clamp(v) => Some(v),
            _ => None,
        };
        let ang_v = match angular {
            AxisVerdict::Clamp(v) => Some(v),
            _ => None,
        };
        match (lin_v.is_some(), ang_v.is_some()) {
            (false, false) => EnforcementAction::Allow,
            (true,  false) => EnforcementAction::ClampLinearVelocity(lin_v.unwrap()),
            (false, true ) => EnforcementAction::ClampAngularVelocity(ang_v.unwrap()),
            (true,  true ) => EnforcementAction::ClampMotion {
                linear:  lin_v,
                angular: ang_v,
            },
        }
    }
}

impl SafetyGovernor for DiverseKirraGovernor {
    fn evaluate(
        &self,
        proposed: &ControlCommand,
        previous: Option<&ControlCommand>,
        delta_time_s: f64,
        posture: SafetyPosture,
    ) -> EnforcementAction {
        // Diverse posture dispatch. Primary uses sequential
        // `if posture == LockedOut { return }` then `if !rss.safe { return mrc }`
        // then `match posture { ... }`. Diverse: single `match` on a
        // synthetic (posture, rss-safe) tuple. Same semantics; the
        // intermediate dispatch surface area differs.
        let key = (posture, self.rss_state.safe);
        match key {
            (SafetyPosture::LockedOut, _) => EnforcementAction::Deny {
                reason: "LockedOut: hard stop".to_string(),
            },
            // RSS-unsafe → MRC profile, regardless of nominal/degraded.
            // (Same behaviour as primary's "RSS gate second" rule.)
            (SafetyPosture::Nominal,  false) |
            (SafetyPosture::Degraded, _    ) => self.diverse_mrc(proposed),

            (SafetyPosture::Nominal, true) => {
                // Nominal path with safe RSS: full envelope.
                let prev = previous.map(|p| p.linear_velocity).unwrap_or(0.0);
                let linear = self.diverse_linear_check(
                    proposed.linear_velocity, prev, delta_time_s);

                // Angular: only run when linear didn't Deny — but we
                // ALWAYS compute the angular verdict so a hard Deny
                // on linear and an angular Clamp can't accidentally
                // be hidden by an early-return.
                let angular = self.diverse_angular_check(
                    proposed.angular_velocity,
                    proposed.linear_velocity,
                    &self.nominal_angular,
                );

                Self::compose(linear, angular)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! These tests cover the diverse governor in isolation. The
    //! cross-governor AGREEMENT and DETECTION tests live alongside
    //! the comparator tests (`crate::comparator::diversity_tests`).

    use super::*;

    fn cmd(linear: f64, angular: f64) -> ControlCommand {
        ControlCommand { linear_velocity: linear, angular_velocity: angular, timestamp_ms: 0 }
    }

    #[test]
    fn diverse_locked_out_returns_deny() {
        let g = DiverseKirraGovernor::new_default();
        let action = g.evaluate(&cmd(2.0, 0.5), None, 0.05, SafetyPosture::LockedOut);
        assert!(matches!(action, EnforcementAction::Deny { .. }));
    }

    #[test]
    fn diverse_nominal_in_envelope_returns_allow() {
        let g = DiverseKirraGovernor::new_default();
        // 0.1 m/s, 0.0 rad/s — tiny inputs the conservative default
        // accepts in full envelope.
        let prev = cmd(0.1, 0.0);
        let action = g.evaluate(&cmd(0.1, 0.0), Some(&prev), 0.05, SafetyPosture::Nominal);
        assert!(matches!(action, EnforcementAction::Allow));
    }

    #[test]
    fn diverse_nan_linear_returns_deny() {
        let g = DiverseKirraGovernor::new_default();
        let action = g.evaluate(&cmd(f64::NAN, 0.0), None, 0.05, SafetyPosture::Nominal);
        match action {
            EnforcementAction::Deny { reason } => {
                assert_eq!(reason, "NAN_INF_LINEAR_VELOCITY");
            }
            other => panic!("expected Deny on NaN linear; got {other:?}"),
        }
    }

    #[test]
    fn diverse_negative_dt_returns_deny() {
        let g = DiverseKirraGovernor::new_default();
        let action = g.evaluate(&cmd(1.0, 0.0), None, -0.01, SafetyPosture::Nominal);
        match action {
            EnforcementAction::Deny { reason } => {
                assert_eq!(reason, "INVALID_TIME_DELTA");
            }
            other => panic!("expected Deny on negative dt; got {other:?}"),
        }
    }

    #[test]
    fn diverse_omega_max_at_v_zero_is_finite() {
        let g = DiverseKirraGovernor::new_default();
        // The diverse v=0 path must NOT divide by zero — must produce
        // a finite bound dominated by sweep/FTTI.
        let omega = g.diverse_omega_max(&g.nominal_angular, 0.0);
        assert!(omega.is_finite() && omega > 0.0,
            "diverse ω_max(0) must be finite and positive; got {omega}");
    }
}
