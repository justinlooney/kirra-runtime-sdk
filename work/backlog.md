# Backlog

> 40 tasks derived from the roadmap. Every coding task includes a Claude Code
> Prompt ready to paste. Framing is corrected: hardware backends are first
> implementations; IEEE 2846 / IEC 61508 / ASTM F3269 integrations are new
> work, not refinements. The CPU ONNX backend (parko-onnx) already exists.
> kirra-runtime-sdk holds ~333 tests; parko-core has its own separate test suite.

---

## PARK-001 `control-loop` `feat` `epic:runtime-core`

**Attach `SafetyGovernor` to `ControlLoop`**

Add `with_governor(impl SafetyGovernor + 'static)` builder to `ControlLoop` in
`parko-core`. The governor is stored as `Option<Box<dyn SafetyGovernor>>`. When
present, the built-in scalar clamp is suppressed — both enforcement paths must
not run on the same tick. This is the foundation for the Kirra governor crate's
integration with the parko-core inference loop.

### Claude Code Prompt
```
You are working in the parko-core crate. The file to edit is
parko-core/src/control_loop.rs (or the equivalent entry point).

Task: Add a `with_governor` builder method to `ControlLoop`.

Requirements:
1. Add field: governor: Option<Box<dyn SafetyGovernor>> to ControlLoop struct.
2. Add method: pub fn with_governor(mut self, g: impl SafetyGovernor + 'static) -> Self
3. In tick(): if self.governor.is_some() { skip built-in clamp; delegate to governor }
              else { run built-in clamp as before }
4. SafetyGovernor trait (if not yet defined) lives in parko-core/src/governor.rs:
     pub trait SafetyGovernor: Send + Sync {
         fn enforce(&self, proposed: f64, posture: PostureState) -> f64;
     }
5. Write test test_builtin_clamp_suppressed:
   - Create a ZeroGovernor that always returns 0.0.
   - Inject via with_governor.
   - Call tick with a value above the built-in clamp threshold.
   - Assert result == 0.0 (governor wins, not clamped by built-in).
6. Write test test_no_governor_uses_builtin_clamp:
   - Create ControlLoop without governor.
   - Call tick with a value above the clamp threshold.
   - Assert result == clamped value.
7. All existing parko-core tests must continue to pass.
8. No unsafe code.
9. Run `cargo test -p parko-core` and confirm exit 0.
```

---

## PARK-002 `control-loop` `feat` `epic:runtime-core`

**Add test-only posture state setter**

Add `set_state_for_test(state: PostureState)` to `ControlLoop` in `parko-core`
behind `#[cfg(test)]`. This is a test seam — it sets internal posture state
directly without transition validation. It must be absent from release builds
(verified with `nm`) and present only when running tests.

### Claude Code Prompt
```
In parko-core/src/control_loop.rs, add a cfg(test) method to ControlLoop.

Requirements:
1. Add this block to the ControlLoop impl:
     #[cfg(test)]
     pub fn set_state_for_test(&mut self, state: PostureState) {
         self.posture_state = state;
     }
2. Do not modify any production code paths.
3. Write a test in parko-core/tests/ that:
   - Creates a ControlLoop.
   - Calls set_state_for_test(PostureState::Degraded).
   - Calls tick and asserts the output is consistent with Degraded behaviour.
4. After a release build, run:
     nm target/release/<binary> | grep set_state_for_test
   and confirm the output is empty.
5. cargo test -p parko-core must exit 0.
6. Do not add this method to any non-test module or feature-flagged path.
```

---

## PARK-003 `control-loop` `test` `epic:runtime-core`

**Posture divergence property test**

Proptest suite: for all valid `(proposed_output: f64, posture_state: PostureState)`,
the Kirra governor result must be at least as conservative as the built-in clamp.
This is the core correctness invariant for the governor integration. Uses
`set_state_for_test` from PARK-002 to drive posture states in the property loop.

### Claude Code Prompt
```
Create parko-core/tests/posture_divergence.rs.

Requirements:
1. Add proptest = "1" to parko-core dev-dependencies if not present.
2. Generate (proposed: f64, state: PostureState) filtered to exclude NaN/Inf.
3. For each pair:
   a. Run proposed through ControlLoop without governor → builtin_result.
   b. Use set_state_for_test to inject state; run through Kirra governor
      (via the Kirra governor crate's SafetyGovernor impl) → governor_result.
   c. For Nominal/Degraded: assert governor_result <= builtin_result.
   d. For LockedOut: assert governor_result == 0.0 (or EnforcementAction::Halt).
4. proptest config: cases = 10_000.
5. Three separate proptest! blocks — one per PostureState variant.
6. No new non-dev dependencies.
7. Run `cargo test -p parko-core -- --test-threads=1` to confirm all pass.
Note: Do not assume 333 tests exist in parko-core; only rely on tests you can
verify exist in the parko-core test suite.
```

---

## PARK-004 `control-loop` `safety` `epic:runtime-core`

**NaN/Inf rejection at tick boundary**

Input guard at the top of `ControlLoop::tick`: any NaN or Inf input returns
`EnforcementAction::Halt` before reaching governor or clamp. Prevents undefined
floating-point behavior from propagating through the safety stack. Must be
verified by a proptest generating adversarial floats.

