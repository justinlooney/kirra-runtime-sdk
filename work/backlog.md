# Backlog

> 40 tasks. Every coding task has a Claude Code Prompt ready to paste.
> Pull into active.md (max 3). Move to done.md on merge.

---

## PARK-001 `control-loop` `feat` `epic:runtime-core`

**Attach `SafetyGovernor` to `ControlLoop`**

Add `with_governor(impl SafetyGovernor + 'static)` builder to `ControlLoop` in
`parko-core`. The governor is stored as `Option<Box<dyn SafetyGovernor>>`. When
present, the built-in scalar clamp is suppressed entirely — both enforcement paths
must not run on the same tick.

### Claude Code Prompt
```
In parko/parko-core/src/control_loop.rs, implement the with_governor builder.

Requirements:
- Add field: governor: Option<Box<dyn SafetyGovernor>> to ControlLoop struct
- Add method: pub fn with_governor(mut self, g: impl SafetyGovernor + 'static) -> Self
- In tick(): if self.governor.is_some() { skip built-in clamp; call governor }
              else { run built-in clamp as before }
- Add test test_builtin_clamp_suppressed:
    1. Create a PassthroughGovernor that returns the input unchanged
    2. Set input to a value above the built-in clamp threshold
    3. Assert output == input (governor passthrough, not clamped)
- Add test test_no_governor_uses_builtin_clamp:
    1. Create ControlLoop without governor
    2. Send a value above threshold
    3. Assert output == clamped_value
- No unsafe code. All existing parko-core tests must pass.
- Files: parko/parko-core/src/control_loop.rs, parko/parko-core/src/lib.rs
```

---

## PARK-002 `control-loop` `feat` `epic:runtime-core`

**Add test-only posture state setter**

Add `set_state_for_test(state: PostureState)` to `parko-core` behind `#[cfg(test)]`.
This is a test seam — it sets internal posture state without transition validation.
Absent from release builds; verified with `nm`.

### Claude Code Prompt
```
In parko/parko-core/src/control_loop.rs (or posture.rs), add a cfg(test) method.

Requirements:
- #[cfg(test)]
  pub fn set_state_for_test(&mut self, state: PostureState)
- Sets internal posture state directly, no validation
- Doc comment: "Test-only seam. Not compiled into release builds."
- Add test: call set_state_for_test(PostureState::Degraded), call tick(),
  assert governor (or clamp) received PostureState::Degraded
- All existing tests pass
- Files: parko/parko-core/src/control_loop.rs
```

---

## PARK-003 `control-loop` `test` `epic:runtime-core`

**Posture divergence property test**

Proptest suite: for all valid `(proposed_output: f32, posture_state: PostureState)`
the `KirraKernelGovernor` result is at least as conservative as the built-in clamp.
Core invariant for the governor integration.

### Claude Code Prompt
```
Create parko/parko-core/tests/posture_divergence.rs.

Requirements:
- Use proptest to generate (proposed: f32, state: PostureState)
  filtered to exclude NaN and Inf
- For each pair:
    a. Run proposed through ControlLoop without governor -> builtin_result
    b. Use set_state_for_test to inject state; run through KirraKernelGovernor
       (via parko-aegis adapter or direct) -> governor_result
    c. Assert for Nominal/Degraded: governor_result <= builtin_result
    d. Assert for LockedOut: governor_result == 0.0 or Halt
- proptest config: cases = 10_000
- Three separate proptest! blocks, one per PostureState variant
- No new non-dev dependencies
- Files: parko/parko-core/tests/posture_divergence.rs
```

---

## PARK-004 `control-loop` `safety` `epic:runtime-core`

**NaN/Inf rejection at tick boundary**

Input guard at the top of `ControlLoop::tick`: any NaN or Inf returns
`EnforcementAction::Halt` before reaching governor or clamp. Prevents undefined
behavior propagating through the safety stack.

### Claude Code Prompt
```
In parko/parko-core/src/control_loop.rs, add an input guard at the top of tick().

Requirements:
- Before any governor or clamp logic:
    if input.iter().any(|v| v.is_nan() || v.is_infinite()) {
        return EnforcementAction::Halt;
    }
- Add proptest generating adversarial f32 (NaN, Inf, -Inf, subnormals):
  assert all NaN/Inf inputs -> Halt, no panic
- Add unit test: normal f32 still reaches governor unchanged
- No change to governor or clamp logic
- Files: parko/parko-core/src/control_loop.rs
```

---

## PARK-005 `control-loop` `feat` `epic:runtime-core`

**`VirtualClock` integration in `ControlLoop`**

Wire the `Clock` trait (`VirtualClock` / `SystemClock`) from `kirra-runtime-sdk`
into `parko-core`'s `ControlLoop`. Enables deterministic temporal tests without sleep.

