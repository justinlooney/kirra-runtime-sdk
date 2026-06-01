// parko/crates/parko-ros2/src/lib.rs
//
// M2 — Parko ROS 2 node crate root. Two-lane layout, mirroring the
// kirra-ros2-adapter convention:
//
//   - `config`, `command_mapping`, `sensor_mapping` — pure (no ROS, no
//     async, no I/O). Unit-testable on stable. These are the seams
//     the integrator overrides per platform.
//   - `tick_pipeline`   — the heart of the loop: drive a configured
//     `InferenceLoop` one step with a given `SensorFrame` + posture,
//     receive the post-governor `ControlCommand`, map to an
//     `OutgoingTwist` via `command_mapping`. Async but
//     transport-independent: tests exercise this via parko-core's
//     `MockBackend` without touching r2r.
//   - `node`           — r2r-backed adapter task: subscribes to the
//     configured sensor topic, drives the tick pipeline, publishes
//     `OutgoingTwist` to the actuator topic. Feature-gated on `ros2`.
//
// Design tie-in: `docs/safety/PARKO_OCCY_TOPOLOGY.md`
// (KIRRA-OCCY-TOPOLOGY-001) — the parallel-paths L1 decision Parko +
// Occy run side by side, sharing safety primitives, never chained.

pub mod command_mapping;
pub mod comparator_adapter;
pub mod config;
pub mod sensor_mapping;
pub mod tick_pipeline;

#[cfg(feature = "ros2")]
pub mod node;

pub use crate::command_mapping::{enforce_outgoing_twist, OutgoingTwist};
pub use crate::comparator_adapter::ComparatorAsGovernor;
pub use crate::config::ParkoNodeConfig;
pub use crate::sensor_mapping::SensorInputMapping;
pub use crate::tick_pipeline::{run_pipeline_tick, TickError, TickOutcome};