### Claude Code Prompt
```
In parko-core/src/control_loop.rs, add an input guard at the top of tick().

Requirements:
1. Before any governor or clamp logic:
     if proposed_output.is_nan() || proposed_output.is_infinite() {
         return EnforcementAction::Halt;
     }
   (adjust to match actual input type — may be a slice; check all elements)
2. Add proptest generating adversarial f64 (NaN, Inf, -Inf, subnormals):
   assert all NaN/Inf inputs → Halt, no panic.
3. Add unit test: a valid f64 still reaches the governor unchanged.
4. Do not change governor or clamp logic.
5. cargo test -p parko-core exits 0.
```

---

## PARK-005 `control-loop` `feat` `epic:runtime-core`

**VirtualClock / SystemClock abstraction in ControlLoop**

Wire the `Clock` trait into `ControlLoop` so all timing logic calls
`self.clock.now_ms()` instead of wall-clock APIs. Use `VirtualClock` for tests
and `SystemClock` as the default. Eliminates any `sleep` dependency from timing
tests in parko-core.

### Claude Code Prompt
```
In parko-core/src/control_loop.rs, wire the Clock trait into ControlLoop.

Requirements:
1. Clock trait (if not already defined in parko-core/src/clock.rs):
     pub trait Clock: Send + Sync {
         fn now_ms(&self) -> u64;
     }
   pub struct SystemClock;
   pub struct VirtualClock { current_ms: Arc<AtomicU64> }
   impl VirtualClock { pub fn advance(&self, ms: u64) }
2. Add field: clock: Arc<dyn Clock> to ControlLoop.
3. Add builder: pub fn with_clock(mut self, c: Arc<dyn Clock>) -> Self.
4. Default to Arc::new(SystemClock) in ControlLoop::new().
5. Replace all direct time reads inside ControlLoop with self.clock.now_ms().
6. Add test: use VirtualClock, advance manually, assert tick fired correct
   number of times without any sleep.
7. cargo test -p parko-core exits 0.
```

---

## PARK-006 `chore` `epic:runtime-core`

**parko-core v0.1.0 release tag**

Set version to `0.1.0` in `parko-core/Cargo.toml`. Verify
`cargo publish --dry-run -p parko-core` exits cleanly. Tag `parko-core-v0.1.0`
in the repo. No code changes — version bump and tagging only.

---

## PARK-007 `control-loop` `feat` `epic:hal`

**Define `InferenceBackend` trait and `BackendDescriptor` enum**

Define the zero-copy `InferenceBackend` trait and `BackendDescriptor` enum in
`parko-core`. These form the contract that all backends (CPU, QNN, TIDL,
ROCm/Vitis, OpenVINO) implement. All scratch memory is pre-allocated at init;
no heap allocation on the hot path.

### Claude Code Prompt
```
Create parko-core/src/backend.rs.

Requirements:
1. pub trait InferenceBackend: Send + Sync {
       fn run(&self, input: &[f32], output: &mut [f32]) -> Result<(), BackendError>;
       fn descriptor(&self) -> BackendDescriptor;
   }
2. #[derive(Debug, Clone, PartialEq, Eq, Hash)]
   pub enum BackendDescriptor { Cpu, QualcommQnn, TiTidl, AmdRocm, IntelOpenVino }
3. #[derive(Debug)]
   pub enum BackendError { ShapeMismatch { expected: usize, got: usize }, Io(String), Unsupported }
4. Re-export from parko_core lib.rs:
     pub use backend::{InferenceBackend, BackendDescriptor, BackendError};
5. Unit test: round-trip each BackendDescriptor variant through format!("{:?}", v).
6. No new dependencies.
7. cargo test -p parko-core exits 0.
```

---

## PARK-008 `control-loop` `feat` `epic:hal`

**Validate parko-onnx CPU backend against InferenceBackend trait**

The parko-onnx crate contains a CPU-based ONNX Runtime backend and a MNIST-style
integration test. Wire it against the `InferenceBackend` trait from PARK-007 and
confirm the integration test is green before any multi-silicon work begins.

### Claude Code Prompt
```
In parko-onnx/src/lib.rs (or the main backend file), implement InferenceBackend.

Requirements:
1. Implement parko_core::InferenceBackend for the existing OrtBackend struct.
2. run(&self, input: &[f32], output: &mut [f32]):
   - Validate input.len() == self.input_size and output.len() == self.output_size;
     return BackendError::ShapeMismatch on mismatch.
   - Run ORT session; copy result into output slice.
   - No Vec<f32> allocation on the hot path (pre-allocate scratch at init).
3. descriptor(&self) returns BackendDescriptor::Cpu.
4. Run cargo test -p parko-onnx and confirm the MNIST integration test passes.
   Do NOT declare this task done if the MNIST test is skipped or ignored.
5. Add parko-core as a dependency in parko-onnx/Cargo.toml if not present.
```

---

## PARK-009 `control-loop` `feat` `epic:hal`

**Add MockBackend for parko-core unit tests**

Add a `MockBackend` to `parko-core` that accepts configurable output values for
deterministic testing. Eliminates the dependency on ORT in the parko-core test
binary. `MockBackend` is the preferred backend for all parko-core unit and
property tests.

### Claude Code Prompt
```
Create parko-core/src/backends/mock.rs.

Requirements:
1. pub struct MockBackend { output: Vec<f32>, descriptor: BackendDescriptor }
2. impl MockBackend {
       pub fn new(output: Vec<f32>) -> Self  // descriptor defaults to Cpu
       pub fn new_with_descriptor(output: Vec<f32>, d: BackendDescriptor) -> Self
   }
3. impl InferenceBackend for MockBackend:
   - run: copy self.output into output slice; return ShapeMismatch if lengths differ.
   - descriptor: return self.descriptor.
4. Re-export: pub use backends::mock::MockBackend in parko-core lib.rs.
5. Test: MockBackend::new(vec![1.0, 2.0]), run with 2-element output, assert values.
6. Confirm parko-core tests compile without any ORT link.
7. cargo test -p parko-core exits 0.
```

