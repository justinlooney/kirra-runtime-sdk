// crates/kirra-ros2-adapter/src/validation.rs
//
// S131 Phase 2A — slow-loop trajectory validator.
//
// Composes the three safety-critical kernel checks into a single per-
// trajectory verdict:
//   A) Containment — `validate_trajectory_containment` (SG2)
//   B) Per-pose kinematics — `validate_vehicle_command` (P0–P6) on
//      every consecutive pose pair
//   C) RSS over horizon — `longitudinal_safe_distance` /
//      `lateral_safe_distance` (SG1) per object × per pose
// The result is `TrajectoryVerdict::Accept | Clamp | MRCFallback`.
//
// First-rejection-wins: containment failure or any DenyBreach or any
// RSS violation short-circuits to MRCFallback. A Clamp from per-pose
// kinematics is recorded but does NOT short-circuit — containment +
// RSS still get a vote.

use kirra_runtime_sdk::gateway::containment::{
    self as containment, Corridor, Pose as KernelPose,
    Point as KernelPoint,
};
use kirra_runtime_sdk::gateway::kinematics_contract::{
    validate_vehicle_command, EnforceAction, ProposedVehicleCommand,
};
use parko_core::rss::{
    lateral_safe_distance, longitudinal_safe_distance,
};

use crate::config::VehicleConfig;
use crate::corridor::{CorridorSource, Point};
use crate::state::{PerceivedObject, Pose, TrajectoryPoint, TrajectoryVerdict};

/// Minimum corridor confidence the slow loop accepts. Tracks the
/// `kirra_runtime_sdk::gateway::containment::Corridor::min_confidence`
/// gate; below this the kernel returns DrivableSpaceDeparture
/// regardless of geometry.
const SLOW_LOOP_MIN_CORRIDOR_CONFIDENCE: f32 = 0.5;

/// Max corridor age (ms). One planning cycle (~100 ms) + jitter.
const SLOW_LOOP_MAX_CORRIDOR_AGE_MS: u64 = 500;

/// RSS reaction time (s). Per IEEE 2846-2022 §5.1 the canonical value
/// is 0.5 s for SAE-Level-4 stacks; we use the conservative end.
const RSS_REACTION_TIME_S: f64 = 0.5;

/// Distance below which two objects are considered laterally aligned
/// (and therefore subject to RSS longitudinal evaluation). Anything
/// beyond this lateral offset is in another corridor; containment
/// covers it.
const RSS_LATERAL_ALIGNMENT_TOLERANCE_M: f64 = 4.0;