### Claude Code Prompt
```
In parko/parko-core/src/control_loop.rs, wire the Clock trait into ControlLoop.

Requirements:
- Add field: clock: Box<dyn Clock + Send + Sync>
- Add builder: pub fn with_clock(mut self, c: impl Clock + Send + Sync + 'static) -> Self
- Default to SystemClock in ControlLoop::new()
- Import Clock from kirra_runtime_sdk::clock (add workspace dependency) or
  re-define a minimal Clock trait in parko-core if circular dep is a concern
- Add test: use VirtualClock, call advance(tick_period_ms), assert tick fired
  correct number of times without any sleep
- Files: parko/parko-core/src/control_loop.rs, parko/parko-core/Cargo.toml
```

---

## PARK-006 `chore` `epic:runtime-core`

**`parko-core` v0.1.0 release tag**

Set version to `0.1.0` in `parko/parko-core/Cargo.toml`. Verify
`cargo publish --dry-run -p parko-core` exits cleanly. Tag `parko-core-v0.1.0`.

*(No code — version bump and tagging only.)*

---

## PARK-007 `control-loop` `feat` `epic:hal`

**Define `BackendDescriptor` enum**

`BackendDescriptor { Cpu, QualcommQnn, TiTidl, AmdRocm, IntelOpenVino }` in
`parko-core`. Re-exported from crate root. Derive `Debug, Clone, PartialEq, Eq, Hash`.

### Claude Code Prompt
```
In parko/parko-core/src/backend.rs (create), define BackendDescriptor.

Requirements:
- #[derive(Debug, Clone, PartialEq, Eq, Hash)]
  pub enum BackendDescriptor { Cpu, QualcommQnn, TiTidl, AmdRocm, IntelOpenVino }
- Re-export from parko_core lib.rs: pub use backend::BackendDescriptor;
- Unit test: round-trip each variant through format!("{:?}", v)
- No new dependencies
- Files: parko/parko-core/src/backend.rs, parko/parko-core/src/lib.rs
```

---

## PARK-008 `backend-qnn` `feat` `epic:hal`

**QNN stub backend**

`QnnStubBackend`: deterministic zero outputs, gated behind
`features = ["backend-qnn"]`. CI passes without Qualcomm hardware.

### Claude Code Prompt
```
In parko/parko-core/src/backends/qnn_stub.rs (create backends/ module), implement
QnnStubBackend.

Requirements:
- #[cfg(feature = "backend-qnn")]
- Implement InferenceBackend trait:
    fn run(&self, input: &[f32], output: &mut [f32]) -> Result<(), BackendError>
      { output.iter_mut().for_each(|v| *v = 0.0); Ok(()) }
    fn backend_descriptor(&self) -> BackendDescriptor { BackendDescriptor::QualcommQnn }
- Add "backend-qnn" as optional feature in parko/parko-core/Cargo.toml
- Test: create QnnStubBackend, run with 8-element input, assert all output zeros
- Files: parko/parko-core/src/backends/qnn_stub.rs
         parko/parko-core/src/backends/mod.rs (create)
         parko/parko-core/Cargo.toml
```

---

## PARK-009 `backend-tidl` `feat` `epic:hal`

**TIDL stub backend**

`TidlStubBackend` with configurable simulated DSP latency (default 2 ms via sleep).
Gated behind `features = ["backend-tidl"]`.

### Claude Code Prompt
```
In parko/parko-core/src/backends/tidl_stub.rs, implement TidlStubBackend.

Requirements:
- pub struct TidlStubBackend { latency_ms: u64 }
- impl TidlStubBackend { pub fn new(latency_ms: u64) -> Self }
- InferenceBackend::run: sleep(Duration::from_millis(self.latency_ms));
  fill output with 0.0; return Ok(())
- BackendDescriptor::TiTidl from backend_descriptor()
- Gate behind #[cfg(feature = "backend-tidl")]
- Add feature "backend-tidl" in Cargo.toml
- Test: TidlStubBackend::new(0), run, assert zeros, elapsed < 10ms
- Files: parko/parko-core/src/backends/tidl_stub.rs, Cargo.toml
```

---

## PARK-010 `backend-openvino` `feat` `epic:hal`

**OpenVINO stub backend**

`OpenVinoStubBackend`: zero outputs, gated behind `features = ["backend-openvino"]`.
Real `OpenVinoBackend` is PARK-014.

### Claude Code Prompt
```
In parko/parko-core/src/backends/openvino_stub.rs, implement OpenVinoStubBackend.

Requirements:
- Return zeros, no real OpenVINO calls
- BackendDescriptor::IntelOpenVino
- Gate behind #[cfg(feature = "backend-openvino")]
- Add "backend-openvino" as optional feature
- Test: run, assert zeros, no panic
- Files: parko/parko-core/src/backends/openvino_stub.rs, Cargo.toml
```

---

## PARK-011 `backend-rocm` `feat` `epic:hal`

**ROCm stub backend**

`RocmStubBackend`: zero outputs, gated behind `features = ["backend-rocm"]`.