---

## PARK-010 `backend-qnn` `backend-tidl` `backend-openvino` `backend-amd` `feat` `epic:hal`

**Feature-gated stub backends (QNN, TIDL, ROCm/Vitis, OpenVINO)**

Four zero-output stub backends, each gated behind a distinct Cargo feature.
No hardware required; designed so CI can build and test all four in one run.
These are stubs only — the real implementations are PARK-020 through PARK-023.

### Claude Code Prompt
```
Create stub backends in parko-core/src/backends/:
  qnn_stub.rs, tidl_stub.rs, rocm_stub.rs, openvino_stub.rs

For each stub (example shown for QNN; repeat pattern for the others):
1. #[cfg(feature = "backend-qnn")]
   pub struct QnnStubBackend;
   impl InferenceBackend for QnnStubBackend {
       fn run(&self, _input: &[f32], output: &mut [f32]) -> Result<(), BackendError> {
           output.iter_mut().for_each(|v| *v = 0.0);
           Ok(())
       }
       fn descriptor(&self) -> BackendDescriptor { BackendDescriptor::QualcommQnn }
   }
2. Add optional features to parko-core/Cargo.toml:
     [features]
     backend-qnn = []
     backend-tidl = []
     backend-rocm = []
     backend-openvino = []
3. Add mod declarations guarded by cfg(feature) in backends/mod.rs.
4. Test for each: cargo test -p parko-core --features backend-<name>
   Run stub, assert all output elements are 0.0, assert descriptor matches.
5. No hardware, no external dependencies.
```

---

## PARK-011 `control-loop` `safety` `epic:hal`

**Backend latency watchdog in InferenceLoop**

`InferenceLoop` wraps a backend and measures time-per-call using the `Clock`
abstraction from PARK-005. When a call exceeds `deadline_ms`, it emits a
`LatencyViolation` event and holds the last safe output. Three consecutive
violations transition posture to `Degraded`.

### Claude Code Prompt
```
In parko-core/src/inference_loop.rs (create or extend), add a latency watchdog.

Requirements:
1. Add field: deadline_ms: Option<u64> to InferenceLoop.
2. Add builder: pub fn with_deadline(mut self, ms: u64) -> Self.
3. On each tick:
   let start = self.clock.now_ms();
   backend.run(input, output)?;
   let elapsed = self.clock.now_ms() - start;
   if let Some(dl) = self.deadline_ms {
       if elapsed > dl {
           self.consecutive_violations += 1;
           // hold last safe output by copying previous output
           if self.consecutive_violations >= 3 {
               self.set_posture(PostureState::Degraded);
           }
           return Ok(LatencyViolation { elapsed_ms: elapsed, deadline_ms: dl });
       }
   }
   self.consecutive_violations = 0;  // reset on success
4. Test: create InferenceLoop with TidlStubBackend latency_ms=5, deadline_ms=1,
   advance VirtualClock to simulate 5ms per call, run 3 times, assert posture == Degraded.
5. Test: 1 slow tick then 1 fast tick → posture returns to Nominal.
6. cargo test -p parko-core exits 0.
```

---

## PARK-012 `chore` `epic:hal`

**CI matrix: all four stub backends**

GitHub Actions matrix job that builds and tests all four feature-gated stub
backends on ubuntu-latest in a single workflow run. Ensures no stub breaks CI
after backend trait changes.

### Claude Code Prompt
```
Add or update .github/workflows/ci.yml with a matrix job for stub backends.

Requirements:
1. Add job: test-stub-backends
   strategy:
     matrix:
       features: [backend-qnn, backend-tidl, backend-rocm, backend-openvino]
   steps:
     - uses: actions/checkout@v4
     - uses: dtolnay/rust-toolchain@stable
     - run: cargo test -p parko-core --features ${{ matrix.features }}
2. Job runs on ubuntu-latest; no hardware required.
3. Do not remove or break any existing CI jobs.
4. All four matrix entries must be green before this task is Done.
```

---

## PARK-013 `behavioral-safety` `safety` `epic:behavioral-safety`

**Longitudinal RSS safe-distance (IEEE 2846-2022 §5.1) — first implementation**

First implementation of the IEEE 2846 longitudinal safe-distance formula in
`parko-core::rss`. No prior behavioral-safety code exists in the repository.
The formula uses ego and lead vehicle kinematics to compute the minimum safe
following distance.

### Claude Code Prompt
```
Create parko-core/src/rss.rs and implement longitudinal safe distance.

Requirements:
1. pub fn longitudinal_safe_distance(
       ego_vel: f64, lead_vel: f64,
       reaction_time: f64, accel_max: f64,
       brake_min: f64, brake_max: f64,
   ) -> f64
   Formula (IEEE 2846-2022 §5.1):
     d_response = ego_vel * reaction_time + 0.5 * accel_max * reaction_time.powi(2)
     v_after = ego_vel + accel_max * reaction_time
     d_brake_ego = v_after.powi(2) / (2.0 * brake_min)
     d_brake_lead = lead_vel.powi(2) / (2.0 * brake_max)
     d_min = d_response + d_brake_ego - d_brake_lead
     return d_min.max(0.0)
2. Unit tests:
   - equal speeds: assert d_min > 0.0
   - ego faster: assert d_min > equal-speed case
   - ego slower: assert d_min == 0.0 (lead can brake harder)
   - both zero: assert d_min == 0.0
   - very high speed: assert no overflow or NaN
3. Add pub mod rss; to parko-core/src/lib.rs.
4. cargo test -p parko-core exits 0.
```

