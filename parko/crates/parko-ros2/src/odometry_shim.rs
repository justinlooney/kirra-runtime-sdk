// parko/crates/parko-ros2/src/odometry_shim.rs
//
// nav_msgs/Odometry → OdomSample extraction shim — the deferred ROS half of the
// odometry mapping. PURE field-copy CORE (no r2r, always compiled, unit-tested)
// + a THIN r2r ADAPTER (`#[cfg(feature = "ros2")]`, extraction only).
//
// SAFETY FRAMING. The shim is upstream of the odom transform → model → governor.
// The transform already fail-closes on non-finite values, so this shim stays
// thin (no value validation here). What it must get right is MEANING: the
// quaternion component order. The r2r `geometry_msgs/Quaternion` is `{x,y,z,w}`
// and `OdomSample.orientation_xyzw` is `[x, y, z, w]` — a direct order copy, but
// a silent reorder would scramble every orientation, so the order is pinned by
// test.
//
// FRAME NOTE (FLAGGED): a `nav_msgs/Odometry` carries pose in the odom/world
// frame and twist in the child (body) frame. This shim copies BOTH VERBATIM —
// it does NOT transform frames. The transform / model owns frame expectations;
// the shim only moves fields.
//
// OdomSample is IN MAIN, so this references `crate::sensor_mapping::OdomSample`
// directly — no mirror, no collapse-at-merge.

use crate::sensor_mapping::OdomSample;

/// Plain mirror of the `nav_msgs/Odometry` fields `OdomSample` needs — r2r-free,
/// so the field mapping is unit-testable without a ROS environment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OdometryRawFields {
    /// `pose.pose.position` {x, y, z}.
    pub position: [f64; 3],
    /// `pose.pose.orientation` in `[x, y, z, w]` order.
    pub orientation_xyzw: [f64; 4],
    /// `twist.twist.linear` {x, y, z}.
    pub linear_velocity: [f64; 3],
    /// `twist.twist.angular` {x, y, z}.
    pub angular_velocity: [f64; 3],
}

/// Pure field copy into the transform's `OdomSample`. No narrowing (both f64),
/// no frame transform, no value validation (the transform owns that).
#[must_use]
pub fn odometry_to_sample(raw: &OdometryRawFields) -> OdomSample {
    OdomSample {
        position: raw.position,
        // [x, y, z, w] verbatim — DO NOT reorder (pinned by test).
        orientation_xyzw: raw.orientation_xyzw,
        linear_velocity: raw.linear_velocity,
        angular_velocity: raw.angular_velocity,
    }
}

/// THIN r2r ADAPTER — extraction only. Pulls the raw fields off the r2r message
/// and calls the pure fn. Compiles only under `--features ros2` in a ROS env.
#[cfg(feature = "ros2")]
pub fn odometry_msg_to_sample(msg: &r2r::nav_msgs::msg::Odometry) -> OdomSample {
    let p = &msg.pose.pose.position;
    let o = &msg.pose.pose.orientation;
    let l = &msg.twist.twist.linear;
    let a = &msg.twist.twist.angular;
    odometry_to_sample(&OdometryRawFields {
        position: [p.x, p.y, p.z],
        orientation_xyzw: [o.x, o.y, o.z, o.w],
        linear_velocity: [l.x, l.y, l.z],
        angular_velocity: [a.x, a.y, a.z],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asymmetric input so any field swap or quaternion reorder is visible.
    fn raw() -> OdometryRawFields {
        OdometryRawFields {
            position: [1.0, 2.0, 3.0],
            orientation_xyzw: [0.1, 0.2, 0.3, 0.4], // x,y,z,w — all distinct
            linear_velocity: [4.0, 5.0, 6.0],
            angular_velocity: [7.0, 8.0, 9.0],
        }
    }

    #[test]
    fn quaternion_lands_in_xyzw_order() {
        let s = odometry_to_sample(&raw());
        // A reorder (e.g. w,x,y,z) would make this fail.
        assert_eq!(s.orientation_xyzw, [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn fields_copied_to_correct_slots() {
        let s = odometry_to_sample(&raw());
        assert_eq!(s.position, [1.0, 2.0, 3.0]);
        assert_eq!(s.linear_velocity, [4.0, 5.0, 6.0]);
        assert_eq!(s.angular_velocity, [7.0, 8.0, 9.0]);
        // Distinct blocks → a position/linear/angular swap would be caught above.
    }
}