### Claude Code Prompt
```
In parko/parko-core/src/backends/rocm_stub.rs, implement RocmStubBackend.

Requirements:
- Return zeros
- BackendDescriptor::AmdRocm
- Gate behind #[cfg(feature = "backend-rocm")]
- Add "backend-rocm" as optional feature
- Test: run, assert zeros
- Files: parko/parko-core/src/backends/rocm_stub.rs, Cargo.toml
```

---

## PARK-012 `control-loop` `safety` `epic:hal`

**Backend latency watchdog**

`InferenceLoop` measures wall-clock time per `backend.run()`. Exceeding
`deadline_ms` emits `LatencyViolation`, holds last safe output. Three consecutive
violations → posture `Degraded`.

### Claude Code Prompt
```
In parko/parko-core/src/inference_loop.rs (or control_loop.rs), add a latency
watchdog.

Requirements:
- Add field: deadline_ms: Option<u64>
- Add builder: pub fn with_deadline(mut self, ms: u64) -> Self
- On each tick: record start = clock.now_ms(); call backend.run();
  elapsed = clock.now_ms() - start;
  if elapsed > deadline_ms: emit LatencyViolation { backend, elapsed_ms, deadline_ms };
  return last_safe_output (cached from previous tick)
- Track consecutive_violations: u32; if >= 3: set posture to Degraded
- Reset counter on a successful tick
- Add test: TidlStubBackend latency_ms=5, deadline_ms=1, run 3 times, assert
  posture == Degraded
- Add test: one slow tick then fast tick, assert posture returns to Nominal
- Files: parko/parko-core/src/inference_loop.rs
```

---

## PARK-013 `chore` `backend-qnn` `epic:hal`

**CI matrix: all four stub backends**

GitHub Actions matrix job building and testing all four backend features on
ubuntu-latest in one workflow run.

### Claude Code Prompt
```
Add or update .github/workflows/ci.yml to include a matrix job for stub backends.

Requirements:
- Add job: test-backends
  strategy:
    matrix:
      features: [backend-qnn, backend-tidl, backend-openvino, backend-rocm]
  steps:
    - cargo test -p parko-core --features ${{ matrix.features }}
- Job must pass on ubuntu-latest without any hardware
- Do not remove or break existing CI jobs
- Files: .github/workflows/ci.yml
```

---

## PARK-014 `backend-openvino` `feat` `epic:hal`

**Real `OpenVinoBackend`**

`OpenVinoBackend` using `openvino-rs` with model loading and output shape
validation. Integration test with trivial identity model.

### Claude Code Prompt
```
In parko/parko-core/src/backends/openvino.rs (create alongside openvino_stub.rs),
implement the real OpenVinoBackend.

Requirements:
- Add optional dependency: openvino = { version = "0.7", optional = true }
  under features = ["backend-openvino"] in Cargo.toml
- pub struct OpenVinoBackend { core: openvino::Core, compiled: openvino::CompiledModel,
    input_size: usize, output_size: usize }
- OpenVinoBackend::new(model_xml: &str, model_bin: &str) -> Result<Self, BackendError>
- run: validate input.len() == input_size; create InferRequest; set_tensor;
  infer; read output tensor into output slice
- Return BackendError::ShapeMismatch for wrong buffer sizes
- Integration test (not #[ignore]; use a tiny 2-input/2-output identity model
  included as test fixture in tests/fixtures/identity.xml + identity.bin):
  assert output == input within 1e-6
- Gate entire struct behind #[cfg(all(feature = "backend-openvino",
  not(feature = "openvino-stub")))]
- Files: parko/parko-core/src/backends/openvino.rs
         parko/parko-core/tests/fixtures/ (model files)
```

---

## PARK-015 `behavioral-safety` `safety` `epic:behavioral-safety`

**`RssSafeDistance::longitudinal`**

IEEE 2846-2022 §5.1 longitudinal safe-distance formula in `parko-core::rss`.

### Claude Code Prompt
```
Create parko/parko-core/src/rss.rs and implement longitudinal safe distance.

Requirements:
- pub fn longitudinal_safe_distance(
      ego_vel: f64, lead_vel: f64,
      reaction_time: f64, accel_max: f64,
      brake_min: f64, brake_max: f64,
  ) -> f64
- Formula (IEEE 2846-2022 §5.1):
    d_response = ego_vel * reaction_time + 0.5 * accel_max * reaction_time.powi(2)
    v_after_response = ego_vel + accel_max * reaction_time
    d_brake_ego = v_after_response.powi(2) / (2.0 * brake_min)
    d_brake_lead = lead_vel.powi(2) / (2.0 * brake_max)
    d_min = d_response + d_brake_ego - d_brake_lead
    return d_min.max(0.0)
- Unit tests: equal speeds (margin > 0), ego faster (larger margin),
  ego slower (margin 0), zero speed both (margin 0), very high speed (no overflow)
- Add pub mod rss; to lib.rs
- Files: parko/parko-core/src/rss.rs, parko/parko-core/src/lib.rs
```

---

## PARK-016 `behavioral-safety` `safety` `epic:behavioral-safety`

**`RssSafeDistance::lateral`**

IEEE 2846-2022 §5.2 lateral safe-distance formula.