---

## PARK-014 `behavioral-safety` `safety` `epic:behavioral-safety`

**Lateral RSS safe-distance (IEEE 2846-2022 §5.2) — first implementation**

First implementation of the IEEE 2846 lateral safe-distance formula.
Computes minimum lateral separation required given the lateral velocities
and maximum lateral acceleration of both actors.

### Claude Code Prompt
```
In parko-core/src/rss.rs, add lateral safe distance.

Requirements:
1. pub fn lateral_safe_distance(
       ego_lat_vel: f64, obj_lat_vel: f64,
       lat_accel_max: f64, reaction_time: f64,
   ) -> f64
   Formula (IEEE 2846-2022 §5.2):
   Compute reaction distances for both actors (v * t + 0.5 * a * t^2),
   compute braking distances, return max(0.0, combined margin).
2. Unit tests:
   - converging fast: large positive margin
   - diverging: margin 0 (they are moving apart)
   - both stationary: margin 0
3. cargo test -p parko-core exits 0.
```

---

## PARK-015 `behavioral-safety` `posture-engine` `safety` `epic:behavioral-safety`

**Wire RssState into kirra-runtime-sdk posture engine**

Define `RssState { safe, longitudinal_margin, lateral_margin }` and wire it into
the `kirra-runtime-sdk` posture engine. An RSS violation triggers `Degraded`
posture and uses the existing 5-tick / 10 s recovery hysteresis.

### Claude Code Prompt
```
In kirra-runtime-sdk/src/posture_engine.rs and posture_engine_v2.rs, integrate
RssState into the posture pipeline.

Requirements:
1. pub struct RssState { pub safe: bool, pub longitudinal_margin: f64,
       pub lateral_margin: f64 }
2. Add PostureRecalcTrigger::RssViolation to the trigger enum.
3. In start_posture_engine_worker: handle RssViolation → recalculate_and_broadcast.
4. In derive_fleet_posture: if any active RssViolation → return FleetPosture::Degraded.
5. Recovery: use existing AV_RECOVERY_STREAK_THRESHOLD=5, AV_RECOVERY_WINDOW_MS=10_000.
   An RssViolation resets the streak to 0.
6. Integration test using ScenarioRunner:
   - inject RssState { safe: false } → assert Degraded
   - inject 5x RssState { safe: true } within 10s → assert Nominal
7. cargo test -p kirra-runtime-sdk exits 0.
Note: kirra-runtime-sdk has ~333 tests; all must remain green.
```

---

## PARK-016 `behavioral-safety` `kirra-governor` `safety` `epic:behavioral-safety`

**RSS pre-actuator gate in Kirra governor crate**

Add an RSS pre-actuator gate to the Kirra governor crate (the crate implementing
`SafetyGovernor` with kinematic contracts and envelope selection). When
`rss_state.safe == false`, clamp velocity to 0.0 before any kinematic envelope
check. This is the first integration of behavioral safety into the governor.

### Claude Code Prompt
```
In the Kirra governor crate (verify the crate name in the workspace Cargo.toml
before editing — it may be named parko-kirra-governor or similar), add RSS gating.

Requirements:
1. Add field: rss_state: RssState (import from parko-core::rss; default safe=true)
   to the main governor struct (verify the struct name in the crate).
2. Add method: pub fn update_rss_state(&mut self, state: RssState)
3. In enforce() or the equivalent SafetyGovernor::enforce implementation:
   FIRST check: if !self.rss_state.safe { clamp velocity output to 0.0; return }
   Then continue with existing kinematic envelope checks.
4. Unit test A: set rss_state.safe=false, input vel=5.0 → assert output vel==0.0.
5. Unit test B: set rss_state.safe=true, input vel=5.0 → normal kinematics apply.
6. Do not change the governor constructor signature.
7. cargo test -p <governor-crate-name> exits 0.
```

---

## PARK-017 `behavioral-safety` `test` `epic:behavioral-safety`

**RSS property test**

Proptest: for all valid `(ego_vel, lead_vel, gap, commanded_vel)` in physically
plausible ranges, no RSS-violating command exits the governor for any posture
state. This is the primary safety property for the behavioral safety increment.

### Claude Code Prompt
```
Create parko-core/tests/rss_property.rs (or in the Kirra governor test suite).

Requirements:
1. proptest generates (ego_vel: f64, lead_vel: f64, gap: f64, commanded_vel: f64)
   filtered: all >= 0.0, gap > 0.0, vel < 150.0 m/s (physically plausible).
2. For each tuple:
   safe_dist = longitudinal_safe_distance(ego_vel, lead_vel, 0.5, 3.0, 6.0, 8.0)
   rss_safe = gap >= safe_dist
   Set RssState { safe: rss_safe, ... } on the Kirra governor.
   governor_output = governor.enforce(commanded_vel, PostureState::Nominal)
   if !rss_safe: assert governor_output == 0.0
   if rss_safe: assert governor_output <= commanded_vel (kinematics may clamp further)
3. proptest config: cases = 10_000.
4. Cover all three PostureState variants: Nominal, Degraded, LockedOut.
5. All cases must pass. No unsafe code.
```

---

## PARK-018 `behavioral-safety` `safety` `epic:behavioral-safety`

**RssViolationEvent in kirra-runtime-sdk audit chain**

