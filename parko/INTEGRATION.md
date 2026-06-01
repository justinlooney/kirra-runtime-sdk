# Parko Integration Guide — Running a Learned Controller Under Safety Governance

**Audience:** external integrators (robotics startups, university labs, pilot
partners) who have a *trained model* and want to deploy it under Parko's
runtime safety governor.

**Status:** developer guide for the current workspace state. This document is
the *how-to-integrate* companion to the safety case in `docs/safety/`
(notably `docs/safety/ANGULAR_VELOCITY_SOTIF.md` and
`docs/safety/PARKO_OCCY_TOPOLOGY.md`); it is **not** itself a safety claim.
Several pieces it references are marked DRAFT or PLANNED — see
§7 *What's built vs. planned* and the inline ⚠️ markers, and do not treat a
PLANNED item as working.

> **Accuracy note.** Every type, field, method, constant, env var, and topic
> named below was cross-checked against the code on this branch. Where the
> spec would otherwise imply something works that does not yet, it is flagged
> PLANNED with the real gap described. See §8 for the full cross-check.

---

## 1. Overview — what Parko is (and is not)

Parko is a **safety-governed runtime for learned controllers**. You bring a
trained model; Parko runs it inside a fixed-rate control loop and gates every
command the model produces through a safety governor before it reaches an
actuator.

The three layers, all of which exist as Rust crates in this workspace:

| Layer | Crate | Responsibility |
|---|---|---|
| Inference | `parko-core` (trait) + `parko-onnx` / `parko-openvino` (impls) | Run the model on a tensor input |
| Control loop | `parko-core::scheduler::InferenceLoop` | Fixed-tick drive, one-tick-delayed publication, degraded-mode + NaN scrubbing |
| Governance | `parko-kirra::KirraGovernor` / `GovernorComparator` | Bound the model's output to a physical safety envelope |
| Transport | `parko-ros2` | ROS 2 sensor-in / twist-out node wiring it all together |

### Scope boundary (read this first)

**Parko bounds the model's OUTPUT to safety limits. It does not plan, replace,
or guarantee the correctness of the model.**

- **Task performance is the integrator's responsibility.** Whether the robot
  reaches its goal, follows a lane, or avoids getting stuck is a property of
  *your* model. Parko never improves task performance.
- **Safety bounds are Parko's responsibility.** Parko guarantees the command
  that reaches the actuator stays inside a configured kinematic / SOTIF /
  RSS / posture envelope, regardless of what the model asked for.
- A **clamped or denied** command is a signal that **the model asked for
  something unsafe**. The governor is the *safety net*, not the primary
  controller. A model that is constantly being clamped is a model that needs
  more training — not a governor that needs loosening.

---

## 2. The model contract

### 2.1 Input — sensor data → tensor

Parko's control loop hands the model a `parko_core::backend::TensorBatch`: a
`HashMap<String, TensorStorage>` of named input tensors plus a metadata map
(`parko-core/src/backend.rs`). The **tensor name must match the model's input
node name** — the backends look inputs up by name (see §2.3).

You produce that `TensorBatch` from your raw sensor data by implementing — or
reusing — a `SensorInputMapping` (`parko-ros2/src/sensor_mapping.rs`):

```rust
pub trait SensorInputMapping: Send + Sync {
    type Sample;                         // your concrete sensor message type
    fn to_frame(&self, frame_id: u64, timestamp_ms: u64, sample: &Self::Sample)
        -> SensorFrame;                  // parko_core::sensor::SensorFrame
}
```

Two **pre-tested mappings** ship today:

#### Camera (`CameraMapping` + `CameraConfig`)

`CameraConfig` fields (all explicit — no silent defaults):