### Claude Code Prompt
```
In parko/parko-core/src/rss.rs, add lateral safe distance.

Requirements:
- pub fn lateral_safe_distance(
      ego_lat_vel: f64, obj_lat_vel: f64,
      lat_accel_max: f64, reaction_time: f64,
  ) -> f64
- Formula: compute relative lateral velocity, reaction distance for both,
  braking distances, return max(0.0, combined margin) per IEEE 2846-2022 §5.2
- Unit tests: converging fast (large margin), diverging (margin 0), stationary
- Files: parko/parko-core/src/rss.rs
```

---

## PARK-017 `posture-engine` `safety` `epic:behavioral-safety`

**`RssState` and posture integration**

`RssState { safe, longitudinal_margin, lateral_margin }` wired into
`kirra-runtime-sdk` posture engine. Violation → Degraded; recovery uses existing
5-tick / 10 s hysteresis.

### Claude Code Prompt
```
In kirra-runtime-sdk/src/posture_engine.rs and posture_engine_v2.rs, integrate
RssState into the posture pipeline.

Requirements:
- Define: pub struct RssState { pub safe: bool, pub longitudinal_margin: f64,
    pub lateral_margin: f64 }
- Add PostureRecalcTrigger::RssViolation to the trigger enum
- In start_posture_engine_worker: handle RssViolation trigger ->
  call recalculate_and_broadcast
- In derive_fleet_posture: if any active RssViolation trigger ->
  return FleetPosture::Degraded
- Recovery: use existing AV hysteresis (AV_RECOVERY_STREAK_THRESHOLD=5,
  AV_RECOVERY_WINDOW_MS=10_000) — an RssViolation resets the streak to 0
- Integration test using ScenarioRunner:
  inject RssState { safe: false } -> assert Degraded
  inject 5x RssState { safe: true } within 10s -> assert Nominal
- Files: kirra-runtime-sdk/src/posture_engine.rs
         kirra-runtime-sdk/src/posture_engine_v2.rs
         kirra-runtime-sdk/src/verifier.rs (add RssState field to AppState)
```

---

## PARK-018 `aegis-integration` `safety` `epic:behavioral-safety`

**Wire RSS into `KirraKernelGovernor`**

RSS pre-actuator gate: `rss_state.safe == false` clamps velocity to 0.0 before
any kinematics envelope check.

### Claude Code Prompt
```
In kirra-runtime-sdk/src/kirra_core.rs, add RSS gating to KirraKernelGovernor.

Requirements:
- Add field: rss_state: RssState (default safe=true) to KirraKernelGovernor
- Add method: pub fn update_rss_state(&mut self, state: RssState)
- In enforce() (or equivalent): FIRST line — if !self.rss_state.safe { clamp vel to 0.0; return }
  Then continue with kinematics envelope
- Unit test A: set rss_state.safe=false, send vel=5.0, assert output vel==0.0
- Unit test B: set rss_state.safe=true, send vel=5.0, assert kinematics applies
  normally (not zeroed by RSS)
- Do not change constructor signature
- Files: kirra-runtime-sdk/src/kirra_core.rs
```

---

## PARK-019 `behavioral-safety` `test` `epic:behavioral-safety`

**RSS property test**

Proptest: for all valid `(ego_vel, lead_vel, gap, commanded_vel)`, no
RSS-violating command exits the governor for any posture state.

### Claude Code Prompt
```
Create parko/parko-core/tests/rss_property.rs.

Requirements:
- proptest generates (ego_vel: f64, lead_vel: f64, gap: f64, commanded_vel: f64)
  filtered to physically valid ranges: all >= 0.0, gap > 0.0, vel < 150.0 m/s
- For each tuple:
    safe_dist = longitudinal_safe_distance(ego_vel, lead_vel, 0.5, 3.0, 6.0, 8.0)
    rss_safe = gap >= safe_dist
    governor_output = run_through_governor(rss_safe, commanded_vel)
    if !rss_safe: assert governor_output.vel == 0.0
    if rss_safe: assert governor_output.vel <= commanded_vel (kinematics may clamp)
- 10_000 cases; all three PostureState variants covered
- Use parko-aegis KirraKernelGovernor as the SafetyGovernor
- Files: parko/parko-core/tests/rss_property.rs
```

---

## PARK-020 `aegis-integration` `safety` `epic:behavioral-safety`

**`RssViolationEvent` in audit chain**

`RssViolationEvent { ego_vel, lead_vel, gap, longitudinal_margin, lateral_margin,
timestamp_ms }` in the hash-chained audit ledger in `kirra-runtime-sdk`.