`RssViolationEvent { ego_vel, lead_vel, gap, longitudinal_margin, lateral_margin,
timestamp_ms }` appended to the SHA-256 hash-chained audit ledger in
`kirra-runtime-sdk`. Tamper detection must work: a single-byte corruption of
the violation entry must cause `verify_chain()` to fail.

### Claude Code Prompt
```
In kirra-runtime-sdk/src/audit_chain.rs, add RssViolationEvent.

Requirements:
1. pub struct RssViolationEvent { pub ego_vel: f64, pub lead_vel: f64,
       pub gap: f64, pub longitudinal_margin: f64, pub lateral_margin: f64,
       pub timestamp_ms: u64 }
2. Add AuditEntry::RssViolation(RssViolationEvent) to the entry enum.
3. Serialize the event consistently with the existing chain format.
4. Add to AuditChainLinker:
     pub fn append_rss_violation(&mut self, e: RssViolationEvent)
         -> Result<(), AuditError>
   Include the event bytes in the SHA-256 chain hash.
5. Test A: append 5 entries including one RssViolation; verify_chain() returns Ok.
6. Test B: corrupt one byte of the RssViolation entry; verify_chain() returns Err.
7. cargo test -p kirra-runtime-sdk exits 0. ~333 existing tests must remain green.
```

---

## PARK-019 `behavioral-safety` `simulation` `test` `epic:behavioral-safety`

**10 000-scenario adversarial trajectory simulation**

`ScenarioRunner` + `VirtualClock` simulation: 10 000 scenarios mixing safe and
unsafe RSS gaps through the full posture engine + governor stack. Assert zero
unsafe commands exit. Must complete in < 60 s on CI.

### Claude Code Prompt
```
Create kirra-runtime-sdk/tests/rss_simulation.rs.

Requirements:
1. Use ScenarioRunner from kirra_runtime_sdk::scenario_runner.
2. Use VirtualClock; do not use sleep.
3. Generate 10_000 scenarios: each is a sequence of 10 ticks with varying
   ego_vel, lead_vel, gap, commanded_vel (some gaps below safe distance).
4. For each tick:
   - Compute RssState from parko_core::rss::longitudinal_safe_distance.
   - Feed RssState into posture engine via PostureEngineSender.
   - Feed RssState into the Kirra governor.
   - Record output velocity.
5. Assert: for every tick where gap < safe_distance, output velocity == 0.0.
6. Assert: posture correctly degrades on violation and recovers after 5 clean ticks.
7. Test must complete in < 60 s on CI.
8. cargo test -p kirra-runtime-sdk exits 0. ~333 existing tests must remain green.
```

---

## PARK-020 `backend-qnn` `feat` `epic:silicon-matrix` `needs-hardware`

**QnnBackend — first implementation**

First real implementation of the QNN backend via Qualcomm AI Engine Direct SDK
C FFI. No prior QNN backend code exists. int8 quantization from tensor metadata.
Hardware test is `#[ignore]`'d in CI; runs on QCS6490 or SA8295.

### Claude Code Prompt
```
Create parko-core/src/backends/qnn.rs (first implementation).

Requirements:
1. Use Qualcomm QNN SDK C FFI: Qnn_Interface_t, QnnBackend_Config_t,
   QnnContext_Config_t, QnnTensor_t.
2. pub struct QnnBackend { /* context, graph, tensor handles; no per-inference alloc */ }
3. QnnBackend::new(model_path: &str) -> Result<Self, BackendError>
   Loads a .serialized QNN context binary.
4. run: populate input tensor from &[f32]; if int8 model, quantize using
   scale/offset from tensor metadata; execute graph; dequantize output to &mut [f32].
5. descriptor() returns BackendDescriptor::QualcommQnn.
6. Gate entire file: #[cfg(feature = "backend-qnn")].
7. Hardware test marked #[ignore]:
     #[ignore] #[test] fn test_qnn_mobilenet_v2() { ... }
   Load a real model, compare top-1 class with ORT CPU reference.
8. Stub test (runs in CI without hardware): confirm QnnStubBackend from PARK-010
   still compiles and outputs zeros.
```

---

## PARK-021 `backend-tidl` `feat` `epic:silicon-matrix` `needs-hardware`

**TidlBackend — first implementation**

First real implementation of the TIDL backend via TI TIDL runtime C FFI,
cross-compiled to `aarch64-unknown-linux-gnu`. No prior TIDL backend code
exists. Target hardware: TDA4VM.

### Claude Code Prompt
```
Create parko-core/src/backends/tidl.rs (first implementation).

Requirements:
1. Use TI TIDL C FFI (tivxTIDLNode, TIDL_IOBufDesc_t).
2. Cross-compile target: aarch64-unknown-linux-gnu (use cross or cargo-cross).
3. TidlBackend::new(model_path: &str) -> Result<Self, BackendError>.
4. run: copy &[f32] to TIDL input buffer; execute node; copy output to &mut [f32].
5. descriptor() returns BackendDescriptor::TiTidl.
6. Gate: #[cfg(feature = "backend-tidl")].
7. Hardware test marked #[ignore]: compare output within 1e-3 of ORT CPU reference.
8. Add parko-core/build.rs for TIDL C FFI linking if needed.
```

---

## PARK-022 `backend-amd` `feat` `epic:silicon-matrix` `needs-hardware`

**RocmBackend — first implementation**

First real implementation of the ROCm/Vitis backend via ROCm HIP C FFI or
MIGraphX Rust bindings. No prior ROCm backend code exists. GPU memory is
allocated once at init, not per inference. Target hardware: RX 6000 or MI100.