| Field | Type / choices | Meaning |
|---|---|---|
| `encoding` | `CameraEncoding::{Rgb8, Bgr8, Mono8}` | Source channel order; output is **always RGB-ordered** for 3-channel input (a `Bgr8` source is channel-swapped on the way out) |
| `target_height`, `target_width` | `u32` | Output H/W; the frame is resized to these |
| `resize` | `CameraResize::Nearest` | Nearest-neighbour only today (bilinear PLANNED) |
| `normalization` | `CameraNormalization::{Unit01, SignedUnit, MeanStd{mean,std}}` | `[0,1]`, `[-1,1]`, or per-channel `(x/255 - mean)/std`; `mean`/`std` length must equal channel count |
| `layout` | `CameraLayout::{Nchw, Nhwc}` | `[1,C,H,W]` (PyTorch/ONNX) or `[1,H,W,C]` (TF/TFLite) |
| `tensor_name` | `String` | **Must equal the model's input-node name** |

The pure transform is `CameraMapping::to_tensor(&CameraSample)` →
`Result<TensorBatch, CameraMappingError>`; the `Sample` for the trait is
`OwnedCameraSample { bytes, src_width, src_height }`. On malformed input the
trait-level `to_frame` logs and emits a **zero tensor** so the downstream MRC
path fires — fail-closed by construction.

#### Odometry / state vector (`OdomMapping` + `OdomConfig`)

| Field | Type | Meaning |
|---|---|---|
| `include_position` | `bool` | `(x,y,z)` — 3 floats |
| `include_orientation` | `Option<OdomOrientation::{Yaw, FullEuler, Quaternion}>` | 1 / 3 / 4 floats; `Yaw` is the planar-control default |
| `include_linear_velocity` | `bool` | `(vx,vy,vz)` — 3 floats |
| `include_angular_velocity` | `bool` | `(wx,wy,wz)` — 3 floats |
| `tensor_name` | `String` | model input-node name |

Output vector order (each block present only if its toggle is on):
`[pos.x, pos.y, pos.z, {orientation}, vlin.x..z, vang.x..z]`. `OdomSample`
carries `orientation_xyzw` in ROS `(x,y,z,w)` convention; yaw uses Tait–Bryan
ZYX, matching `kirra-ros2-adapter`'s `quat_to_yaw`. `OdomConfig::vector_len()`
returns the total length.

A trivial `VectorMapping::new(tensor_name)` (`Sample = Vec<f32>`) is also
provided — it wraps a flat float vector under one tensor name and is what the
shipped node binary uses (§5).

### 2.2 Output — model output → `ControlCommand`

The scheduler reads two **named output tensors** from the model
(`InferenceLoop::parse_inference_to_command`, `parko-core/src/scheduler.rs`):

| Output tensor name | Maps to | Units |
|---|---|---|
| `cmd_vel_linear` | `ControlCommand::linear_velocity` | m/s, forward axis |
| `cmd_vel_angular` | `ControlCommand::angular_velocity` | rad/s, yaw (positive = CCW) |

Rules, verbatim from the code:

- Only the **first element** of each tensor is read (`.as_slice().first()`).
- A **missing** tensor defaults that axis to `0.0` (not an error).
- A **non-finite** value (NaN / ±Inf) on either axis makes the loop discard
  the command and emit `ControlCommand::stopped()` with
  `active_state_degraded = true` (fail-closed; see §6).

`ControlCommand { linear_velocity: f64, angular_velocity: f64, timestamp_ms:
u64 }` models a planar differential-drive / `geometry_msgs/Twist` 2D subset
(`linear.x`, `angular.z`). 6-DOF, manipulator joints, and steering-angle
(Ackermann) commands are **not** representable by this type.

### 2.3 Model format per backend

| Backend | Type | Constructor | Format | Status |
|---|---|---|---|---|
| ONNX Runtime (CPU) | `parko_onnx::OrtBackend` | `OrtBackend::new(model_path)` | `.onnx`; needs `libonnxruntime.so` v1.24.x via `ORT_DYLIB_PATH` | ✅ built |
| Intel OpenVINO (CPU) | `parko_openvino::OvBackend` | `OvBackend::new(model_path)` | `.onnx` ingested directly (or `.xml` IR); needs `libopenvino_c.so` (`OPENVINO_LIB_PATH`) | ✅ built |
| TensorRT / QNN / TIDL / AMD | — | — | — | ⚠️ PLANNED (zero-output stubs only, behind `backend-*` cargo features in `parko-core`; no hardware inference) |