### Claude Code Prompt
```
In kirra-runtime-sdk/src/audit_chain.rs, add RssViolationEvent to the audit chain.

Requirements:
- pub struct RssViolationEvent { pub ego_vel: f64, pub lead_vel: f64,
    pub gap: f64, pub longitudinal_margin: f64, pub lateral_margin: f64,
    pub timestamp_ms: u64 }
- Add AuditEntry::RssViolation(RssViolationEvent) to the entry enum
- Implement serialization matching existing chain format
- Add to AuditChainLinker:
    pub fn append_rss_violation(&mut self, e: RssViolationEvent)
      -> Result<(), AuditError>
- Include event in SHA-256 chain hash
- Test: append 5 entries including one RssViolation, call verify_chain(),
  assert no error; corrupt the RssViolation entry, assert verify_chain() errors
- Files: kirra-runtime-sdk/src/audit_chain.rs
```

---

## PARK-021 `simulation` `test` `epic:behavioral-safety`

**10 000 adversarial trajectory simulation**

`ScenarioRunner` + `VirtualClock` simulation: 10 000 scenarios mixing safe and
unsafe RSS gaps through the full stack. Assert zero unsafe commands exit.

### Claude Code Prompt
```
Create kirra-runtime-sdk/tests/rss_simulation.rs.

Requirements:
- Use ScenarioRunner from kirra_runtime_sdk::scenario_runner
- Use VirtualClock; do not sleep
- Generate 10_000 scenarios: each is a sequence of 10 ticks with varying
  ego_vel, lead_vel, gap, commanded_vel (some gaps below safe distance)
- For each tick in each scenario: compute RssState, feed into posture engine
  and governor, record output vel
- Assert: for every tick where gap < safe_distance, output vel == 0.0
- Assert: posture correctly degrades and recovers (5-tick streak)
- Test completes in < 60 s on CI
- Files: kirra-runtime-sdk/tests/rss_simulation.rs
```

---

## PARK-022 `backend-qnn` `feat` `epic:silicon-matrix` `needs-hardware`

**Real `QnnBackend`**

`QnnBackend` via Qualcomm AI Engine Direct SDK C FFI. Internal quantization for
int8 models. Hardware test (`#[ignore]`) vs. ORT CPU reference on QCS6490/SA8295.

### Claude Code Prompt
```
In parko/parko-core/src/backends/qnn.rs (create), implement the real QnnBackend.

Requirements:
- Use Qualcomm QNN SDK C FFI: Qnn_Interface_t, QnnBackend_Config_t,
  QnnContext_Config_t, QnnTensor_t
- pub struct QnnBackend { /* context, graph, input/output tensor handles */ }
- QnnBackend::new(model_path: &str) -> Result<Self, BackendError>
  loads a .serialized QNN context binary
- run: populate input tensor from &[f32]; if model requires int8,
  quantize using scale/offset from tensor metadata; execute graph;
  dequantize output into &mut [f32]
- BackendDescriptor::QualcommQnn
- Gate entire file behind #[cfg(feature = "backend-qnn")]
- Hardware test marked #[ignore]:
    #[ignore] #[test] fn test_qnn_mobilenet_v2()
    Load MobileNetV2 model, run on random input, compare top-1 with ORT CPU ref
- Files: parko/parko-core/src/backends/qnn.rs
```

---

## PARK-023 `backend-tidl` `feat` `epic:silicon-matrix` `needs-hardware`

**Real `TidlBackend`**

`TidlBackend` via TI TIDL runtime C FFI, cross-compiled to
`aarch64-unknown-linux-gnu`. Target: TDA4VM.

### Claude Code Prompt
```
In parko/parko-core/src/backends/tidl.rs, implement the real TidlBackend.

Requirements:
- Use TI TIDL C FFI (tivxTIDLNode, TIDL_IOBufDesc_t)
- Cross-compile target: aarch64-unknown-linux-gnu (use cross or cargo-cross)
- TidlBackend::new(model_path: &str) -> Result<Self, BackendError>
- run: copy &[f32] to TIDL input buffer; execute node; copy output to &mut [f32]
- BackendDescriptor::TiTidl
- Gate behind #[cfg(feature = "backend-tidl")]
- Hardware test marked #[ignore]: compare output within 1e-3 of ORT CPU ref
- Files: parko/parko-core/src/backends/tidl.rs
         parko/parko-core/build.rs (for C FFI linking)
```

---

## PARK-024 `backend-rocm` `feat` `epic:silicon-matrix` `needs-hardware`

**Real `RocmBackend`**

`RocmBackend` via MIGraphX Rust bindings. GPU memory allocated once at init.
Target: AMD RX 6000 or MI100.

### Claude Code Prompt
```
In parko/parko-core/src/backends/rocm.rs, implement RocmBackend.

Requirements:
- Use migraphx crate (add as optional dep) or raw HIP C FFI
- RocmBackend::new(model_path: &str) -> Result<Self, BackendError>
  Parse ONNX model via migraphx::Program; compile for GPU
- run: copy input to device; execute program; copy output to host &mut [f32]
- GPU buffer allocated once at init; not per inference
- BackendDescriptor::AmdRocm
- Gate behind #[cfg(feature = "backend-rocm")]
- Hardware test marked #[ignore]
- Files: parko/parko-core/src/backends/rocm.rs
```

---

## PARK-025 `control-loop` `feat` `epic:silicon-matrix`