### Claude Code Prompt
```
Create parko-core/src/backends/rocm.rs (first implementation).

Requirements:
1. Use migraphx crate (add as optional dep) or raw HIP C FFI.
2. RocmBackend::new(model_path: &str) -> Result<Self, BackendError>
   Parse ONNX model via migraphx::Program; compile for GPU.
3. run: copy &[f32] to device; execute; copy output to host &mut [f32].
4. GPU buffer allocated once at init; zero per-inference alloc.
5. descriptor() returns BackendDescriptor::AmdRocm.
6. Gate: #[cfg(feature = "backend-rocm")].
7. Hardware test marked #[ignore].
8. Build on ubuntu-latest without GPU: cargo build --features backend-rocm
   should compile even without a GPU present (stubs the HIP link if needed).
```

---

## PARK-023 `backend-openvino` `feat` `epic:silicon-matrix`

**OpenVinoBackend — first implementation**

First real implementation of the OpenVINO backend using `openvino-rs`. Unlike the
other three hardware backends, this can be integration-tested in CI without physical
hardware using the OpenVINO CPU plugin. Includes an identity model fixture.

### Claude Code Prompt
```
Create parko-core/src/backends/openvino.rs (first implementation).

Requirements:
1. Add optional dependency: openvino = { version = "0.7", optional = true }
   activated by features = ["backend-openvino"] in Cargo.toml.
2. pub struct OpenVinoBackend { core: openvino::Core,
       compiled: openvino::CompiledModel, input_size: usize, output_size: usize }
3. OpenVinoBackend::new(model_xml: &str, model_bin: &str) -> Result<Self, BackendError>
4. run: validate lengths; create InferRequest; set tensor; infer;
   read output tensor into &mut [f32]. Return BackendError::ShapeMismatch on mismatch.
5. Gate: #[cfg(all(feature = "backend-openvino", not(feature = "openvino-stub")))].
6. Integration test (NOT #[ignore]; runs on CI using the CPU plugin):
   Use a tiny 2-input/2-output identity model in tests/fixtures/identity.xml +
   identity.bin. Assert output == input within 1e-6.
7. Ensure the stub (PARK-010) is still usable when the real backend feature is off.
```

---

## PARK-024 `control-loop` `feat` `epic:silicon-matrix`

**BackendSelector — runtime backend selection with fallback**

`BackendSelector::new(BackendDescriptor)` creates the correct backend for the
target platform, falling back to the stub (with `tracing::warn!`) when the real
backend is unavailable or not compiled in. Enables `KIRRA_BACKEND` env-var
selection at runtime.

### Claude Code Prompt
```
Create parko-core/src/backend_selector.rs.

Requirements:
1. pub struct BackendSelector(Box<dyn InferenceBackend>);
2. BackendSelector::new(d: BackendDescriptor) -> Result<Self, BackendError>:
   QualcommQnn → try QnnBackend::new if feature enabled, else QnnStubBackend
   TiTidl → try TidlBackend::new if feature enabled, else TidlStubBackend
   AmdRocm → try RocmBackend::new if feature enabled, else RocmStubBackend
   IntelOpenVino → try OpenVinoBackend::new if feature enabled, else OpenVinoStubBackend
   Cpu → OrtBackend (always available via parko-onnx)
   On fallback: tracing::warn!("Backend {:?} unavailable, using stub", d)
3. Implement InferenceBackend for BackendSelector (delegates to inner).
4. Test: BackendSelector::new(QualcommQnn) on CI (no hardware) → Ok;
   assert descriptor() == QualcommQnn (the stub returns the correct descriptor).
5. pub use backend_selector::BackendSelector in parko-core lib.rs.
```

---

## PARK-025 `simulation` `test` `epic:silicon-matrix`

**Cross-backend determinism validation**

Fixed input through ORT CPU backend + QnnStub + TidlStub must produce outputs
within 1e-5 element-wise. Since stubs return zeros, this validates the stub
contract and establishes the tolerance baseline for when real backends arrive.

### Claude Code Prompt
```
Create parko-core/tests/cross_backend_determinism.rs.

Requirements:
1. const FIXED_INPUT: [f32; 128] = [/* 0.1 * i as f32 for i in 0..128 */];
2. Run FIXED_INPUT through: OrtBackend, QnnStubBackend, TidlStubBackend.
3. Assert all three output slices are within 1e-5 of each other element-wise.
4. Comment: "Stubs return zeros; this validates stub contract.
   Update tolerance when real backends available (see PARK-020, PARK-021)."
5. Test must run on CI without hardware.
6. cargo test -p parko-core exits 0.
```

---

## PARK-026 `packaging` `feat` `epic:packaging`

**Unified kirra_safety_runtime binary**

A single binary merging the posture engine from `kirra-runtime-sdk` and the
inference loop from `parko-core`. Configured by env vars; backend selected via
`KIRRA_BACKEND`; serves `/health` and `/inference/status`.

### Claude Code Prompt
```
Create kirra-runtime-sdk/src/bin/kirra_safety_runtime.rs.

Requirements:
1. Read env vars: KIRRA_ADMIN_TOKEN, KIRRA_DB_PATH, KIRRA_VERIFIER_ADDR
   (fail-closed if missing, same semantics as existing kirra_verifier_service),
   KIRRA_BACKEND (default "ort"), KIRRA_TICK_RATE_HZ (default 100).
2. Create BackendSelector from KIRRA_BACKEND string.
3. Start axum HTTP service reusing all routes from kirra_verifier_service.rs.
4. Start InferenceLoop with selected backend at KIRRA_TICK_RATE_HZ.
5. Wire InferenceLoop posture output into PostureEngineSender.
6. GET /health → 200 JSON {"status":"ok","backend":"<name>","tick_hz":N}.
7. GET /inference/status → {"tick_rate_hz":f64,"backend":String,"p99_latency_ms":f64}.
8. All ~333 existing kirra-runtime-sdk tests must remain green.
```