Both shipped backends implement `parko_core::backend::InferenceBackend`
(`load_model` + `run`), wrap their non-`Sync` session in a `Mutex`, and return
owned `TensorBatch<'static>` outputs. Backend selection is by *construction* —
you build the backend you want and hand it to `InferenceLoop::new`. (Note the
shipped *node binary* does not yet do this selection — §5, §7.)

---

## 3. The governance contract

Every command the model produces is evaluated by a `SafetyGovernor`
(`parko-core/src/safety.rs`) before publication. The production governor is
`parko_kirra::KirraGovernor`, normally wrapped in a `GovernorComparator`
(dual-instance lockstep). `evaluate` takes the proposed command, the previous
command, `delta_time_s`, and a `SafetyPosture`, and returns an
`EnforcementAction`:

| Verdict | Meaning at the actuator |
|---|---|
| `Allow` | forward the command unmodified |
| `ClampLinearVelocity(v)` | replace linear axis with `v` |
| `ClampAngularVelocity(w)` | replace angular axis with `w` |
| `ClampMotion { linear, angular }` | replace each `Some` axis; leave each `None` axis as proposed |
| `Deny { reason }` | hard stop — scheduler stamps `ControlCommand::stopped()` |

What `KirraGovernor` enforces, by posture (`parko-kirra/src/lib.rs`):

- **`LockedOut`** → `Deny` (hard stop), unconditionally, checked first.
- **`Degraded`** (or RSS unsafe — see below) → `apply_mrc_profile`: linear
  clamped to `MRC_VELOCITY_CEILING_MPS = 5.0` m/s; angular clamped to the
  **MRC SOTIF bound** (derated). Most-restrictive-wins across axes.
- **`Nominal`** →
  - **Linear axis**: the Kirra **vehicle kinematics contract**
    (`validate_vehicle_command`, `src/gateway/kinematics_contract.rs`) — the
    velocity hard ceiling + implied accel/brake limits + (Ackermann steering
    checks, which the bridge leaves inert; see §3 note). `KirraGovernor::new()`
    uses `VehicleKinematicsContract::nominal_reference_profile()`
    (`max_speed_mps = 35.0`).
  - **Angular axis**: the **SOTIF-derived bound**
    `ω_max(v) = min(rollover(v), sweep, ftti)`
    (`AngularVelocityBound::omega_max`, `parko-kirra/src/angular_bound.rs`),
    evaluated at the command's linear velocity.

- **RSS safe-distance gate**: if `update_rss_state` has set an unsafe
  `RssState`, the Nominal path is short-circuited to the MRC profile (a sensor
  gap is recoverable, so it derates rather than hard-stops). The RSS math lives
  in `parko_core::rss::{longitudinal_safe_distance, lateral_safe_distance}`
  (IEEE 2846-2022), which fail safe to `RSS_FAILSAFE_DISTANCE_M = 1e6` on
  invalid input.