**`BackendSelector`: runtime backend selection**

`BackendSelector::new(BackendDescriptor)` creates the correct backend, falling
back to the stub (with `tracing::warn!`) when the target is unavailable.

### Claude Code Prompt
```
In parko/parko-core/src/backend_selector.rs (create), implement BackendSelector.

Requirements:
- pub struct BackendSelector { descriptor: BackendDescriptor,
    backend: Box<dyn InferenceBackend> }
- BackendSelector::new(d: BackendDescriptor) -> Result<Self, BackendError>:
    QualcommQnn -> try QnnBackend::new, on Err or feature absent -> QnnStubBackend
    TiTidl -> try TidlBackend::new, fallback TidlStubBackend
    IntelOpenVino -> try OpenVinoBackend::new, fallback OpenVinoStubBackend
    AmdRocm -> try RocmBackend::new, fallback RocmStubBackend
    Cpu -> OrtBackend (always available)
  Log tracing::warn!("Backend {:?} unavailable, using stub", d) on fallback
- Test: BackendSelector::new(QualcommQnn) on CI (no hardware) -> Ok, descriptor is QualcommQnn
- Files: parko/parko-core/src/backend_selector.rs
         parko/parko-core/src/lib.rs (pub use backend_selector::BackendSelector)
```

---

## PARK-026 `simulation` `test` `epic:silicon-matrix`

**Cross-backend determinism validation**

Same fixed input on ORT + QnnStub + TidlStub. Outputs within 1e-5 element-wise.

### Claude Code Prompt
```
Create parko/parko-core/tests/cross_backend_determinism.rs.

Requirements:
- Define FIXED_INPUT: [f32; 128] = [/* constant values, e.g. 0.1 * i as f32 */]
- Run FIXED_INPUT through: OrtBackend, QnnStubBackend, TidlStubBackend
- Assert all three output slices are within 1e-5 of each other element-wise
- Comment: "Stubs return zeros so this validates stub contract.
  Update tolerance when real backends are available (see PARK-022, PARK-023)"
- Test must run on CI without hardware
- Files: parko/parko-core/tests/cross_backend_determinism.rs
```

---

## PARK-027 `packaging` `feat` `epic:packaging`

**Unified `kirra_safety_runtime` binary**

Merges `kirra-runtime-sdk` posture engine + `parko-core` inference loop. Configured
by env vars. Serves `/health`.

### Claude Code Prompt
```
Create kirra-runtime-sdk/src/bin/kirra_safety_runtime.rs.

Requirements:
- Read env vars: KIRRA_ADMIN_TOKEN, KIRRA_DB_PATH, KIRRA_VERIFIER_ADDR (fail closed
  if missing, same as kirra_verifier_service), KIRRA_BACKEND (default "ort"),
  KIRRA_TICK_RATE_HZ (default 100)
- Create BackendSelector from KIRRA_BACKEND value
- Start axum HTTP service reusing all routes from kirra_verifier_service.rs
- Start InferenceLoop with selected backend at tick rate KIRRA_TICK_RATE_HZ
- Wire InferenceLoop posture output into PostureEngineSender
- GET /health: returns 200 JSON {"status":"ok","backend":"<name>","tick_hz":N}
- GET /inference/status: returns {"tick_rate_hz":f64,"backend":String,"p99_latency_ms":f64}
- All existing kirra_verifier_service tests must still compile and pass
- Files: kirra-runtime-sdk/src/bin/kirra_safety_runtime.rs
```

---

## PARK-028 `packaging` `chore` `epic:packaging`

**systemd unit with watchdog**

`scripts/kirra-safety-runtime.service` with `WatchdogSec=5`, `MemoryMax=512M`,
`CPUQuota=80%`. Restarts automatically on watchdog timeout.

*(No code — service unit file only. Verify with `systemd-analyze verify`.)*

---

## PARK-029 `packaging` `chore` `epic:packaging`

**Backend-aware installer**

`install.sh --backend <ort|qnn|tidl|openvino|rocm>`. Non-interactive with `--yes`.
Downloads correct binary; configures systemd unit.

*(Update to existing `install.sh` — no Claude Code Prompt needed; bash script work.)*

---

## PARK-030 `packaging` `feat` `epic:packaging`

**Dashboard inference panels**

Tick rate, backend P99 latency, RSS margin, posture sparkline panels in React dashboard.

