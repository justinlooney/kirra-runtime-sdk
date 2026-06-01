# Parko ↔ Occy Topology — Two Parallel Paths, Shared Safety Primitives

**Doc ID:** KIRRA-OCCY-TOPOLOGY-001
**Status:** Draft (L1 decision; supersedes any implicit "one pipeline" assumption)
**Related:** M2 — Parko ROS 2 node (this branch); KIRRA-OCCY-OPTIONB-001 (Occy adapter)

---

## 1. The decision

Parko and Occy are **two parallel safety paths**, selected per deployment
class. They are **not** chained — a deployment runs exactly **one** of
them (or both, for distinct subsystems running independently), never
piping one's output into the other.

Both consume the **same safety primitives**:

| Primitive | Crate | Used by Parko | Used by Occy |
|---|---|---|---|
| `parko_core::rss::{longitudinal_safe_distance, lateral_safe_distance}` | `parko-core` | indirectly via `KirraGovernor`'s RSS gate | directly in `validate_trajectory_slow` |
| `VehicleKinematicsContract` + `validate_vehicle_command` | `kirra-runtime-sdk` | via `KirraGovernor` per-tick | per-pose in `validate_trajectory_slow` |
| `URBAN_ODD_SPEED_CAP_MPS` (H2) | `kirra-runtime-sdk` | via `VehicleConfig`/`KirraGovernor` contract | via `VehicleConfig` (default_urban) |
| `FleetPosture` + posture-gated MRC | `kirra-runtime-sdk` | `KirraGovernor::apply_mrc_profile` | `validate_trajectory_slow(.., posture)` (M1) |

This shared substrate is what makes the two paths comparable from a
safety-case standpoint — they enforce the same envelopes, just at
different positions in the autonomy stack.

## 2. Parko — end-to-end ML control

```
                                              ┌─────────────────┐
sensor topic ──▶ SensorFrame ──▶ InferenceLoop│ KirraGovernor    │── ControlCommand ──▶ /cmd_vel
                                  ├──▶ backend│  (primary)        │       (gated)
                                  ├──▶ parse  │ GovernorComparator│
                                  ├──▶ govern │  (shadow ↔ primary)│
                                  └──▶ tx     │  divergence audit │
                                              └─────────────────┘
```

- **Input:** sensor observation (camera tensor, lidar batch, etc.) on a
  ROS 2 topic; mapped to a `SensorFrame` with a payload tensor batch.
- **Compute:** the ONNX (or other) backend runs the policy model and
  emits per-axis velocity commands. Non-finite outputs are caught by
  `parse_inference_to_command` and translated to a stopped
  `ControlCommand` — the loop never propagates a `NaN` to the actuator.
- **Govern:** `parko-kirra::GovernorComparator` runs **two** independent
  `KirraGovernor` instances in lockstep on every tick. Disagreement on
  either the linear or angular axis (CERT-006 v3) emits a
  `ComparatorDivergence` event and reconciles to a most-restrictive
  command; persistent divergence escalates to `LockedOut`.
- **Output:** the post-governor `ControlCommand` is published as a
  ROS 2 `geometry_msgs/Twist` (or equivalent) on the actuator topic.
- **Target deployments:** edge robotics, differential-drive platforms,
  small mobile bases, manipulator-mounted vehicles. Anywhere the
  control policy is a model rather than a planner+follower.

## 3. Occy — trajectory-based AV

```
Autoware planning ──▶ Trajectory ──▶ validate_trajectory_slow ──▶ TrajectoryVerdict
                                       ├── containment (SG2)         │
                                       ├── per-pose kinematics (SG3) │  Accept / Clamp / MRCFallback
                                       ├── RSS over horizon (SG1)    │
                                       └── posture profile (M1)      │
                                                                     ▼
                                                            fast loop ─▶ /control_cmd
                                                                            (gated, MRC on staleness)
```

- **Input:** an `autoware_planning_msgs::Trajectory` (planned poses + velocities + time-from-start).
- **Validate:** SG1 (RSS), SG2 (containment vs HD-map corridor), SG3
  (per-pose kinematics), SG8 (posture-driven contract selection).
- **Output:** fast-loop control commands that conform to the most
  recently accepted trajectory; MRC on any staleness fault.
- **Target deployments:** mapped AV stacks running Autoware-class
  planners.

## 4. Why they don't chain

The two paths produce **incompatible artifacts**:

| Parko output | Occy input |
|---|---|
| `ControlCommand { linear_velocity, angular_velocity, timestamp_ms }` — a single instantaneous Twist | an entire `Trajectory` — N poses with `time_from_start_s`, velocities, geometric continuity |

Parko produces **commands**, not trajectories. Its model's output is a
single tick's intended velocity, not a future-time plan. Synthesising a
`Trajectory` from a stream of `ControlCommand`s is not a meaningful
operation — the resulting "trajectory" would be a degenerate
extrapolation that contains zero of the planner-side information SG2
containment needs to validate (corridor membership over an N-pose
horizon vs. an instantaneous Twist with no horizon).

The reverse is equally unsound. Occy's accepted `Trajectory` is the
plan the vehicle is *committed* to follow; piping the trajectory's
current-pose target velocity through Parko's `KirraGovernor` would
double-gate (Occy already gated it) and risk divergence between the
trajectory verdict and the per-tick command.

**The right composition is at the deployment level**, not the pipeline
level: one or the other, never both in series on the same actuator.

## 5. What this means for a CARLA demo

Each path gets its own demo path:

- **Parko in CARLA:** the Parko ROS 2 node subscribes to CARLA sensor
  topics, runs the ML policy + governor, publishes gated commands to
  CARLA's vehicle interface. The parallel-path analog of the Occy
  adapter's CARLA scenarios.
- **Occy in CARLA:** the Autoware-in-CARLA stack produces trajectories;
  the Occy adapter validates them and publishes gated control commands.

Both demos exercise the **same** safety primitives (RSS, kinematics,
posture-driven MRC, ODD cap) but on different artifacts. A green run
on one is not evidence for the other — both must be exercised
independently.

## 6. What's in scope for M2

M2 builds the **Parko ROS 2 node** (a new crate
`parko/crates/parko-ros2`) so Parko's path runs live in ROS, the way
the Occy adapter runs live in ROS today. The node:

- Reuses the adapter's r2r patterns (node init, subscription drain
  tasks, mpsc channels, feature gate).
- Drives `InferenceLoop::tick(SensorFrame, SafetyPosture)` with the
  `GovernorComparator` attached as the governor.
- Publishes the post-governor `ControlCommand` as ROS 2 Twist on the
  configured actuator topic.
- Fails closed on: backend errors / non-finite outputs → stopped
  command; comparator escalation → zero command; sensor-input
  staleness → MRC; backend-not-installed → process exit at startup.

Live posture sourcing reuses the **same M1b mechanism** as the Occy
adapter (`crate::posture_tracker::PostureTracker` + SSE subscriber).
M2 declares the integration point and accepts posture as a node
parameter for now; the live wiring is the next milestone (one
PostureTracker instance per node, identical state machine).

---

**Decision recorded by:** M2 (parko-ros2 crate creation).
**Next review:** when a deployment proposes piping one path into the
other (don't — see §4) or proposes adding a third (lateral path beside
both).
