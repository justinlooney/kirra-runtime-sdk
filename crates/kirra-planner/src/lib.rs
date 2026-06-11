//! kirra-planner — Occy autonomy planner, **Phase-0 interface lock** (#89 / Occy 0.A).
//!
//! This crate is the **scaffold** that locks the Phase-0 planner interfaces so the
//! Occy Phase-1 chain (#90–#93, CARLA-blocked) can build against a stable shape.
//! It is **not** a real planner.
//!
//! # Derivation, not invention
//!
//! The #89 issue body predates a checker side that now fully exists on main. The
//! interfaces here are therefore **derived from current main, never copied from the
//! issue**. The load-bearing fact: the planner's job is to **propose** a trajectory
//! that the **existing checker** consumes — it does not check, and it does not
//! redefine the checker's types.
//!
//! - The checker entry is [`kirra_ros2_adapter::validate_trajectory_slow`] (the
//!   **#131** per-trajectory containment path), which consumes `&[TrajectoryPoint]`.
//!   So [`PlanOutput`] carries exactly `Vec<TrajectoryPoint>` — the same type,
//!   imported, never redefined.
//! - Posture is [`kirra_runtime_sdk::verifier::FleetPosture`].
//! - **The planner does NOT produce scenes.** Scenes are perception-side inputs
//!   (`parko_kirra::…evaluate_scene*`); the planner consumes a world-state.
//!
//! # Phase-0 finding (surfaced, not fixed)
//!
//! The checked trajectory type (`TrajectoryPoint`) and the validation entry live in
//! the `kirra-ros2-adapter` crate — a downstream integration layer. A planner
//! depending on the adapter inverts the natural direction and pulls the whole SDK +
//! adapter. **Proposal (NOT done here):** promote the trajectory contract + the
//! validation entry to a lean shared home (e.g. a `kirra-trajectory` crate, or the
//! SDK gateway) so the planner depends on the *contract*, not the integration crate.
//! Until then we **import** the real type — the held line: no parallel redefinition.

// Import (never redefine) the locked upstream types. Re-exported so a Phase-1
// consumer names them from one place — but they remain the adapter's / SDK's
// definitions.
pub use kirra_ros2_adapter::state::{Pose, TrajectoryPoint, TrajectoryVerdict};
pub use kirra_runtime_sdk::verifier::FleetPosture;

use kirra_ros2_adapter::corridor::CorridorSource;

/// Ego world-state the planner consumes.
///
/// `// PHASE-0 LOCKED` — derived from `kirra_ros2_adapter::state::EgoOdom`
/// (`linear_x_mps`, `yaw_rate_rads`, `stamp_ms`), plus the ego `pose`. The pose is
/// **integrator / localization sourced** (the SDK localization-integrity gate,
/// AOU-LOCALIZATION-001, owns its trustworthiness — not this crate).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EgoState {
    pub pose: Pose,
    pub linear_x_mps: f64,
    pub yaw_rate_rads: f64,
    pub stamp_ms: u64,
}

/// The planning goal.
///
/// `// PHASE-0 LOCKED` — Phase-0 shape is a target pose; **integrator / mission
/// sourced**. Richer goal forms (route, behavior intent) are later-slice work.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Goal {
    pub target: Pose,
}

/// World-state input to [`Planner::plan`].
///
/// `// PHASE-0 LOCKED` — derived from the checker's own consumed inputs: ego
/// state, the drivable-space handle (the **same** [`CorridorSource`] trait
/// `validate_trajectory_slow` consumes), and the fleet posture. Borrowed `map`
/// keeps it allocation-free and lets the planner and the checker read one corridor.
pub struct PlanInput<'a> {
    pub ego: EgoState,
    pub goal: Goal,
    /// Drivable-space handle — the same `CorridorSource` the checker re-reads.
    pub map: &'a dyn CorridorSource,
    /// Fleet posture → planner mode (see [`planner_mode`]).
    pub posture: FleetPosture,
}

/// Intent label on a proposal.
///
/// **AUDIT-ONLY.** Like #89's `command_source`, it MUST NOT relax the checker —
/// the checker never sees it (`validate_trajectory_slow` takes only the
/// trajectory). It records what the planner *intended*, nothing more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalKind {
    Motion,
    SafeStop,
}

/// A trajectory proposal — **exactly** the shape the #131 checker consumes.
///
/// `// PHASE-0 LOCKED` — `trajectory` is `Vec<TrajectoryPoint>`, the input type of
/// [`kirra_ros2_adapter::validate_trajectory_slow`]. No curvature / accel / metadata
/// fields are added: the checked `TrajectoryPoint` is `{pose, velocity_mps,
/// time_from_start_s}`, and the checker derives per-pose deltas itself. (The #89
/// "Trajectory {…curvature, accel, horizon, metadata}" shape is **not** the checked
/// shape — main wins; see the PR divergence table.)
#[derive(Debug, Clone, PartialEq)]
pub struct PlanOutput {
    pub trajectory: Vec<TrajectoryPoint>,
    pub kind: ProposalKind,
}