### Claude Code Prompt
```
In dashboard/src/components/ (create new files), add four panels to the dashboard.

Requirements:
- InferenceTickPanel.tsx: polls GET /inference/status every 2s; renders tick_rate_hz
  and p99_latency_ms; shows "—" when endpoint unreachable
- RssMarginPanel.tsx: polls GET /fleet/rss/status every 1s; renders longitudinal_margin
  and lateral_margin as progress bars (red if safe==false)
  (you will need to add GET /fleet/rss/status to kirra_safety_runtime returning
  { longitudinal_margin: f64, lateral_margin: f64, safe: bool })
- PostureSparklinePanel.tsx: reads last 60 posture events from SSE
  /system/posture/stream or GET /fleet/history; renders 60-point sparkline
  (green=Nominal, yellow=Degraded, red=LockedOut)
- BackendLatencyPanel.tsx: buffers last 100 p99_latency_ms samples from
  /inference/status; renders a mini histogram
- Wire all four panels into App.tsx or the main dashboard layout
- All panels handle fetch errors gracefully (no crash, show "—" or "Offline")
- Use existing dashboard styling patterns; no new UI libraries
- Files: dashboard/src/components/InferenceTickPanel.tsx
         dashboard/src/components/RssMarginPanel.tsx
         dashboard/src/components/PostureSparklinePanel.tsx
         dashboard/src/components/BackendLatencyPanel.tsx
         dashboard/src/App.tsx (or layout file)
```

---

## PARK-031 `packaging` `chore` `epic:packaging`

**`v1.2.0` release pipeline**

CI matrix builds backend variants for x86_64, aarch64, armv7. All attached to
GitHub Release with SHA256SUMS.

### Claude Code Prompt
```
Update .github/workflows/release.yml to support backend variants.

Requirements:
- Add backend to the build matrix:
    matrix:
      include:
        - target: x86_64-unknown-linux-musl; backend: ort
        - target: x86_64-unknown-linux-musl; backend: openvino
        - target: aarch64-unknown-linux-musl; backend: qnn
        - target: aarch64-unknown-linux-musl; backend: tidl
        - target: armv7-unknown-linux-musleabihf; backend: ort
- Each matrix entry compiles with the appropriate --features backend-<name>
- Archive name: kirra-${VERSION}-${TARGET_NAME}-${BACKEND}.tar.gz
- All archives attached to GitHub Release alongside SHA256SUMS
- Files: .github/workflows/release.yml
```

---

## PARK-032 `docs` `certification` `epic:certification`

**Complete RTM (`KIRRA-RTM-001`)**

Every safety requirement traced to source line, test ID, and coverage entry.
No code — document work only.

---

## PARK-033 `test` `certification` `epic:certification`

**MC/DC coverage report**

`cargo-llvm-cov` MC/DC for `posture_cache.rs`, `posture_engine_v2.rs`,
`kirra_core.rs`, `rss.rs`. CI fails if < 100%.

### Claude Code Prompt
```
Create or update .github/workflows/coverage.yml.

Requirements:
- Job: mcdc-coverage, runs-on: ubuntu-latest
- Install cargo-llvm-cov: cargo install cargo-llvm-cov
- Run: cargo llvm-cov --mcdc --html --output-dir coverage-report \
      -p kirra-runtime-sdk -p parko-core
- Check coverage for target files using llvm-cov report --json:
    posture_cache.rs, posture_engine_v2.rs, kirra_core.rs, rss.rs
  Fail if any file has MC/DC < 100%:
    if [ $(jq '.data[0].files[] | select(.filename | contains("posture_cache"))
         | .summary.regions.percent' coverage.json) != "100" ]; then exit 1; fi
  (repeat for each target file)
- Upload HTML report as artifact named "mcdc-coverage"
- Trigger: push to main, pull_request
- Files: .github/workflows/coverage.yml
```

---

## PARK-034 `docs` `certification` `epic:certification`

**FMEA (`KIRRA-FMEA-001`)**

Posture stale cache, governor bypass, attestation replay, nonce exhaustion, RSS
numerical overflow. Each failure mode: severity, detection, mitigation.

*(Documentation work only — no Claude Code Prompt needed.)*

---

## PARK-035 `docs` `certification` `epic:certification`

**DFA (`KIRRA-DFA-001`)**

Common-cause failures in HA active/passive pair on NFS-shared SQLite per ISO
26262 Part 9.

*(Documentation work only — no Claude Code Prompt needed.)*

---

## PARK-036 `aegis-integration` `feat` `epic:certification`

**Offline `kirra_audit_verify` binary**

Reads audit chain from SQLite, verifies Ed25519 signatures, prints tamper-evidence
report, exits 1 on any corruption.

### Claude Code Prompt
```
Create kirra-runtime-sdk/src/bin/kirra_audit_verify.rs.

Requirements:
- CLI: kirra_audit_verify --db <path> [--verbose]
  (parse with clap or std::env::args; clap is already a dependency)
- Open SQLite read-only at --db path using VerifierStore
- Read all rows from audit_log_chain ordered by id ASC
- For each row:
    a. Recompute SHA-256(previous_hash || entry_data); compare with stored hash
    b. If row has an Ed25519 signature, verify against trusted_federation_controllers
    c. Print: "Row NNN: OK" or "Row NNN: TAMPERED (hash mismatch)" or
              "Row NNN: TAMPERED (signature invalid)"
- Exit 0 if all rows OK; exit 1 if any row fails
- Does not require KIRRA_ADMIN_TOKEN; read-only operation
- Tests:
    a. Build a valid chain with 5 entries; run binary; assert exit 0
    b. Corrupt one byte in row 3; run binary; assert exit 1 and row 3 is named
- Files: kirra-runtime-sdk/src/bin/kirra_audit_verify.rs
```