---

## PARK-027 `packaging` `chore` `epic:packaging`

**systemd unit with watchdog**

`scripts/kirra-safety-runtime.service` with `WatchdogSec=5`, `MemoryMax=512M`,
`CPUQuota=80%`. Restarts automatically on watchdog timeout or OOM kill. Verify
with `systemd-analyze verify` before this task is Done.

*(Service unit file only — no Claude Code Prompt needed.)*

---

## PARK-028 `packaging` `chore` `epic:packaging`

**Backend-aware installer**

`install.sh --backend <ort|qnn|tidl|openvino|rocm>` downloads the correct binary
for the host architecture, configures the systemd unit, and completes without
prompts when `--yes` is passed.

*(Update to existing `install.sh` — bash script work, no Claude Code Prompt needed.)*

---

## PARK-029 `packaging` `feat` `epic:packaging`

**Dashboard inference panels**

Four React panels in the existing React dashboard: inference tick rate, backend
P99 latency, RSS margin (longitudinal and lateral), posture sparkline. All panels
show "—" gracefully when the service is unreachable.

### Claude Code Prompt
```
In dashboard/src/components/ (create new files), add four panels.

Requirements:
1. InferenceTickPanel.tsx: polls GET /inference/status every 2s; renders
   tick_rate_hz and p99_latency_ms; shows "—" on error.
2. RssMarginPanel.tsx: polls GET /fleet/rss/status every 1s; renders
   longitudinal_margin and lateral_margin as progress bars (red if safe==false).
   Add GET /fleet/rss/status to kirra_safety_runtime returning
   { longitudinal_margin: f64, lateral_margin: f64, safe: bool }.
3. PostureSparklinePanel.tsx: reads last 60 posture events from SSE
   /system/posture/stream; renders 60-point sparkline
   (green=Nominal, yellow=Degraded, red=LockedOut).
4. BackendLatencyPanel.tsx: buffers last 100 p99_latency_ms samples; mini histogram.
5. Wire all four panels into the main dashboard layout.
6. All panels handle fetch errors (no crash; show "—" or "Offline").
7. Use existing dashboard styling; no new UI libraries.
```

---

## PARK-030 `packaging` `chore` `epic:packaging`

**v1.2.0 release pipeline**

CI matrix builds all backend variants for x86_64, aarch64, and armv7 musl targets.
All tarballs and a SHA256SUMS file are attached to the GitHub Release.

### Claude Code Prompt
```
Update .github/workflows/release.yml.

Requirements:
1. Add backend dimension to the build matrix:
     include:
       - target: x86_64-unknown-linux-musl; backend: ort
       - target: x86_64-unknown-linux-musl; backend: openvino
       - target: aarch64-unknown-linux-musl; backend: qnn
       - target: aarch64-unknown-linux-musl; backend: tidl
       - target: armv7-unknown-linux-musleabihf; backend: ort
2. Each entry compiles with --features backend-<backend>.
3. Archive name: kirra-${VERSION}-${TARGET}-${BACKEND}.tar.gz.
4. Attach all archives + SHA256SUMS to the GitHub Release.
5. Do not break existing cross-compilation jobs.
```

---

## PARK-031 `docs` `safety-case` `epic:certification`

**Complete RTM (KIRRA-RTM-001)**

Complete the Requirements Traceability Matrix so every ISO 26262 ASIL-D safety
requirement links to a source line, a test ID, and a coverage entry. The HARA,
Safety Goals, and Architecture documents already exist; the RTM connects them to
code and tests.

*(Document work only — no Claude Code Prompt needed.)*

---

## PARK-032 `docs` `safety-case` `epic:certification`

**IEC 61508 SIL 3 requirements mapping — first implementation**

IEC 61508 SIL 3 has been identified as a target standard but no mapping document
exists yet. This task produces the first SIL 3 requirements mapping: identify
existing safety functions that can claim SIL 3, identify gaps, and document
required mitigations or additional measures.

*(Document work only — no Claude Code Prompt needed.)*

---

## PARK-033 `docs` `safety-case` `epic:certification`

**ASTM F3269-21 bounded-operation envelope mapping — first implementation**

ASTM F3269 has been identified as a target standard but no mapping exists yet.
Define the Nominal, Degraded, and BLLOS (Beyond Line of Sight) operational
envelopes per §6, and trace each to posture engine states and governor limits
in the codebase.

*(Document work only — no Claude Code Prompt needed.)*

---

## PARK-034 `test` `safety-case` `epic:certification`

**MC/DC coverage report**