impl PlanOutput {
    // SAFETY: occy planner stop-proposal invariant | REQ: Occy-0.A (#89) | TEST: kirra_planner::tests::{safe_stop_is_valid_stop_proposal, stop_planner_output_feeds_the_checker}
    /// The always-available safe-stop / MRC proposal.
    ///
    /// `// PHASE-0 LOCKED — the stop-proposal invariant.` A planner MUST always be
    /// able to propose stopping: the checker may veto every *motion* proposal, but
    /// the architecture needs a safe-stop proposal to fall back to — **a planner
    /// with no stop output deadlocks it.** This constructor guarantees one exists.
    ///
    /// Produces ≥ 2 zero-velocity points holding `at` (the checker requires ≥ 2
    /// points; a held pose at 0 m/s is the controlled stop-and-hold).
    #[must_use]
    pub fn safe_stop(at: Pose) -> Self {
        let trajectory = vec![
            TrajectoryPoint { pose: at, velocity_mps: 0.0, time_from_start_s: 0.0 },
            TrajectoryPoint { pose: at, velocity_mps: 0.0, time_from_start_s: 0.1 },
        ];
        PlanOutput { trajectory, kind: ProposalKind::SafeStop }
    }
}

/// The planner contract.
///
/// `// PHASE-0 LOCKED` — derived from the checker consumer
/// (`validate_trajectory_slow`): a planner takes a world-state and **proposes** a
/// trajectory; the checker decides. Object-safe so Phase-1 may hold `Box<dyn
/// Planner>`.
pub trait Planner {
    fn plan(&mut self, input: &PlanInput<'_>) -> PlanOutput;
}

/// Planner operating mode, derived from fleet posture (#89 "FleetPosture →
/// planner-mode").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannerMode {
    /// `Nominal` → full planning.
    Full,
    /// `Degraded` → conservative planning.
    Conservative,
    /// `LockedOut` → MRC-only: the planner may only propose safe-stop.
    MrcOnly,
}

// PHASE-0 LOCKED — derived from kirra_runtime_sdk::verifier::FleetPosture.
/// Map fleet posture to planner mode.
#[must_use]
pub fn planner_mode(posture: FleetPosture) -> PlannerMode {
    match posture {
        FleetPosture::Nominal => PlannerMode::Full,
        FleetPosture::Degraded => PlannerMode::Conservative,
        FleetPosture::LockedOut => PlannerMode::MrcOnly,
    }
}

/// Trivial reference planner: **always** proposes safe-stop.
///
/// NOT a real planner — it exists to prove the locked interfaces are constructible
/// and consumable: it compiles against the trait, feeds the real checker, and
/// satisfies the stop-proposal invariant.
#[derive(Debug, Default, Clone, Copy)]
pub struct StopPlanner;

impl Planner for StopPlanner {
    fn plan(&mut self, input: &PlanInput<'_>) -> PlanOutput {
        // Always able to stop — holds the ego pose at zero velocity.
        PlanOutput::safe_stop(input.ego.pose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kirra_ros2_adapter::config::VehicleConfig;
    use kirra_ros2_adapter::corridor::MockCorridorSource;
    use kirra_ros2_adapter::validate_trajectory_slow;

    fn sample_input<'a>(map: &'a dyn CorridorSource) -> PlanInput<'a> {
        PlanInput {
            ego: EgoState {
                pose: Pose { x_m: 0.0, y_m: 0.0, heading_rad: 0.0 },
                linear_x_mps: 3.0,
                yaw_rate_rads: 0.0,
                stamp_ms: 0,
            },
            goal: Goal { target: Pose { x_m: 50.0, y_m: 0.0, heading_rad: 0.0 } },
            map,
            posture: FleetPosture::Nominal,
        }
    }

    #[test]
    fn safe_stop_is_valid_stop_proposal() {
        let out = PlanOutput::safe_stop(Pose { x_m: 1.0, y_m: 2.0, heading_rad: 0.0 });
        assert_eq!(out.kind, ProposalKind::SafeStop);
        assert!(out.trajectory.len() >= 2, "the checker requires >= 2 points");
        assert!(
            out.trajectory.iter().all(|p| p.velocity_mps == 0.0),
            "a safe-stop proposal is zero velocity"
        );
    }

    #[test]
    fn stop_planner_output_feeds_the_checker() {
        // Construct → feed the EXISTING #131 validation entry → no panic. This is
        // the locked shape proving its job: a planner output is consumable by the
        // real checker at the type level. Verdict content is whatever it is.
        let corridor = MockCorridorSource::straight_5m_half_width(100.0);
        let mut planner = StopPlanner;
        let out = planner.plan(&sample_input(&corridor));

        let _verdict: TrajectoryVerdict = validate_trajectory_slow(
            &out.trajectory,
            &corridor,
            &[], // no perceived objects
            &VehicleConfig::default_urban(),
            None, // no odom
            FleetPosture::Nominal,
        );
    }

    #[test]
    fn planner_is_object_safe() {
        let corridor = MockCorridorSource::straight_5m_half_width(10.0);
        let mut boxed: Box<dyn Planner> = Box::new(StopPlanner);
        let out = boxed.plan(&sample_input(&corridor));
        assert_eq!(out.kind, ProposalKind::SafeStop);
    }

    #[test]
    fn planner_mode_maps_every_posture() {
        assert_eq!(planner_mode(FleetPosture::Nominal), PlannerMode::Full);
        assert_eq!(planner_mode(FleetPosture::Degraded), PlannerMode::Conservative);
        assert_eq!(planner_mode(FleetPosture::LockedOut), PlannerMode::MrcOnly);
    }
}