> ⚠️ **DRAFT — angular bound.** The SOTIF angular-velocity derivation
> (issue #136, `docs/safety/ANGULAR_VELOCITY_SOTIF.md`) is engineering
> analysis pending formal safety-engineer sign-off. Don't cite it as a
> validated safety claim.

> **Linear-ODD-cap caveat (accuracy).** The kinematics contract *supports* an
> ODD speed cap (`odd_speed_cap_mps`, `URBAN_ODD_SPEED_CAP_MPS = 22.35`), and
> `effective_max_speed_mps()` enforces `min(max_speed, odd_cap)`. **But
> `KirraGovernor` is hard-wired to `nominal_reference_profile()` /
> `mrc_fallback_profile()` and exposes no setter for a custom contract or ODD
> cap.** So the linear ceiling actually enforced through parko-kirra today is
> 35 m/s (Nominal) / 5 m/s (MRC); the 22.35 m/s ODD cap is **not reachable**
> through the parko path yet. Per-platform contract / ODD-cap injection into
> `KirraGovernor` is PLANNED (§7).

The key point restated: **the model should produce in-bounds commands.** The
governor is the safety net. A clamp/deny means the model misbehaved.

---

## 4. Platform configuration

Three groups of knobs. Conservative defaults mean an *uncharacterized*
platform fails toward safe.

### 4.1 Linear kinematics — `VehicleKinematicsContract`

(`src/gateway/kinematics_contract.rs`) Fields: `max_speed_mps`,
`max_accel_mps2`, `max_brake_mps2`, `max_steering_deg`,
`max_steering_rate_deg_s`, `min_follow_distance_m`, `max_lateral_accel_mps2`,
`wheelbase_m`, `width_m`, `length_m`, `overhang_front_m`, `overhang_rear_m`,
`odd_speed_cap_mps: Option<f64>`.

Canonical profiles:

| Profile | `max_speed_mps` | `max_accel` | `max_brake` | `max_lateral_accel` |
|---|---|---|---|---|
| `nominal_reference_profile()` | 35.0 | 2.5 | 4.5 | 3.5 |
| `mrc_fallback_profile()` | 5.0 | 1.0 | 3.0 | 1.5 |

> The parko-kirra governor selects these two profiles internally by posture;
> see the §3 caveat — there is no public path to override them per platform
> yet.

### 4.2 Angular geometry — `PlatformParams`

(`parko-kirra/src/angular_bound.rs`) Drives `ω_max(v)`:

| Field | Meaning |
|---|---|
| `track_width_m` | wheel-to-wheel track (rollover `t/2h`) |
| `cog_height_m` | centre-of-gravity height (rollover) |
| `robot_extent_m` | bounding-circle radius (sweep `r·ω ≤ v_edge_safe`) |
| `v_edge_safe_mps` | safe contact velocity (ISO/TS 15066 basis) |
| `theta_max_rad` | max heading change per FTTI |
| `ftti_s` | fault-tolerant time interval |
| `mrc_posture_factor` | (0,1] derate for Degraded (default 0.5) |

Constructors: `PlatformParams::conservative_default()` (tight; for an
uncharacterized platform — ω_max(0) ≈ 0.20 rad/s) and
`PlatformParams::urban_service_robot_reference()` (≈ TurtleBot-4 scale —
ω_max(0) ≈ 0.833 rad/s). `validate()` rejects non-positive geometry / out-of-
range factor. Below `ROLLOVER_MIN_LINEAR_VELOCITY_MPS = 0.05` the rollover term
is masked (v→0 singularity). Apply via
`KirraGovernor::with_platform_params(params)`, or override with a flat scalar
via `with_angular_bounds(nominal_rad_s, mrc_rad_s)`.

### 4.3 Posture source — fail-closed

The posture feeding the governor comes from the shared kernel
`PostureTracker` (`src/posture_tracker.rs`), wrapped for parko by
`ParkoPostureState` (`parko-ros2/src/posture_state.rs`):

- `ParkoPostureState::no_source()` → always `Nominal` (`observe` is a no-op).
  This is the M2 default and the *full-envelope* mode.
- `ParkoPostureState::with_source()` → fail-closed:
  - pre-first-event seed = **Degraded** (not Nominal);
  - staleness derate to **Degraded** after
    `POSTURE_STALENESS_TIMEOUT_MS = 6_000` ms of silence;
  - `LockedOut` is **sticky-toward-safe** (only an explicit non-LockedOut
    observation releases it);
  - a poisoned lock reads as **Degraded**, never Nominal.

`current_safety_posture()` bridges the kernel `FleetPosture` to the parko-core
`SafetyPosture` (a pure 3-arm match via `fleet_to_safety`).

> ⚠️ **PLANNED — live posture transport (M2c).** `with_source()` and the
> posture-aware tick (`run_pipeline_tick_with_posture_state`) exist and are
> unit-tested, but **nothing in the shipped binary subscribes to a verifier
> posture stream**. The env var `KIRRA_POSTURE_STREAM_URL` is referenced only
> in *comments* describing the intended wiring — it is **not read** by any
> code, and there is no `KIRRA_POSTURE_*` family of env vars implemented. The
> only env var the binary reads is `PARKO_MODEL_PATH`. Until M2c lands, the
> node runs the static `no_source()` → `Nominal` path (§5).

---

## 5. Deployment steps

1. **Pick a backend.** Construct `OrtBackend::new(path)` (set `ORT_DYLIB_PATH`)
   or `OvBackend::new(path)` (set `OPENVINO_LIB_PATH`). For a dry run,
   `parko_core::backends::mock::MockBackend::new(outputs, descriptor)`.
2. **Choose / implement a `SensorInputMapping`.** Use `CameraMapping` /
   `OdomMapping` / `VectorMapping`, or implement the trait for your message
   type. Ensure `tensor_name` matches the model's input node.
3. **Configure the platform.** Set up the angular geometry via
   `KirraGovernor::new().with_platform_params(PlatformParams { … })`. (Linear
   contract / ODD cap injection is PLANNED — today you get the reference/MRC
   profiles.)
4. **Configure the posture source** — or run without one. Today: `no_source()`
   (Nominal). The `with_source()` fail-closed path is wired in the tick
   pipeline but not in the binary (M2c).
5. **Wire the ROS 2 topics.** `ParkoNodeConfig` (`parko-ros2/src/config.rs`):
   - `sensor_topic` (default `~/input/observation`)
   - `command_topic` (default `~/output/cmd_vel`)
   - `tick_period_s` (default `0.05` = 20 Hz; becomes the governor's
     `delta_time_s`)
   - `sensor_staleness_budget_ms` (default `200`)
6. **Run.** Build the inference loop and start the node:

   ```rust
   let infer = InferenceLoop::new(backend, model, actuator_tx)
       .with_governor(ComparatorAsGovernor(
           GovernorComparator::new(KirraGovernor::new(), KirraGovernor::new())))
       .with_tick_period(0.05);
   parko_ros2::node::run_node(config, Arc::new(Mutex::new(infer)),
                              mapping, SafetyPosture::Nominal, "parko_governor").await?;
   ```

   The shipped binary `parko_ros2_node` (`required-features = ["ros2"]`,
   `parko-ros2/src/bin/parko_ros2_node.rs`) does this end-to-end:
   `init_tracing()` → `ParkoNodeConfig::default()` → build loop with a
   `GovernorComparator` → `run_node(..)` → SIGINT/SIGTERM shutdown.

   **Node I/O (current transport):** subscribes to `sensor_topic` as
   `std_msgs/msg/Float32MultiArray` decoded from a project-local JSON shape
   `{ "data": [f32…], "stamp_ms": u64 }`, and publishes `command_topic` as
   `geometry_msgs/msg/Twist` (`linear.x` ← `OutgoingTwist::linear_x_mps`,
   `angular.z` ← `OutgoingTwist::angular_z_rads`, other axes zeroed). The tick
   flow per message is: decode → `mapping.to_frame` → `run_pipeline_tick`
   (staleness check → `InferenceLoop::tick` → governor → `enforce_outgoing_twist`)
   → publish.

> ⚠️ **Two honest gaps in the shipped binary.**
> 1. **Backend.** Despite a feature-gated comment, `main()` always calls
>    `build_dev_backend()` → `MockBackend` (fixed zero output, with a loud
>    WARN). The real `OrtBackend`/`OvBackend` are *not yet selected* by the
>    binary — wire them yourself via `InferenceLoop::new` or wait for the
>    `onnx-backend` dispatch to be finished (PLANNED).
> 2. **Sample type.** `run_node` is generic but constrained to
>    `SensorInputMapping<Sample = Vec<f32>>`, and the transport decodes a JSON
>    float array. `CameraMapping` (`Sample = OwnedCameraSample`) and
>    `OdomMapping` (`Sample = OdomSample`) are pure-tested but their ROS
>    message shims (`sensor_msgs/Image`, `nav_msgs/Odometry`) are PLANNED —
>    use them today via direct `to_tensor`/`to_frame` calls (tests, CARLA,
>    bag replay), not yet through the live node.

---

## 6. Fail-closed behavior

Every failure resolves to a safe command (a stopped twist, a clamp, or a
hard stop). Sources cited.

| Failure | Detector | Response |
|---|---|---|
| **Sensor staleness** (`frame_age > sensor_staleness_budget_ms`, default 200 ms) | `run_pipeline_tick` (`tick_pipeline.rs`) | `OutgoingTwist::stopped` + `TickError::StaleSensorInput`; inference skipped |
| **Inference error** (`InferenceLoop::tick` returns `Err`, e.g. bad model handle) | `run_pipeline_tick` | `OutgoingTwist::stopped` + `TickError::InferenceError` |
| **NaN / Inf model output** | `parse_inference_to_command` (`scheduler.rs`) | discard → `ControlCommand::stopped`, `active_state_degraded = true` |
| **NaN leaking past the governor** | `enforce_outgoing_twist` (`command_mapping.rs`) | defence-in-depth re-check → `OutgoingTwist::stopped` |
| **Malformed camera frame** | `CameraMapping::to_frame` | zero tensor + `tracing::error!` → downstream MRC |
| **Governor escalation** | `KirraGovernor` (`LockedOut`→`Deny`) / `GovernorComparator` (persistent divergence at safe speed → `Deny`) | scheduler stamps `ControlCommand::stopped` |
| **Posture seed / staleness / lock** | `PostureTracker` via `ParkoPostureState::with_source` | pre-first-event → Degraded; >6 s silence → Degraded; `LockedOut` sticky |
| **RSS invalid / non-finite input** | `parko_core::rss` | returns `RSS_FAILSAFE_DISTANCE_M` (1e6) → governor clamps/stops |
| **Degraded/no-governor builtin** | `InferenceLoop` degraded mode (only when *no* governor attached) | linear clamped to `DegradationThresholds::max_linear_velocity_mps = 1.5` |

---

## 7. What's built vs. planned

**Built (✅):**
- `InferenceBackend` trait + CPU backends: `parko-onnx::OrtBackend`,
  `parko-openvino::OvBackend` (ONNX; cross-backend equivalence test on
  MNIST-12).
- `SensorInputMapping` + `CameraMapping` / `OdomMapping` / `VectorMapping`
  (pure transforms, fully unit-tested).
- Command mapping `OutgoingTwist` / `enforce_outgoing_twist`, and the
  `EnforcementAction` → command application in the scheduler.
- `KirraGovernor` (linear kinematics contract + SOTIF angular bound + RSS gate
  + posture profiles) and `GovernorComparator` (lockstep + audit event).
- `PostureTracker` / `ParkoPostureState` fail-closed state machine.
- `parko-ros2` node + binary (`ros2` feature): Float32MultiArray in,
  geometry_msgs/Twist out, with the fail-closed tick pipeline.

**Planned / DRAFT (⚠️):**
- Hardware backends (TensorRT, QNN, TIDL, AMD Vitis) — stubs only.
- LiDAR / radar / fused-feature sensor mappings — only camera + odom today.
- Bilinear camera resize.
- Live posture transport for the node (**M2c**) — `KIRRA_POSTURE_STREAM_URL`
  wiring; the env var is comment-only and unread today.
- Backend selection in the node binary (it always uses `MockBackend`).
- ROS message shims for camera/odom (`sensor_msgs/Image`, `nav_msgs/Odometry`).
- Per-platform kinematics-contract / ODD-cap injection into `KirraGovernor`.
- SOTIF angular-velocity numbers (DRAFT — pending safety-engineer review).

---

## 8. Accuracy cross-check

Every interface named in this guide, confirmed against the code:

| Symbol | Location | Status |
|---|---|---|
| `InferenceBackend`, `TensorBatch`, `TensorStorage`, `ModelHandle`, `BackendDescriptor` | `parko-core/src/backend.rs` | ✅ confirmed |
| `OrtBackend::new` | `parko-onnx/src/lib.rs` | ✅ |
| `OvBackend::new`, `BackendDescriptor::IntelOpenVino` | `parko-openvino/src/lib.rs` | ✅ |
| stub backends behind `backend-{tensorrt,qnn,tidl,openvino,amd}` | `parko-core/src/backends/mod.rs` | ✅ (stubs only) |
| `SensorInputMapping`, `CameraMapping/Config/Encoding/Normalization/Layout/Resize`, `CameraSample`, `OwnedCameraSample`, `CameraMappingError`, `OdomMapping/Config/Orientation`, `OdomSample`, `VectorMapping` | `parko-ros2/src/sensor_mapping.rs` | ✅ |
| `ControlCommand{linear_velocity,angular_velocity,timestamp_ms}`, `::stopped` | `parko-core/src/commands.rs` | ✅ |
| output tensor names `cmd_vel_linear` / `cmd_vel_angular`, NaN→stopped | `parko-core/src/scheduler.rs` | ✅ |
| `OutgoingTwist{linear_x_mps,angular_z_rads,stamp_ms}`, `enforce_outgoing_twist` | `parko-ros2/src/command_mapping.rs` | ✅ |
| `SafetyGovernor`, `EnforcementAction::{Allow,ClampLinearVelocity,ClampAngularVelocity,ClampMotion,Deny}`, `SafetyPosture` | `parko-core/src/safety.rs` | ✅ |
| `KirraGovernor::{new,nominal,mrc_fallback,for_posture,with_platform_params,with_angular_bounds,update_rss_state}`, `MRC_VELOCITY_CEILING_MPS=5.0`, `GovernorComparator` | `parko-kirra/src/lib.rs`, `comparator.rs` | ✅ |
| `AngularVelocityBound::{omega_max,nominal,mrc,Scalar}`, `PlatformParams` + fields + `conservative_default`/`urban_service_robot_reference`, `ROLLOVER_MIN_LINEAR_VELOCITY_MPS=0.05` | `parko-kirra/src/angular_bound.rs` | ✅ |
| `VehicleKinematicsContract` + fields, `nominal_reference_profile`(35) / `mrc_fallback_profile`(5), `effective_max_speed_mps`, `odd_speed_cap_mps`, `URBAN_ODD_SPEED_CAP_MPS=22.35`, `validate_vehicle_command`, `EnforceAction`, `ProposedVehicleCommand`, `DenyCode` | `src/gateway/kinematics_contract.rs` | ✅ (note: **not** injectable into `KirraGovernor`) |
| `RssState`, `longitudinal_safe_distance`, `lateral_safe_distance`, `RSS_FAILSAFE_DISTANCE_M=1e6` | `parko-core/src/rss.rs` | ✅ |
| `PostureTracker::{nominal_default_no_source,with_source,observe,current_posture}`, `POSTURE_STALENESS_TIMEOUT_MS=6000` | `src/posture_tracker.rs` | ✅ |
| `ParkoPostureState::{no_source,with_source,observe,current_fleet_posture,current_safety_posture}`, `fleet_to_safety` | `parko-ros2/src/posture_state.rs` | ✅ |
| `ParkoNodeConfig{sensor_topic,command_topic,tick_period_s,sensor_staleness_budget_ms,mrc_command}`, defaults `~/input/observation` / `~/output/cmd_vel` / 0.05 / 200 | `parko-ros2/src/config.rs` | ✅ |
| `run_node(config,infer,mapping,posture,node_name)`, topics `std_msgs/msg/Float32MultiArray` (in) / `geometry_msgs/msg/Twist` (out) | `parko-ros2/src/node.rs` | ✅ (Sample = Vec<f32>) |
| `run_pipeline_tick`, `run_pipeline_tick_with_posture_state`, `TickError`, `TickOutcome` | `parko-ros2/src/tick_pipeline.rs` | ✅ |
| binary `parko_ros2_node`, env `PARKO_MODEL_PATH`, `ComparatorAsGovernor` | `parko-ros2/src/bin/parko_ros2_node.rs`, `comparator_adapter.rs` | ✅ (uses MockBackend) |

**Could NOT confirm in code (flagged PLANNED above, not used as if working):**
- `KIRRA_POSTURE_STREAM_URL` and any `KIRRA_POSTURE_*` env var — present **only
  in comments**; no `env::var` reads it. (Only `PARKO_MODEL_PATH` is read.)
- ROS message shims `image_msg_to_sample` / `sensor_msgs/Image` /
  `nav_msgs/Odometry` decode — described in comments / planned, not built.
- Any `KirraGovernor` setter for a custom `VehicleKinematicsContract` or ODD
  cap — does not exist.

No invented APIs are used in this document.

---

## 9. Worked example — camera policy on the urban reference platform

A minimal end-to-end integration: a vision policy that takes a 224×224 RGB
image and emits `(cmd_vel_linear, cmd_vel_angular)`, on a TurtleBot-4-class
robot. All values are real field names you could type out.

**(a) Sensor mapping** — ImageNet-normalized NCHW, tensor name matching the
model's input node (`"input"`):

```rust
use parko_ros2::{CameraConfig, CameraEncoding, CameraLayout,
                 CameraMapping, CameraNormalization, CameraResize};

let mapping = std::sync::Arc::new(CameraMapping::new(CameraConfig {
    encoding:      CameraEncoding::Bgr8,      // ROS sensor_msgs/Image is usually bgr8
    target_height: 224,
    target_width:  224,
    resize:        CameraResize::Nearest,
    normalization: CameraNormalization::MeanStd {
        mean: vec![0.485, 0.456, 0.406],
        std:  vec![0.229, 0.224, 0.225],
    },
    layout:        CameraLayout::Nchw,        // [1, 3, 224, 224]
    tensor_name:   "input".to_string(),       // == the ONNX input node name
}));
```

**(b) Angular geometry** — the urban reference platform:

```rust
use parko_kirra::{KirraGovernor, GovernorComparator, PlatformParams};

let governor = || KirraGovernor::new()
    .with_platform_params(PlatformParams::urban_service_robot_reference());
//   track 0.50 m, CoG 0.40 m, extent 0.30 m, v_edge_safe 0.25 m/s,
//   theta_max 0.087 rad, ftti 0.10 s  →  ω_max(0) ≈ 0.833 rad/s
let comparator = GovernorComparator::new(governor(), governor());
```

**(c) Linear kinematics** — *as built*, the parko-kirra governor uses
`nominal_reference_profile()` (35 m/s) / `mrc_fallback_profile()` (5 m/s); a
slow service robot will essentially never approach the 35 m/s ceiling, so the
binding linear limits in practice are the accel/brake rates (2.5 / 4.5 m/s²)
and, in Degraded, the 5 m/s MRC cap. (A tighter per-platform contract / ODD
cap is PLANNED — §7.)

**(d) Backend + loop + run:**

```rust
use std::sync::Arc;
use parko_core::scheduler::InferenceLoop;
use parko_core::safety::SafetyPosture;
use parko_core::backend::InferenceBackend;
use parko_ros2::{ParkoNodeConfig, ComparatorAsGovernor};
use parko_ros2::node::run_node;          // requires the `ros2` feature
use tokio::sync::{mpsc, Mutex};

// ORT_DYLIB_PATH must point at libonnxruntime.so v1.24.x at runtime.
let backend = Arc::new(parko_onnx::OrtBackend::new("/models/vision_policy.onnx")?);
let model   = backend.load_model("/models/vision_policy.onnx")?;
let (actuator_tx, _rx) = mpsc::channel(8);

let infer = Arc::new(Mutex::new(
    InferenceLoop::new(backend, model, actuator_tx)
        .with_governor(ComparatorAsGovernor(comparator))
        .with_tick_period(0.05)));        // 20 Hz → delta_time_s = 0.05

let config = Arc::new(ParkoNodeConfig {
    sensor_topic:  "/camera/image_raw".to_string(),
    command_topic: "/cmd_vel".to_string(),
    tick_period_s: 0.05,
    sensor_staleness_budget_ms: 100,      // tightened for a 20 Hz camera
    ..ParkoNodeConfig::default()
});

run_node(config, infer, mapping, SafetyPosture::Nominal, "parko_governor").await?;
```

The model output is `cmd_vel_linear` / `cmd_vel_angular`; the governor caps
linear to the contract and angular to ω_max(v); the node publishes the gated
`geometry_msgs/Twist`. A stale camera frame, a NaN output, or a `LockedOut`
posture all resolve to a stopped twist (§6).

> Reminder for this worked example: feeding `CameraMapping` through the live
> `run_node` needs the camera ROS shim (PLANNED). Today you would either run
> the camera transform through a `Sample = Vec<f32>` adapter or drive
> `CameraMapping::to_tensor` directly in a CARLA / bag-replay harness; the
> config values above are exactly what you'd use.