`cargo-llvm-cov` MC/DC coverage for `posture_cache.rs`, `posture_engine_v2.rs`,
`kirra_core.rs` (or the Kirra governor crate's core file), and `rss.rs`. CI
fails if any of the four files is below 100% MC/DC.

### Claude Code Prompt
```
Create or update .github/workflows/coverage.yml.

Requirements:
1. Job: mcdc-coverage, runs-on: ubuntu-latest.
2. Install: cargo install cargo-llvm-cov.
3. Run: cargo llvm-cov --mcdc --json --output-path coverage.json
         -p kirra-runtime-sdk -p parko-core
4. Parse coverage.json and fail if any of these files is below 100% MC/DC:
   posture_cache.rs, posture_engine_v2.rs, kirra_core.rs (or governor equiv), rss.rs.
5. Also generate --html report; upload as artifact "mcdc-coverage".
6. Trigger on push to main and pull_request.
```

---

## PARK-035 `docs` `safety-case` `epic:certification`

**FMEA (KIRRA-FMEA-001)**

Failure Mode and Effects Analysis covering: posture stale cache, governor bypass,
attestation replay, nonce exhaustion, RSS numerical overflow, backend latency
watchdog failure. Each failure mode requires severity, detection method, and
mitigation entry.

*(Document work only — no Claude Code Prompt needed.)*

---

## PARK-036 `docs` `safety-case` `epic:certification`

**DFA (KIRRA-DFA-001)**

Dependent Failure Analysis for common-cause failures in the HA active/passive
pair on NFS-shared SQLite per ISO 26262 Part 9. Identify all single points of
failure and propose independent protections.

*(Document work only — no Claude Code Prompt needed.)*

---

## PARK-037 `feat` `safety-case` `epic:certification`

**Offline kirra_audit_verify binary**

Reads the audit chain from SQLite, verifies Ed25519 signatures, prints a
tamper-evidence report, and exits 1 on any corruption. Read-only; does not
require `KIRRA_ADMIN_TOKEN`. Used by auditors without a running service.

### Claude Code Prompt
```
Create kirra-runtime-sdk/src/bin/kirra_audit_verify.rs.

Requirements:
1. CLI: kirra_audit_verify --db <path> [--verbose]
   (use clap, which is already a dependency, or std::env::args).
2. Open SQLite read-only at --db path using VerifierStore.
3. Read all rows from audit_log_chain ORDER BY id ASC.
4. For each row:
   a. Recompute SHA-256(previous_hash || entry_data); compare with stored hash.
   b. If an Ed25519 signature is present, verify against trusted_federation_controllers.
   c. Print: "Row NNN: OK" or "Row NNN: TAMPERED (hash mismatch)" etc.
5. Exit 0 if all rows OK; exit 1 if any row fails.
6. Does not require KIRRA_ADMIN_TOKEN.
7. Test A: valid 5-entry chain → exit 0.
8. Test B: corrupt one byte in row 3 → exit 1; row 3 named in output.
9. All ~333 existing tests must remain green.
```

---

## PARK-038 `docs` `safety-case` `epic:certification`

**SOTIF analysis (KIRRA-SOTIF-001)**

ISO 21448 SOTIF analysis covering intended function boundaries, triggering
conditions, and evaluation scenarios for the inference loop + RSS governor
integration. Identifies conditions where correct function leads to unsafe
outcomes (e.g., model confidence edge cases, latency-induced RSS gap errors).

*(Document work only — no Claude Code Prompt needed.)*

---

## PARK-039 `simulation` `test` `epic:certification`

**HIL test harness**

Scriptable harness connecting `kirra_safety_runtime` to CARLA or a kinematics
integrator at 100 Hz. Runs 1 000 randomized trajectories from a fixed seed;
logs all inputs/outputs/posture to a timestamped CSV; exits 1 if any RSS
violation produces a non-zero output velocity.

### Claude Code Prompt
```
Create kirra-runtime-sdk/tests/hil/ directory with the HIL harness.

Requirements:
1. tests/hil/hil_runner.rs: binary test connecting to a running
   kirra_safety_runtime via HTTP.
2. Sends vehicle state updates at 100 Hz via POST /attestation/verify
   (or the appropriate telemetry endpoint).
3. Reads posture from GET /fleet/posture or SSE /system/posture/stream.
4. Logs to tests/hil/output/<timestamp>.csv:
   tick_ms, ego_vel, lead_vel, gap, commanded_vel, output_vel, posture.
5. Detects failures:
   a. Any RSS violation that produces output_vel > 0.
   b. Posture stays Nominal when gap < safe_distance for > 1 tick.
   c. Service unreachable → fail immediately.
6. Run 1_000 randomized trajectories from a fixed seed.
7. Exit 0 on zero escapes; exit 1 with failing trajectory details.
8. tests/hil/README.md: how to run against CARLA and against built-in sim.
```

---

## PARK-040 `docs` `epic:certification`

**Update docs/architecture.md**

Update the architecture document to reflect the multi-silicon backend map,
the RSS governor integration, the IEC 61508 / ASTM F3269 operational envelopes,
and the ASIL decomposition. Mermaid block diagram must match current codebase.

### Claude Code Prompt
```
Update (or create) docs/architecture.md.

Requirements:
1. Section 1: System Block Diagram (Mermaid flowchart LR).
   Include: AI Agent → Action Filter → Posture Engine → InferenceLoop →
   BackendSelector → [ORT, QNN, TIDL, ROCm, OpenVINO] and
   InferenceLoop → Kirra Governor → RSS Gate → Actuator.
2. Section 2: Data Flow — describe what crosses each arrow, who owns the buffer,
   what validation occurs. Do not invent requirements; source from CLAUDE.md and
   existing source files.
3. Section 3: Security Boundaries — trust boundary diagram; reference security
   invariants from CLAUDE.md.
4. Section 4: ASIL Decomposition Table
   | Component | ASIL Claim | Justification |
   Source claims from KIRRA-SA-001 and the HARA in docs/safety/.
5. Section 5: Operational Envelopes (stub, to be completed in PARK-033):
   Nominal, Degraded, BLLOS — trace to posture states.
6. Do not claim IEC 61508 or ASTM F3269 are complete; note "mapping in progress".
```