---

## PARK-037 `docs` `certification` `epic:certification`

**SOTIF analysis (`KIRRA-SOTIF-001`)**

ISO 21448: intended function, triggering conditions, evaluation scenarios for
inference loop + RSS governor integration.

*(Documentation work only.)*

---

## PARK-038 `simulation` `test` `epic:certification`

**HIL test harness**

Scriptable harness connecting `kirra_safety_runtime` to CARLA or kinematics
integrator at 100 Hz. Zero RSS escapes on 1 000 trajectories.

### Claude Code Prompt
```
Create kirra-runtime-sdk/tests/hil/ directory with the HIL harness.

Requirements:
- tests/hil/hil_runner.rs: binary test that connects to a running
  kirra_safety_runtime via HTTP
- Sends vehicle state updates at 100 Hz:
    POST /attestation/verify with simulated sensor telemetry
    (or use the existing CARLA client pattern from kirra_carla_client.rs)
- Reads posture from GET /fleet/posture every 100ms via SSE
  /system/posture/stream
- Logs all inputs/outputs/posture to a CSV: tests/hil/output/<timestamp>.csv
  columns: tick_ms, ego_vel, lead_vel, gap, commanded_vel, output_vel, posture
- Detects failures:
    a. Any RSS violation that produces output_vel > 0
    b. Posture stays Nominal when gap < safe_distance for > 1 tick
    c. Service unreachable (connection refused -> fail immediately)
- Run 1_000 randomized trajectories from a fixed seed (reproducible)
- Exit 0 if zero escapes; exit 1 with failing trajectory details otherwise
- tests/hil/README.md: how to run against CARLA and against built-in sim
- Files: kirra-runtime-sdk/tests/hil/hil_runner.rs
         kirra-runtime-sdk/tests/hil/README.md
```

---

## PARK-039 `packaging` `chore` `epic:certification`

**Helm chart: inference backend values**

`inferenceBackend`, `tickRateHz`, `modelPath`, `rssReactionTimeS` in
`charts/kirra-verifier/values.yaml`. Chart deploys correct binary variant.

### Claude Code Prompt
```
Update charts/kirra-verifier/ to support inference backend configuration.

Requirements:
- Add to values.yaml:
    inferenceBackend: "ort"   # ort | qnn | tidl | openvino | rocm
    tickRateHz: 100
    modelPath: ""             # mount path inside container
    rssReactionTimeS: 0.5
    rssAccelMax: 3.0
    rssBrakeMin: 6.0
- In templates/deployment.yaml:
    - Set KIRRA_BACKEND env var from .Values.inferenceBackend
    - Set KIRRA_TICK_RATE_HZ from .Values.tickRateHz
    - Set RSS_REACTION_TIME_S from .Values.rssReactionTimeS
    - If .Values.modelPath != "": mount the model volume and set KIRRA_MODEL_PATH
- Verify: helm lint charts/kirra-verifier exits 0
- Verify: helm template test charts/kirra-verifier --set inferenceBackend=qnn
  produces valid YAML
- Files: charts/kirra-verifier/values.yaml
         charts/kirra-verifier/templates/deployment.yaml
```

---

## PARK-040 `docs` `certification` `epic:certification`

**Architecture overview document**

`docs/architecture.md`: Mermaid block diagram, data-flow, security boundaries,
ASIL decomposition table.

### Claude Code Prompt
```
Create docs/architecture.md for the kirra + parko ecosystem.

Requirements:
- Section 1: System Block Diagram (Mermaid flowchart)
    flowchart LR
      LLM["LLM / AI Agent"] --> AF["Action Filter\n(kirra-runtime-sdk)"]
      AF --> PE["Posture Engine\n(kirra-runtime-sdk)"]
      Sensor["Sensor Telemetry"] --> PE
      PE --> IL["InferenceLoop\n(parko-core)"]
      IL --> BS["BackendSelector\n(parko-core)"]
      BS --> ORT["ORT (CPU)"]
      BS --> QNN["QNN (Qualcomm)"]
      BS --> TIDL["TIDL (TI)"]
      BS --> OV["OpenVINO (Intel)"]
      BS --> ROCM["ROCm (AMD)"]
      IL --> Gov["KirraKernelGovernor\n(kirra-runtime-sdk)"]
      Gov --> RSS["RssGate\n(parko-core::rss)"]
      RSS --> Act["Actuator"]
- Section 2: Data Flow — describe each arrow: what data crosses it,
  who owns the buffer, what validation occurs
- Section 3: Security Boundaries — trust boundary diagram; map to
  kirra-runtime-sdk security invariants from CLAUDE.md
- Section 4: ASIL Decomposition Table
  | Component | ASIL Claim | Justification |
  (derive from KIRRA-SA-001 in docs/safety/)
- Do not invent requirements; source everything from CLAUDE.md, docs/safety/,
  and existing source files
- Files: docs/architecture.md
```
