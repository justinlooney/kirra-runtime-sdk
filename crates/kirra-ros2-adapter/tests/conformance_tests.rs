// crates/kirra-ros2-adapter/tests/conformance_tests.rs
//
// S131 Phase 3 — fast-loop conformance integration tests.
//
// Each test constructs an `AcceptedTrajectory` directly (the slow loop
// is exercised separately in `validation_tests.rs`), then asks the
// conformance check for a verdict. No ROS, no spawned tasks — the
// conformance check is sync and pure.

use kirra_ros2_adapter::{
    config::VehicleConfig,
    state::{AcceptedTrajectory, EgoOdom, Pose, TrajectoryPoint, TrajectoryVerdict, DEFAULT_MAX_AGE_MS},
    validation::{check_command_conforms, ConformanceVerdict, IncomingControl},
};

fn straight_pts(n: usize, v: f64, dt: f64) -> Vec<TrajectoryPoint> {
    (0..n).map(|i| TrajectoryPoint {
        pose: Pose { x_m: (i as f64) * v * dt, y_m: 0.0, heading_rad: 0.0 },
        velocity_mps: v,
        time_from_start_s: (i as f64) * dt,
    }).collect()
}

fn fresh_accepted(promoted_at_ms: u64, pts: Vec<TrajectoryPoint>) -> AcceptedTrajectory {
    AcceptedTrajectory::with_verdict(
        "av_01", 1, pts, TrajectoryVerdict::Accept, promoted_at_ms,
    )
}

// ---------------------------------------------------------------------------
// 1. Conforming command → Accept
// ---------------------------------------------------------------------------

#[test]
fn test_conforming_command_passes() {
    // Trajectory promoted 50 ms ago at 5 m/s for 1 s. Command velocity
    // 5.0 m/s (== nearest), steering 0 → conforms.
    let promoted = 100_000;
    let now = promoted + 50;  // 50 ms after promotion
    let traj = fresh_accepted(promoted, straight_pts(10, 5.0, 0.1));
    let cmd = IncomingControl { velocity_mps: 5.0, steering_rad: 0.0, stamp_ms: now };
    let cfg = VehicleConfig::default_urban();
    let ego = EgoOdom { linear_x_mps: 5.0, yaw_rate_rads: 0.0, stamp_ms: now };

    let start = std::time::Instant::now();
    let verdict = check_command_conforms(&cmd, &traj, &ego, &cfg, now);
    let elapsed_us = start.elapsed().as_micros();
    eprintln!("conforming_command_passes elapsed_us = {elapsed_us}");

    assert_eq!(verdict, ConformanceVerdict::Accept,
        "conforming command (cmd.v = nearest.v, steering in range, fresh trajectory) \
         must Accept; got {verdict:?}");
}

// ---------------------------------------------------------------------------
// 2. Overspeed command → MRCFallback
// ---------------------------------------------------------------------------

#[test]
fn test_overspeed_command_mrcs() {
    let promoted = 100_000;
    let now = promoted + 50;
    let traj = fresh_accepted(promoted, straight_pts(10, 5.0, 0.1));
    // VELOCITY_TOLERANCE_MPS = 0.5 → 5.6 is 0.1 m/s past the tolerance.
    let cmd = IncomingControl { velocity_mps: 5.6, steering_rad: 0.0, stamp_ms: now };
    let cfg = VehicleConfig::default_urban();
    let ego = EgoOdom::default();

    let verdict = check_command_conforms(&cmd, &traj, &ego, &cfg, now);
    assert_eq!(verdict, ConformanceVerdict::MRCFallback,
        "command velocity 5.6 m/s > nearest.v (5.0) + tolerance (0.5) must MRC; got {verdict:?}");
}

// ---------------------------------------------------------------------------
// 3. Stale trajectory → MRCFallback
// ---------------------------------------------------------------------------

#[test]
fn test_stale_trajectory_mrcs() {
    let promoted = 100_000;
    // now is past promoted + DEFAULT_MAX_AGE_MS (200 ms) → stale.
    let now = promoted + DEFAULT_MAX_AGE_MS + 50;
    let traj = fresh_accepted(promoted, straight_pts(10, 5.0, 0.1));
    let cmd = IncomingControl { velocity_mps: 5.0, steering_rad: 0.0, stamp_ms: now };
    let cfg = VehicleConfig::default_urban();
    let ego = EgoOdom::default();

    let verdict = check_command_conforms(&cmd, &traj, &ego, &cfg, now);
    assert_eq!(verdict, ConformanceVerdict::MRCFallback,
        "trajectory aged past DEFAULT_MAX_AGE_MS must MRC even on a conforming command; \
         got {verdict:?}");
}

// ---------------------------------------------------------------------------
// 4. No trajectory installed → MRCFallback (driven through AdaptorState)
// ---------------------------------------------------------------------------

#[test]
fn test_no_trajectory_mrcs() {
    use kirra_ros2_adapter::state::AdaptorState;

    // Build an AdaptorState with no trajectory for the asset. The fast
    // loop's "no trajectory installed" branch is the same as the
    // AdaptorState::snapshot returning None → caller emits MRC. We
    // exercise that path directly (snapshot returns None → MRC).
    let state = AdaptorState::new();
    let snap = state.snapshot("ghost_av");
    assert!(snap.is_none(),
        "AdaptorState with no install must return None for unknown asset");

    // current_verdict (the fast-loop's other entry point) also collapses
    // to MRCFallback per the Phase 1 contract.
    let now = 100_000;
    let verdict = state.current_verdict("ghost_av", now);
    assert_eq!(verdict, TrajectoryVerdict::MRCFallback,
        "AdaptorState::current_verdict on unknown asset must be MRCFallback; got {verdict:?}");
}