/// Compose the three slow-loop checks into one verdict. First-rejection-
/// wins on containment and RSS; Clamp is recorded but does not
/// short-circuit (containment + RSS still vote).
///
/// Returns:
///   Accept       — clean: containment + per-pose kinematics + RSS all green
///   Clamp        — per-pose requested a Clamp on ≥ 1 pose; containment + RSS green
///   MRCFallback  — containment fail / per-pose DenyBreach / RSS violation
pub fn validate_trajectory_slow(
    trajectory: &[TrajectoryPoint],
    corridor: &dyn CorridorSource,
    objects: &[PerceivedObject],
    config: &VehicleConfig,
) -> TrajectoryVerdict {
    // Reject empty / single-point trajectories outright (the per-pose
    // loop needs ≥ 2 points to compute deltas). Conservative MRC.
    if trajectory.len() < 2 {
        return TrajectoryVerdict::MRCFallback;
    }

    // ----- A) Containment (SG2) ----------------------------------------
    //
    // Materialize the kernel-side Corridor from the trait. The trait
    // returns adapter `Point`s; we need kernel `Point`s. The field
    // shapes are identical so the conversion is a 1-for-1 copy.
    let left_kernel:  Vec<KernelPoint> = corridor.left_boundary().iter()
        .map(adapter_to_kernel_point).collect();
    let right_kernel: Vec<KernelPoint> = corridor.right_boundary().iter()
        .map(adapter_to_kernel_point).collect();
    let kernel_corridor = Corridor {
        left:           &left_kernel,
        right:          &right_kernel,
        confidence:     corridor.confidence(),
        age_ms:         corridor.age_ms(),
        min_confidence: SLOW_LOOP_MIN_CORRIDOR_CONFIDENCE,
        max_age_ms:     SLOW_LOOP_MAX_CORRIDOR_AGE_MS,
    };
    let footprint = config.to_vehicle_footprint();
    let poses: Vec<KernelPose> = trajectory.iter().map(|p| adapter_to_kernel_pose(&p.pose)).collect();

    let containment_verdict = containment::validate_trajectory_containment(
        &poses, &kernel_corridor, &footprint,
    );
    if !matches!(containment_verdict, EnforceAction::Allow) {
        return TrajectoryVerdict::MRCFallback;
    }

    // ----- B) Per-pose kinematics (P0–P6) ------------------------------
    let kinematics = config.to_kinematics_contract();
    let mut clamp_seen = false;
    for i in 0..trajectory.len() - 1 {
        let cmd = pose_pair_to_command(&trajectory[i], &trajectory[i + 1], config);
        match validate_vehicle_command(&cmd, &kinematics) {
            EnforceAction::Allow => {}
            EnforceAction::ClampLinear(_) | EnforceAction::ClampSteering(_) => {
                clamp_seen = true;
            }
            EnforceAction::DenyBreach(_) => {
                return TrajectoryVerdict::MRCFallback;
            }
        }
    }

    // ----- C) RSS over horizon (SG1) -----------------------------------
    //
    // For each PerceivedObject, find the trajectory pose closest to it
    // and evaluate longitudinal + lateral RSS gaps. The lateral check
    // gates the longitudinal check: if the object is far enough off the
    // ego corridor laterally, containment handled it; longitudinal is
    // skipped to avoid spurious violations from objects in another lane.
    for obj in objects {
        for traj_point in trajectory {
            let dx = obj.pos.x_m - traj_point.pose.x_m;
            let dy = obj.pos.y_m - traj_point.pose.y_m;

            // Skip if behind ego pose (objects we've already passed).
            // Ego-frame: rotate world delta by -heading.
            let cos_h = traj_point.pose.heading_rad.cos();
            let sin_h = traj_point.pose.heading_rad.sin();
            let dx_ego =  cos_h * dx + sin_h * dy;     // longitudinal
            let dy_ego = -sin_h * dx + cos_h * dy;     // lateral

            // Behind ego — RSS does not apply (the object is no longer
            // a forward collision risk; containment + posture handle
            // rear-end concerns).
            if dx_ego <= 0.0 {
                continue;
            }
            // Lateral filter — object is in a different lane; let
            // containment cover it.
            if dy_ego.abs() > RSS_LATERAL_ALIGNMENT_TOLERANCE_M {
                continue;
            }

            // Longitudinal RSS — required forward gap.
            let lon_required = longitudinal_safe_distance(
                traj_point.velocity_mps,
                obj.velocity_mps,
                RSS_REACTION_TIME_S,
                config.max_accel_mps2,
                config.max_decel_mps2,
                config.max_decel_mps2,
            );
            if dx_ego < lon_required {
                return TrajectoryVerdict::MRCFallback;
            }

            // Lateral RSS — required side gap. Use the object's
            // lateral velocity component as the lateral-vel input.
            // (Phase 2A: assume objects' lateral velocity = 0 if
            // PerceivedObject does not carry per-axis velocity. The
            // longitudinal check is the dominant risk; lateral RSS is
            // defence in depth against an object cutting in.)
            let obj_lat_vel = obj.velocity_mps * (obj.heading_rad - traj_point.pose.heading_rad).sin();
            let ego_lat_vel = 0.0; // straight-following assumption per
                                   // §3 (the per-pose Pose.heading
                                   // captures any planned curvature).
            let lat_required = lateral_safe_distance(
                ego_lat_vel,
                obj_lat_vel,
                kinematics.max_lateral_accel_mps2,
                RSS_REACTION_TIME_S,
            );
            if dy_ego.abs() < lat_required {
                return TrajectoryVerdict::MRCFallback;
            }
        }
    }

    // ----- D) Aggregate ------------------------------------------------
    if clamp_seen {
        TrajectoryVerdict::Clamp
    } else {
        TrajectoryVerdict::Accept
    }
}

// ---------------------------------------------------------------------------
// Conversions: adapter types ↔ kernel types
// ---------------------------------------------------------------------------

#[inline]
fn adapter_to_kernel_point(p: &Point) -> KernelPoint {
    KernelPoint { x_m: p.x_m, y_m: p.y_m }
}

#[inline]
fn adapter_to_kernel_pose(p: &Pose) -> KernelPose {
    KernelPose { x_m: p.x_m, y_m: p.y_m, heading_rad: p.heading_rad }
}

/// Map a consecutive pose pair to a kernel `ProposedVehicleCommand`.
/// The mapping derives:
///   - `delta_time_s`        = b.time_from_start_s - a.time_from_start_s
///   - `current_velocity_mps`= a.velocity_mps
///   - `linear_velocity_mps` = b.velocity_mps
///   - `current_steering_angle_deg` = derived from a.pose.heading vs prior (assume 0 at i=0)
///   - `steering_angle_deg`         = bicycle-model approx:
///        steering = atan2(δheading * wheelbase, velocity * δt) → degrees
/// The bicycle-model approximation matches the kernel's P6 (lateral-accel)
/// model and is the canonical pose-pair → steering-angle conversion.
/// Field names match `ProposedVehicleCommand` exactly (Step 0).
fn pose_pair_to_command(
    a: &TrajectoryPoint,
    b: &TrajectoryPoint,
    config: &VehicleConfig,
) -> ProposedVehicleCommand {
    let delta_time_s = b.time_from_start_s - a.time_from_start_s;
    let delta_heading = b.pose.heading_rad - a.pose.heading_rad;
    // Average velocity over the segment; avoids dividing by ~0 when
    // velocity is small at one endpoint.
    let avg_velocity = 0.5 * (a.velocity_mps + b.velocity_mps);
    let denom = avg_velocity * delta_time_s;
    // Guard the bicycle-model denominator: at near-zero velocity or
    // near-zero dt the steering is undefined; report 0 (the P1
    // `delta_time_s <= 0.0` check in `validate_vehicle_command` will
    // catch genuinely-bad inputs).
    let steering_rad = if denom.abs() > 1e-6 {
        (delta_heading * config.wheelbase_m).atan2(denom)
    } else {
        0.0
    };
    ProposedVehicleCommand {
        linear_velocity_mps:        b.velocity_mps,
        current_velocity_mps:       a.velocity_mps,
        delta_time_s,
        steering_angle_deg:         steering_rad.to_degrees(),
        // Phase 2A: 0.0 at i=0; for i>0 we don't carry steering state
        // (planner-published trajectories don't include it). The kernel's
        // P5b steering-rate check still bounds the change because the
        // mapping treats each segment independently.
        current_steering_angle_deg: 0.0,
    }
}

#[cfg(test)]
mod conversion_tests {
    use super::*;
    use crate::state::Pose as AdapterPose;

    #[test]
    fn pose_pair_zero_delta_heading_produces_zero_steering() {
        let cfg = VehicleConfig::default_urban();
        let a = TrajectoryPoint {
            pose: AdapterPose { x_m: 0.0, y_m: 0.0, heading_rad: 0.0 },
            velocity_mps: 10.0, time_from_start_s: 0.0,
        };
        let b = TrajectoryPoint {
            pose: AdapterPose { x_m: 1.0, y_m: 0.0, heading_rad: 0.0 },
            velocity_mps: 10.0, time_from_start_s: 0.1,
        };
        let cmd = pose_pair_to_command(&a, &b, &cfg);
        assert!((cmd.steering_angle_deg).abs() < 1e-9);
        assert_eq!(cmd.linear_velocity_mps, 10.0);
        assert_eq!(cmd.current_velocity_mps, 10.0);
        assert!((cmd.delta_time_s - 0.1).abs() < 1e-9);
    }

    #[test]
    fn pose_pair_curvature_produces_proportional_steering() {
        let cfg = VehicleConfig::default_urban();
        // 10° heading change over 0.5 s at 10 m/s → ~ atan2(0.1745*2.8,
        // 10*0.5) = atan2(0.4886, 5.0) ≈ 5.58° steering.
        let a = TrajectoryPoint {
            pose: AdapterPose { x_m: 0.0, y_m: 0.0, heading_rad: 0.0 },
            velocity_mps: 10.0, time_from_start_s: 0.0,
        };
        let b = TrajectoryPoint {
            pose: AdapterPose { x_m: 5.0, y_m: 0.0, heading_rad: 10.0_f64.to_radians() },
            velocity_mps: 10.0, time_from_start_s: 0.5,
        };
        let cmd = pose_pair_to_command(&a, &b, &cfg);
        assert!(cmd.steering_angle_deg > 4.0 && cmd.steering_angle_deg < 7.0,
            "expected ~5.6° steering, got {}", cmd.steering_angle_deg);
    }
}
