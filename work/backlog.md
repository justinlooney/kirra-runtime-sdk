# Backlog

> ~40 tasks derived from the roadmap. Every coding task includes a self-contained
> Claude Code Prompt. Framing is corrected: all hardware backends beyond CPU ONNX
> are new work; IEEE 2846 / IEC 61508 / ASTM F3269 integrations are new work, not
> refinements; parko-core has ~30–40 tests (not 333); the MNIST integration test
> must be verified, not assumed green; the governor crate name must be searched
> before any rename task is written.

---

## PARK-001 `control-loop` `feat`

**Attach `SafetyGovernor` to `ControlLoop`**

Add `with_governor(impl SafetyGovernor + 'static)` builder to `ControlLoop` in
`parko-core`. The governor is stored as `Option<Box<dyn SafetyGovernor>>`. When
present, the built-in scalar clamp is suppressed entirely — both enforcement paths
must not fire on the same tick. This is the foundation for KirraGovernor's
integration with the parko-core inference loop.

### Claude Code Prompt
```
You are working in the parko-core crate. Before writing any code, search the
workspace for the actual crate and struct names:

  find parko/ -name "*.toml" | xargs grep -l "\[package\]"
  grep -r "SafetyGovernor\|Governor\|AegisGovernor\|KirraGovernor" parko/ --include="*.rs" -l

If the governor struct is named AegisGovernor or similar, rename it to KirraGovernor
in the same commit. Use Kirra naming in all new comments and docs.

Task: Add `with_governor` builder to ControlLoop in parko-core/src/control_loop.rs.

Requirements:
1. Add field: governor: Option<Box<dyn SafetyGovernor>> to ControlLoop struct.
2. Add method: pub fn with_governor(mut self, g: impl SafetyGovernor + 'static) -> Self
3. In tick(): if self.governor.is_some() { delegate to governor, skip built-in clamp }
              else { run built-in clamp as before }
4. SafetyGovernor trait (if not yet defined) in parko-core/src/governor.rs:
     pub trait SafetyGovernor: Send + Sync {
         fn enforce(&self, proposed: f64, posture: PostureState) -> f64;
     }
5. Write test test_builtin_clamp_suppressed:
   - ZeroGovernor always returns 0.0.
   - Inject via with_governor.
   - Call tick with value above built-in clamp threshold.
   - Assert result == 0.0.
6. Write test test_no_governor_uses_builtin_clamp:
   - No governor injected.
   - Call tick with value above clamp threshold.
   - Assert result == clamped value.
7. Run `cargo test -p parko-core` — confirm exit 0.
   Do NOT assume any specific test count.
   Do NOT assume the MNIST integration test is passing.
8. No unsafe code.
```

---

## PARK-002 `control-loop` `feat`

**Add test-only posture state setter**

Add `set_state_for_test(state: PostureState)` to `ControlLoop` in `parko-core`
behind `#[cfg(test)]`. This is a pure test seam — it mutates internal posture state
directly without transition validation, unblocking posture-divergence property tests.
The method must be absent from release builds (verified with `nm`) and present only
when running tests.

### Claude Code Prompt
```
In parko-core/src/control_loop.rs, add a cfg(test) method to ControlLoop.

Before writing code, search for the actual governor struct name:
  grep -r "AegisGovernor\|KirraGovernor\|Governor" parko/ --include="*.rs" -l
Use Kirra naming in all new comments.

Requirements:
1. Add this block to the ControlLoop impl:
     #[cfg(test)]
     pub fn set_state_for_test(&mut self, state: PostureState) {
         self.posture_state = state;
     }
2. Do not modify any production code paths.
3. Write a test that:
   - Creates a ControlLoop.
   - Calls set_state_for_test(PostureState::Degraded).
   - Calls tick and asserts output consistent with Degraded behaviour.
4. After release build, run:
     nm target/release/<binary> | grep set_state_for_test
   Confirm output is empty. Locate binary name from workspace Cargo.toml.
5. cargo test -p parko-core exits 0.
   Do NOT assume any specific test count.
6. No unsafe code.
```

---

## PARK-003 `control-loop` `test`

**Write posture divergence property test**

Proptest suite: for all valid `(proposed_output: f64, posture_state: PostureState)`
pairs, the KirraGovernor output is at least as conservative as the built-in clamp
ceiling. This is the core correctness invariant for the governor integration. Depends
on PARK-002 for posture state injection; requires ≥ 10,000 cases per PostureState
variant.

### Claude Code Prompt
```
PARK-002 must be complete before this task.
You are working in the parko-core crate.

Before writing code, verify the actual governor crate name:
  find parko/ -name "*.toml" | xargs grep -l "\[package\]"
  grep -r "impl.*SafetyGovernor\|KirraGovernor\|AegisGovernor" parko/ --include="*.rs"

Task: Write proptest suite asserting governor output <= builtin clamp ceiling.

Requirements:
1. Add proptest = "1" to parko-core dev-dependencies if not present.
2. Create parko-core/tests/posture_divergence.rs.
3. Expose in parko-core/src/control_loop.rs:
     pub(crate) fn builtin_clamp_ceiling(proposed: f64, state: PostureState) -> f64
   Must return exact ceiling the built-in clamp applies — not a copy.
4. Three proptest! blocks (one per PostureState variant):
   - Nominal/Degraded: prop_assert!(gov_out <= ceiling)
   - LockedOut: prop_assert!(gov_out == 0.0)
5. cases = 10_000 per block.
6. Add governor crate as dev-dependency in parko-core/Cargo.toml.
   Verify crate name from workspace Cargo.toml before editing.
7. cargo test -p parko-core -- --test-threads=1 exits 0.
   Do NOT assume any specific test count.
   Do NOT assume MNIST integration test is passing.
8. No unsafe code.
```

---

## PARK-004 `control-loop` `safety`

**NaN/Inf input guard at tick boundary**

Input guard at the top of `ControlLoop::tick`: any NaN or Inf input returns
`EnforcementAction::Halt` before reaching governor or clamp. Prevents undefined
floating-point behavior from propagating through the safety stack. Must be verified
by a proptest generating adversarial floats (NaN, Inf, -Inf, subnormals).

### Claude Code Prompt
```
PREREQUISITE: PARK-001 (with_governor builder) must be complete.
Verify before proceeding:

  grep -n "governor\|with_governor" parko/parko-core/src/control_loop.rs

If the governor field and delegation are absent, stop and report:
"PARK-001 not complete."

You are working in the parko-core crate.
Use Kirra naming in all new comments and docs.

======================================================================
STEP 0: VERIFY TYPES BEFORE WRITING CODE
======================================================================

1. Check the actual return type of tick():

     grep -n "fn tick" parko/parko-core/src/control_loop.rs

2. Check whether EnforcementAction exists:

     grep -r "EnforcementAction\|enum.*Action" \
       parko/parko-core/src/ --include="*.rs"

3. Check the actual input type of tick() (f64, PlannedCommand, slice, etc.):

     grep -n "fn tick\|proposed\|input\|cmd" \
       parko/parko-core/src/control_loop.rs | head -20

Based on what you find, determine the safe return value for the guard:

- If tick() returns EnforcementAction and a Halt variant exists:
    safe return = EnforcementAction::Halt
- If tick() returns f64:
    safe return = 0.0
- If tick() returns a command struct:
    safe return = zeroed/default struct
- If tick() returns Result<...>:
    safe return = Err(...) with an appropriate error variant

In all cases, add a comment above the guard:
    // Priority 0: NaN/Inf rejection — no arithmetic on invalid inputs
    // Guard fires before governor — safe return is NOT the MRC ceiling
    // Consistent with validate_vehicle_command in kirra-runtime-sdk

If the input is a struct or slice, check ALL numeric fields for
NaN/Inf, not just the first.

======================================================================
AUTHORITY MODEL (corrected per ADL-001, commit 8be497e)
======================================================================

KirraGovernor applies profile-based enforcement:
- Degraded/LockedOut: MRC fallback profile — 5.0 m/s ceiling
- Nominal: 35.0 m/s ceiling, stricter accel rate-limit

The NaN/Inf guard fires BEFORE the governor. The safe return value
is NOT the MRC ceiling — it is the return type's safe floor from
STEP 0. The governor profile does not apply to NaN/Inf inputs.

======================================================================
TASK: Add NaN/Inf input guard at the top of tick()
======================================================================

File: parko-core/src/control_loop.rs

Requirements:
1. Add guard at the top of tick(), before any governor or clamp logic:

     // Priority 0: NaN/Inf rejection — no arithmetic on invalid inputs
     // Guard fires before governor — safe return is NOT the MRC ceiling
     if proposed_output.is_nan() || proposed_output.is_infinite() {
         return <safe return value from STEP 0>;
     }

   If input is a struct, check each numeric field individually.
   If input is a slice, check all elements in a loop.

2. Do not change governor logic.
3. Do not change clamp logic.
4. No unsafe code.

Tests:

TEST 1 — Adversarial proptest (NaN/Inf → safe return, no panic):

  use proptest::prelude::*;
  proptest! {
      #[test]
      fn test_nan_inf_inputs_guard_fires_no_panic(
          v in prop_oneof![
              Just(f64::NAN),
              Just(f64::INFINITY),
              Just(f64::NEG_INFINITY),
              proptest::num::f64::SUBNORMAL,
          ]
      ) {
          let mut loop_ = ControlLoop::new();
          let result = loop_.tick(<command from v>);
          // Guard fires before governor — safe return is NOT MRC ceiling
          prop_assert_eq!(result, <safe return value from STEP 0>);
      }
  }

  Note: subnormal f64 values are finite and pass is_nan()||is_infinite().
  Assert they do not panic — they are not expected to return the safe floor.

TEST 2 — Valid input still reaches the governor unchanged:

  struct RecordingGovernor {
      last_proposed: Arc<Mutex<Option<f64>>>,
  }
  impl SafetyGovernor for RecordingGovernor {
      fn enforce(&self, proposed: <cmd type>, posture: PostureState) -> <cmd type> {
          *self.last_proposed.lock().unwrap() = Some(<value from proposed>);
          proposed
      }
  }

  test_valid_input_reaches_governor_unchanged:
  - Inject RecordingGovernor via with_governor.
  - Call set_state_for_test(PostureState::Nominal).
  - Call tick() with valid finite input (e.g. 5.0).
  - Assert RecordingGovernor recorded exactly 5.0.

Verification:
  cargo test -p parko-core
  Confirm exit 0. Do NOT assume specific test count (currently 33).

Commit: feat(parko-core): add NaN/Inf input guard at top of tick() — Priority 0
```

---

## PARK-005 `control-loop` `feat`

**RuntimeClock / MockClock abstraction in ControlLoop**

Wire the `Clock` trait into `ControlLoop` so all timing logic calls
`self.clock.now_ms()` instead of wall-clock APIs. `MockClock` is used in tests
and `RuntimeClock` wraps wall-clock as the default. Eliminates `sleep` dependencies
from all timing tests in parko-core.

### Claude Code Prompt
```
PREREQUISITE: PARK-001 (with_governor builder) must be complete.
Verify: grep -n "with_governor" parko/parko-core/src/control_loop.rs

You are working in the parko-core crate.
Use Kirra naming in all new comments and docs.

======================================================================
STEP 0: VERIFY WHAT ALREADY EXISTS
======================================================================

1. Check whether Clock, RuntimeClock, or MockClock already exist:
     grep -r "trait Clock\|RuntimeClock\|MockClock" \
       parko/parko-core/src/ --include="*.rs"
   If they exist, import from the existing location.
   Only create parko-core/src/clock.rs if the trait does NOT exist.

2. Find all existing time calls in control_loop.rs:
     grep -n "SystemTime\|Instant::now\|now_ms\|elapsed\|duration" \
       parko/parko-core/src/control_loop.rs
   If none exist, skip replacements — add clock field for future use.

3. Check the tick interval:
     grep -n "tick\|interval\|hz\|period\|50\|100\|20" \
       parko/parko-core/src/control_loop.rs
   Needed for the concrete test assertion.

======================================================================
TASK: Wire Clock trait into ControlLoop
======================================================================

Define in parko-core/src/clock.rs ONLY if not already present:

  pub trait Clock: Send + Sync { fn now_ms(&self) -> u64; }
  pub struct RuntimeClock;
  impl Clock for RuntimeClock { /* SystemTime::now() */ }
  pub struct MockClock { current_ms: Arc<AtomicU64> }
  impl MockClock {
      pub fn new(start_ms: u64) -> Self { ... }
      // Use fetch_add so concurrent advances compose correctly
      pub fn advance(&self, ms: u64) {
          self.current_ms.fetch_add(ms, Ordering::SeqCst);
      }
  }
  impl Clock for MockClock { fn now_ms(&self) -> u64 { load(SeqCst) } }

Requirements:
1. Add field: clock: Arc<dyn Clock> to ControlLoop.
2. Add builder: pub fn with_clock(mut self, c: Arc<dyn Clock>) -> Self.
3. Default in ControlLoop::new(): clock: Arc::new(RuntimeClock).
4. Replace direct time reads with self.clock.now_ms() (from STEP 0).
5. No unsafe code.

Tests:

TEST 1 — test_mock_clock_tick_count (no sleep):
- Create MockClock starting at 0ms.
- Wire into ControlLoop via with_clock.
- If tick interval not configurable, add:
    #[cfg(test)]
    pub fn with_tick_interval_ms(mut self, ms: u64) -> Self
- Set interval to 50ms (20Hz).
- Verify tick fires at 0ms, does not fire at 40ms, fires again at 50ms.
- Advance 200ms total, assert exactly 4 ticks fired.
- Zero sleep() calls.

TEST 2 — test_runtime_clock_default_smoke:
- Create ControlLoop::new() with no with_clock() call.
- Call clock.now_ms() or tick() once.
- Assert no panic, result > 0.

Verification:
  cargo test -p parko-core
  Confirm exit 0. Do NOT assume specific test count.

Commit: feat(parko-core): wire Clock trait into ControlLoop with MockClock support
```

---

## PARK-006 `chore`

**parko-core v0.1.0 release tag**

Set version to `0.1.0` in `parko-core/Cargo.toml`. Verify `cargo publish --dry-run
-p parko-core` exits cleanly. Tag `parko-core-v0.1.0` in the repo. No code changes
— version bump and tagging only.

---

## PARK-007 `backend-architecture` `docs`

**Verify crate and struct names in parko/ workspace**

Search the parko/ workspace for all crate names, struct names, and governor
implementations before any rename or refactor. If the governor struct is still
`AegisGovernor` or a similar legacy name, record the rename target (`KirraGovernor`)
and any import paths that will be affected. Document findings in `decisions.md`
before any renaming task is started.

### Claude Code Prompt
```
You are doing a read-only audit of the parko/ workspace. Do NOT rename anything.

Run these commands and report the findings:
  find parko/ -name "Cargo.toml" -exec grep -l "name" {} \; | xargs grep "^name ="
  grep -r "struct.*Governor\|AegisGovernor\|KirraGovernor\|SafetyGovernor" \
       parko/ --include="*.rs" -n
  grep -r "pub trait SafetyGovernor\|impl SafetyGovernor" parko/ --include="*.rs" -n
  cat parko/Cargo.toml   # workspace members list

Write a summary to decisions.md under a new section "Crate and Struct Name Audit
(DATE)" with:
- Actual crate names in parko/ workspace
- Actual governor struct name(s)
- Whether SafetyGovernor trait is defined and where
- List of files that will need updating when renamed to KirraGovernor

Do NOT modify any Rust source files.
```

---

## PARK-008 `backend-architecture` `feat`

**Finalize InferenceBackend trait zero-copy boundary**

Finalize the `InferenceBackend` trait with a zero-copy hot-path signature:
`run(&self, input: &[f32], output: &mut [f32]) -> Result<(), BackendError>`. All
scratch memory must be pre-allocated at `new()`; no heap allocation on the `run`
path. Shape mismatch must return `BackendError::ShapeMismatch`, never panic.

### Claude Code Prompt
```
In parko-core/src/backend.rs (create if absent), finalize InferenceBackend.

Requirements:
1. pub trait InferenceBackend: Send + Sync {
       fn run(&self, input: &[f32], output: &mut [f32]) -> Result<(), BackendError>;
       fn descriptor(&self) -> BackendDescriptor;
   }
2. #[non_exhaustive] pub enum BackendDescriptor {
       Cpu, TensorRT, QualcommQnn, TiTidl, IntelOpenVino, AmdVitis
   }
3. pub enum BackendError {
       ShapeMismatch { expected: usize, got: usize },
       Io(String),
       Unsupported,
   }
4. Re-export from parko_core lib.rs.
5. Unit test: round-trip each BackendDescriptor variant through format!("{:?}", v).
6. No new dependencies.
7. cargo test -p parko-core exits 0. Do NOT assume specific test count.
```

---

## PARK-009 `backend-architecture` `feat`

**Validate parko-onnx CPU backend against InferenceBackend trait**

The parko-onnx crate contains a CPU-based ONNX Runtime backend and a MNIST-style
integration test. Wire it against the finalized `InferenceBackend` trait from
PARK-008 and verify the MNIST integration test is actually green by running it —
do not assume it passes without verification. The CPU baseline must be solid before
multi-silicon work begins.

### Claude Code Prompt
```
In parko-onnx/src/lib.rs, implement InferenceBackend for the existing ORT backend.

Requirements:
1. Implement parko_core::InferenceBackend for the existing OrtBackend struct.
   First, find the actual struct name: grep -r "pub struct" parko/crates/parko-onnx/src/
2. run(&self, input: &[f32], output: &mut [f32]):
   - Validate lengths; return BackendError::ShapeMismatch on mismatch.
   - Run ORT session; copy result into output slice.
   - No Vec<f32> allocation on the hot path (pre-allocate scratch at new()).
3. descriptor() returns BackendDescriptor::Cpu.
4. Run cargo test -p parko-onnx and check whether the MNIST integration test passes.
   If it fails, fix it or document the failure — do NOT assume it is passing.
5. Add parko-core as dependency in parko-onnx/Cargo.toml if not present.
```

---

## PARK-010 `backend-architecture` `feat`

**Add MockBackend for parko-core unit tests**

Add a `MockBackend` to `parko-core` that accepts configurable output values for
deterministic testing. Eliminates the ORT dependency from the parko-core test binary.
`MockBackend` is the preferred backend for all parko-core unit and property tests;
it must not require any external crate.

### Claude Code Prompt
```
Create parko-core/src/backends/mock.rs.

Requirements:
1. pub struct MockBackend { output: Vec<f32>, descriptor: BackendDescriptor }
2. impl MockBackend {
       pub fn new(output: Vec<f32>) -> Self
       pub fn new_with_descriptor(output: Vec<f32>, d: BackendDescriptor) -> Self
   }
3. impl InferenceBackend for MockBackend:
   - run: copy self.output into output slice; ShapeMismatch if lengths differ.
   - descriptor: return self.descriptor.
4. Re-export: pub use backends::mock::MockBackend in parko-core lib.rs.
5. Test: MockBackend::new(vec![1.0, 2.0]), run with 2-element output, assert values.
6. Confirm parko-core tests compile without any ORT link.
7. cargo test -p parko-core exits 0. Do NOT assume specific test count.
```

---

## PARK-011 `backend-architecture` `feat`

**Define backend capability reporting**

Add a `capabilities()` method to `InferenceBackend` and a `BackendCapabilities`
struct describing supported features (quantization, int8, fp16, max batch size).
Each backend reports its descriptor and capabilities at construction time. Enables
`BackendSelector` runtime decisions and logging in later increments.

### Claude Code Prompt
```
Extend parko-core/src/backend.rs.

Requirements:
1. pub struct BackendCapabilities {
       pub supports_int8: bool,
       pub supports_fp16: bool,
       pub max_batch_size: Option<usize>,
   }
2. Add to InferenceBackend trait:
     fn capabilities(&self) -> BackendCapabilities;
3. MockBackend::capabilities() returns all false, None.
4. OrtBackend::capabilities() returns appropriate values for CPU ONNX Runtime.
5. Unit test: capability struct for MockBackend matches expected defaults.
6. cargo test -p parko-core exits 0.
```

---

## PARK-012 `backend-architecture` `chore`

**Feature-gated stub backends for CI**

Define feature-gated zero-output stub backends for TensorRT, QNN, TIDL, OpenVINO,
and AMD in `parko-core`. Each stub is gated behind `features = ["backend-<name>"]`
and returns zeros deterministically. CI builds and tests all stubs without hardware.
These are stubs only — real implementations are PARK-020 through PARK-030.

### Claude Code Prompt
```
Create stub backends in parko-core/src/backends/:
  tensorrt_stub.rs, qnn_stub.rs, tidl_stub.rs, openvino_stub.rs, amd_stub.rs

For each stub (example for TensorRT; repeat for others):
1. #[cfg(feature = "backend-tensorrt")]
   pub struct TensorRTStubBackend;
   impl InferenceBackend for TensorRTStubBackend {
       fn run(&self, _input: &[f32], output: &mut [f32]) -> Result<(), BackendError> {
           output.iter_mut().for_each(|v| *v = 0.0);
           Ok(())
       }
       fn descriptor(&self) -> BackendDescriptor { BackendDescriptor::TensorRT }
       fn capabilities(&self) -> BackendCapabilities { BackendCapabilities::default() }
   }
2. Add optional features to parko-core/Cargo.toml:
     [features]
     backend-tensorrt = []
     backend-qnn = []
     backend-tidl = []
     backend-openvino = []
     backend-amd = []
3. Test each: cargo test -p parko-core --features backend-<name>
   Assert all output elements == 0.0; assert descriptor matches.
4. No hardware, no external dependencies.
```

---

## PARK-013 `behavioral-safety` `safety`

**Longitudinal RSS safe-distance — first implementation**

First implementation of the IEEE 2846-2022 §5.1 longitudinal safe-distance formula
in `parko-core::rss`. No prior behavioral-safety code exists in the repository. The
formula uses ego and lead vehicle kinematics (velocities, reaction time, braking
limits) to compute the minimum safe following distance.

### Claude Code Prompt
```
Create parko-core/src/rss.rs. This is a first implementation; no prior RSS code exists.

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
     d_min = (d_response + d_brake_ego - d_brake_lead).max(0.0)
2. Unit tests: equal speeds, ego faster, ego slower, both zero, high speed (no NaN).
3. Add pub mod rss; to parko-core/src/lib.rs.
4. cargo test -p parko-core exits 0.
```

---

## PARK-014 `behavioral-safety` `safety`

**Lateral RSS safe-distance — first implementation**

First implementation of the IEEE 2846-2022 §5.2 lateral safe-distance formula.
Computes minimum lateral separation required given lateral velocities and maximum
lateral acceleration of both actors. No prior behavioral-safety code exists.

### Claude Code Prompt
```
In parko-core/src/rss.rs (created in PARK-013), add lateral safe distance.

Requirements:
1. pub fn lateral_safe_distance(
       ego_lat_vel: f64, obj_lat_vel: f64,
       lat_accel_max: f64, reaction_time: f64,
   ) -> f64
   Compute reaction and braking distances for both actors; return max(0.0, margin).
2. Unit tests: converging fast (large margin), diverging (margin 0), both stationary.
3. cargo test -p parko-core exits 0.
```

---

## PARK-015 `behavioral-safety` `kirra-governor` `safety`

**Wire RssState into kirra-runtime-sdk posture engine**

Define `RssState { safe, longitudinal_margin, lateral_margin }` and wire it into
the `kirra-runtime-sdk` posture engine. An RSS violation triggers `Degraded` posture
using the existing 5-tick / 10 s recovery hysteresis. An RSS violation resets the
recovery streak to 0.

### Claude Code Prompt
```
In kirra-runtime-sdk/src/posture_engine.rs and posture_engine_v2.rs,
integrate RssState. The kirra-runtime-sdk has ~333 tests; all must remain green.

Requirements:
1. pub struct RssState { pub safe: bool, pub longitudinal_margin: f64,
       pub lateral_margin: f64 }
2. Add PostureRecalcTrigger::RssViolation to the trigger enum.
3. In start_posture_engine_worker: handle RssViolation → recalculate_and_broadcast.
4. In derive_fleet_posture: if any active RssViolation → FleetPosture::Degraded.
5. Recovery: AV_RECOVERY_STREAK_THRESHOLD=5, AV_RECOVERY_WINDOW_MS=10_000.
   An RssViolation resets the streak to 0.
6. Integration test using ScenarioRunner:
   - inject RssState { safe: false } → assert Degraded
   - inject 5x RssState { safe: true } within 10s → assert Nominal
7. cargo test -p kirra-runtime-sdk exits 0. ~333 existing tests must remain green.
```

---

## PARK-016 `behavioral-safety` `kirra-governor` `safety`

**RSS pre-actuator gate in KirraGovernor**

Add an RSS pre-actuator gate to the KirraGovernor crate. When `rss_state.safe ==
false`, clamp velocity to 0.0 before any kinematic envelope check. KirraGovernor
already hard-vetoes on Degraded/LockedOut per the authority model; this adds RSS
as an additional input to Nominal-mode decisions. Verify the actual governor crate
name before editing.

### Claude Code Prompt
```
PREREQUISITES:
- PARK-013 (longitudinal_safe_distance) must be complete.
- PARK-007 (crate name audit) must be complete.

Verify:
  grep -n "longitudinal_safe_distance" parko/parko-core/src/rss.rs
  find parko/ -name "Cargo.toml" | xargs grep "^name ="
  grep -r "impl.*SafetyGovernor\|pub struct.*Governor" parko/ --include="*.rs" -n

======================================================================
AUTHORITY MODEL (corrected per ADL-001, commit 8be497e)
======================================================================

RSS-unsafe behavior: apply MRC fallback profile (5.0 m/s cap), NOT 0.0.
LockedOut/Degraded → MRC profile. RSS violation → same semantics.

======================================================================
TASK: Add RSS pre-actuator gate to KirraGovernor
======================================================================

Requirements:
1. Add field to governor struct:
     rss_state: RssState  // import from parko_core::rss
   Default: RssState { safe: true, longitudinal_margin: f64::MAX, lateral_margin: f64::MAX }

2. Add: pub fn update_rss_state(&mut self, state: RssState)

3. In enforce() BEFORE kinematic envelope checks:
     if !self.rss_state.safe {
         // RSS unsafe: MRC fallback (5.0 m/s cap) — NOT hard zero
         // Per ADL-001: RSS violation → Degraded semantics
         return self.apply_mrc_profile(proposed);
     }
   Verify apply_mrc_profile or equivalent method name in existing code.

4. Do not change governor constructor signature.
5. No unsafe code.

Tests:
A: rss_state.safe=false, vel=10.0 → assert output <= 5.0 AND output > 0.0
B: rss_state.safe=true, vel=3.0 → normal kinematics (not capped at 5.0)
C: rss_state.safe=false, vel=2.0 → assert output == 2.0 (MRC is a cap, not fixed)

Verification:
  cargo test -p <governor-crate-name>
  Confirm exit 0. Do NOT assume specific test count.

Commit: feat(governor): add RSS pre-actuator gate — MRC fallback on unsafe state
```

---

## PARK-017 `behavioral-safety` `test`

**RSS property test**

Proptest: for all valid `(ego_vel, lead_vel, gap, commanded_vel)` in physically
plausible ranges (all ≥ 0, gap > 0, vel < 150 m/s), no RSS-violating command exits
the governor for any posture state. 10,000 cases covering Nominal, Degraded, and
LockedOut.

### Claude Code Prompt
```
PREREQUISITES: PARK-013 and PARK-016 must be complete.
Verify:
  grep -n "longitudinal_safe_distance" parko/parko-core/src/rss.rs
  grep -r "update_rss_state" parko/ --include="*.rs" -n

======================================================================
AUTHORITY MODEL (corrected per ADL-001, commit 8be497e)
======================================================================

RSS unsafe → MRC fallback (5.0 m/s cap), NOT hard zero.
Assertions: if !rss_safe → assert out <= 5.0, NOT out == 0.0.

======================================================================
TASK: RSS property test suite — 3 posture variants × 10,000 cases
======================================================================

File: parko-core/tests/rss_property.rs

Three proptest! blocks (one per PostureState variant).
Input strategy for all blocks:
  ego_vel in 0.0f64..150.0,
  lead_vel in 0.0f64..150.0,
  gap in 0.001f64..500.0,
  commanded_vel in 0.0f64..150.0
Do NOT use arbitrary f64.

For each block:
  safe_dist = longitudinal_safe_distance(ego_vel, lead_vel, 0.5, 3.0, 6.0, 8.0)
  rss_safe = gap >= safe_dist

BLOCK 1 — Nominal:
  if !rss_safe: prop_assert!(out <= 5.0)  // MRC cap — NOT == 0.0
  if rss_safe:  prop_assert!(out <= commanded_vel)

BLOCK 2 — Degraded:
  Posture is already Degraded → MRC applies regardless of RSS:
  prop_assert!(out <= 5.0)

BLOCK 3 — LockedOut:
  LockedOut maps to MRC profile (same as Degraded):
  prop_assert!(out <= 5.0)

cases = 10_000 per block. No unsafe code.

Verification:
  cargo test -p parko-core -- rss_property
  Confirm exit 0.

Commit: test(parko-core): RSS property tests — 3 posture variants × 10,000 cases
```

---

## PARK-018 `behavioral-safety` `safety`

**RssViolationEvent in kirra-runtime-sdk audit chain**

`RssViolationEvent { ego_vel, lead_vel, gap, longitudinal_margin, lateral_margin,
timestamp_ms }` appended to the SHA-256 hash-chained audit ledger. A single-byte
corruption of any entry must cause `verify_chain()` to fail. All ~333 existing
kirra-runtime-sdk tests must remain green.

### Claude Code Prompt
```
In kirra-runtime-sdk/src/audit_chain.rs, add RssViolationEvent.

Requirements:
1. pub struct RssViolationEvent { pub ego_vel: f64, pub lead_vel: f64,
       pub gap: f64, pub longitudinal_margin: f64, pub lateral_margin: f64,
       pub timestamp_ms: u64 }
2. Add AuditEntry::RssViolation(RssViolationEvent) variant.
3. pub fn append_rss_violation(&mut self, e: RssViolationEvent) -> Result<(), AuditError>
   Include event bytes in the SHA-256 chain hash.
4. Test A: 5-entry chain including one RssViolation; verify_chain() returns Ok.
5. Test B: corrupt one byte of the RssViolation entry; verify_chain() returns Err.
6. cargo test -p kirra-runtime-sdk exits 0. ~333 existing tests must remain green.
```

---

## PARK-019 `behavioral-safety` `simulation` `test`

**10,000-scenario adversarial trajectory simulation**

`ScenarioRunner` + `MockClock` simulation: 10,000 scenarios mixing safe and unsafe
RSS gaps through the full posture engine + governor stack. Assert zero unsafe commands
exit. Must complete in < 60 s on CI. All ~333 kirra-runtime-sdk tests must remain
green.

### Claude Code Prompt
```
PREREQUISITES: PARK-013, PARK-015, PARK-016, PARK-007 must be complete.
Verify:
  grep -n "longitudinal_safe_distance" parko/parko-core/src/rss.rs
  grep -n "RssViolation\|RssState" kirra-runtime-sdk/src/posture_engine*.rs
  grep -n "update_rss_state" parko/ -r --include="*.rs"
  grep -r "MockClock\|VirtualClock\|struct.*Clock" \
    kirra-runtime-sdk/src/ --include="*.rs" -n

Record actual clock type name and governor struct name from above.

======================================================================
AUTHORITY MODEL (corrected per ADL-001, commit 8be497e)
======================================================================

RSS violation → MRC fallback (5.0 m/s cap), NOT hard zero.
Assert: gap < safe_distance → output <= 5.0 (NOT == 0.0).

======================================================================
TASK: 10,000-scenario adversarial simulation
======================================================================

File: kirra-runtime-sdk/tests/rss_simulation.rs

Requirements:
1. Use ScenarioRunner from kirra_runtime_sdk::scenario_runner.
2. Use actual clock type from prerequisite check. No sleep().
3. 10,000 scenarios × 10 ticks. Deterministic seed. Mix of
   gaps below and above safe distance.
4. Per tick:
   a. Compute RssState from parko_core::rss::longitudinal_safe_distance.
   b. Feed into posture engine via PostureRecalcTrigger::RssViolation.
   c. Feed into governor via update_rss_state (actual struct name).
   d. Record output velocity.
5. Assert per violation tick:
     assert!(output_velocity <= 5.0)  // MRC cap — NOT == 0.0
6. Assert posture lifecycle:
   - After RSS violation: posture == Degraded.
   - After 5 consecutive safe ticks within 10s: posture == Nominal.
7. Must complete < 60s on CI.
8. No unsafe code.

Verification:
  cargo test -p kirra-runtime-sdk
  ~333 existing tests must remain green.

Commit: test(kirra-runtime-sdk): 10,000-scenario RSS adversarial simulation
```

---

## PARK-020 `backend-tensorrt` `feat`

**TensorRT API spike (TIME-SENSITIVE — Jetson arriving)**

Set up TensorRT FFI bindings (`tensorrt` crate or `trt-sys`) and verify a trivial
model loads and runs on the Jetson hardware. Document the build toolchain and any
driver/SDK version requirements in `decisions.md`. Gate everything behind
`features = ["backend-tensorrt"]`.

### Claude Code Prompt
```
Create parko-core/src/backends/tensorrt_spike.rs. Gate: #[cfg(feature = "backend-tensorrt")].

Requirements:
1. Find the best available TensorRT Rust binding (check crates.io for tensorrt,
   trt-sys, or similar). Document choice in decisions.md.
2. Implement a minimal struct TensorRTBackend with new(engine_path: &str).
   Load a .trt serialized engine file.
3. Add a hardware test (mark #[ignore] for CI):
     #[ignore] #[test]
     fn test_tensorrt_trivial_model() { /* load model, run inference, check no segfault */ }
4. The stub (PARK-012 TensorRTStubBackend) must still compile when this file is active.
5. Add feature = ["backend-tensorrt"] to parko-core/Cargo.toml.
6. Document: which TRT SDK version was tested, Jetson toolchain commands used.
```

---

## PARK-021 `backend-tensorrt` `feat`

**Implement TensorRTBackend struct**

Full `TensorRTBackend` implementation: `new(engine_path)` deserializes a `.trt`
plan and pre-allocates CUDA input/output buffers at init. `run()` performs H→D copy,
execute, D→H copy with no per-inference allocation. Implements `InferenceBackend`.

### Claude Code Prompt
```
In parko-core/src/backends/tensorrt.rs (extend from PARK-020 spike).

Requirements:
1. pub struct TensorRTBackend {
       engine: /* TRT engine handle */,
       input_buf: /* CUDA device buffer, pre-allocated */,
       output_buf: /* CUDA device buffer, pre-allocated */,
       input_size: usize,
       output_size: usize,
   }
2. TensorRTBackend::new(engine_path: &str, input_size: usize, output_size: usize)
   Loads .trt plan; allocates CUDA buffers. No per-inference alloc after this.
3. impl InferenceBackend:
   - run: H2D copy input; execute; D2H copy output. Return ShapeMismatch on bad lengths.
   - descriptor: BackendDescriptor::TensorRT
4. Hardware test #[ignore]: load real model, run inference, output is not all zeros.
5. Stub test (runs in CI): TensorRTStubBackend from PARK-012 still outputs zeros.
6. Gate: #[cfg(feature = "backend-tensorrt")].
```

---

## PARK-022 `backend-tensorrt` `feat`

**Integrate TensorRT into BackendSelector**

`BackendSelector::new(BackendDescriptor::TensorRT)` creates a `TensorRTBackend`
when the feature is enabled; falls back to `TensorRTStubBackend` with
`tracing::warn!` otherwise. Enables `KIRRA_BACKEND=tensorrt` env-var runtime
selection in the Kirra safety runtime binary.

### Claude Code Prompt
```
Create parko-core/src/backend_selector.rs.

Requirements:
1. pub struct BackendSelector(Box<dyn InferenceBackend>);
2. BackendSelector::new(d: BackendDescriptor, model_path: Option<&str>)
   -> Result<Self, BackendError>:
   TensorRT → TensorRTBackend if feature enabled + model_path provided,
              else TensorRTStubBackend with tracing::warn!
   Cpu → OrtBackend (always available)
   Others → respective stubs with tracing::warn!
3. impl InferenceBackend for BackendSelector (delegates to inner).
4. Test: BackendSelector::new(TensorRT, None) on CI (no GPU) → Ok;
   descriptor() == TensorRT (stub returns correct descriptor).
5. pub use backend_selector::BackendSelector in lib.rs.
```

---

## PARK-023 `backend-tensorrt` `test`

**CPU vs TensorRT output comparison**

Same fixed input through the CPU ONNX backend and the TensorRT backend; outputs must
be within 1e-3 element-wise. Hardware test `#[ignore]`'d in CI; comment documents
that the test requires Jetson. Validates the TensorRT implementation against the CPU
baseline.

### Claude Code Prompt
```
Create parko-core/tests/tensorrt_cpu_comparison.rs.

Requirements:
1. const FIXED_INPUT: [f32; N] = /* deterministic values */;
2. Load same model on OrtBackend and TensorRTBackend.
3. Run FIXED_INPUT through both; assert element-wise diff < 1e-3.
4. Mark the whole test #[ignore]:
   #[ignore] // requires Jetson with TensorRT runtime
   #[test]
   fn test_cpu_vs_tensorrt() { ... }
5. Comment: "Update tolerance if model quantization changes (see PARK-021)."
```

---

## PARK-024 `qnx` `feat`

**QNX deployment spike (TIME-SENSITIVE — 30-day license)**

Bring up the `kirra_verifier_service` binary on QNX. Identify and document any
POSIX subset gaps (signal handling, threading model, filesystem paths, dynamic
linking). Target: service starts and `/health` returns 200 on QNX. Record all
findings in `decisions.md` before the license expires.

### Claude Code Prompt
```
Target: cross-compile kirra-runtime-sdk for QNX and bring up kirra_verifier_service.

Requirements:
1. Identify the correct Rust target triple for QNX (e.g., x86_64-pc-nto-qnx710).
2. Add cross-compilation configuration to .cargo/config.toml.
3. Identify and fix any POSIX subset issues (signal, threads, sockets, filesystem).
   Document each issue and fix in decisions.md.
4. Build kirra_verifier_service for QNX:
     cargo build --target x86_64-pc-nto-qnx710 --bin kirra_verifier_service
5. Run on QNX device/VM; confirm /health returns 200 with KIRRA_ADMIN_TOKEN set.
6. Document in decisions.md: QNX version, SDK version, any feature flags disabled.
Note: Time-sensitive — 30-day license window. Prioritize getting a binary running
over feature completeness.
```

---

## PARK-025 `qnx` `backend-qnn` `docs`

**QNN + QNX compatibility analysis**

Document the Qualcomm AI Engine Direct SDK version requirements on QNX, FFI linking
differences from Linux, and memory model constraints relevant to the no-alloc backend
contract. Record findings in `decisions.md`. This analysis gates the QNN backend
implementation (PARK-027) and must be completed before QNN work starts.

### Claude Code Prompt
```
Research and document QNN + QNX compatibility. Write findings to decisions.md.

Tasks:
1. Review Qualcomm AI Engine Direct SDK release notes for QNX support.
   Document supported QNX versions and SDK version requirements.
2. Identify FFI linking differences: shared library names, rpath differences,
   init/teardown order between QNX and Linux.
3. Document memory model constraints:
   - Can QNN SDK allocate device memory on QNX without dynamic alloc in run()?
   - What is the POSIX memory API subset available on QNX?
4. Identify any features of the InferenceBackend zero-copy contract (ADL-003)
   that conflict with QNX + QNN requirements.
5. Write a "QNN + QNX Compatibility" section to decisions.md with:
   - Go/no-go recommendation for PARK-027 on QNX
   - Required SDK versions
   - List of known constraints
```

---

## PARK-026 `qnx` `backend-architecture` `docs`

**Define QNX-safe backend selection rules**

Document and enforce QNX-safe backend selection: no dynamic allocation in the
backend hot-path, restricted POSIX API surface, and single-process model constraints.
Add QNX as a recognized target in `BackendSelector` with appropriate restrictions.
Blocked until PARK-024 confirms which POSIX features are available.

### Claude Code Prompt
```
After PARK-024 is complete, add QNX-safe rules to BackendSelector.

Requirements:
1. Add compile-time gate:
     #[cfg(target_os = "nto")]  // QNX Neutrino
   to any code path that uses features unavailable on QNX (as found in PARK-024).
2. In BackendSelector::new on QNX targets:
   - If TensorRT requested: warn + fall back to CPU (TensorRT not available on QNX).
   - If QNN requested and feature enabled: proceed (per PARK-025 analysis).
   - Document the QNX-specific fallback table in decisions.md.
3. Add a doc comment to BackendSelector explaining QNX constraints.
4. Verify: cargo build --target x86_64-pc-nto-qnx710 -p parko-core succeeds.
```

---

## PARK-027 `backend-qnn` `feat`

**QNN backend MVP — first implementation**

First real implementation of the QNN backend via Qualcomm AI Engine Direct SDK C
FFI. No prior QNN backend code exists in this repository. Depends on PARK-025
(compatibility analysis). Hardware test `#[ignore]`'d in CI; stub from PARK-012
used for CI validation.

### Claude Code Prompt
```
Create parko-core/src/backends/qnn.rs. This is a first implementation.
Gate: #[cfg(feature = "backend-qnn")].

IMPORTANT: First complete PARK-025 (QNN + QNX compatibility analysis).
Verify SDK version requirements before writing any FFI bindings.

Requirements:
1. Use Qualcomm QNN SDK C FFI: Qnn_Interface_t, QnnBackend_Config_t, QnnTensor_t.
2. pub struct QnnBackend { /* context, graph, tensor handles; no per-run alloc */ }
3. QnnBackend::new(model_path: &str) -> Result<Self, BackendError>
4. run: populate input tensor; if int8 model, quantize using scale/offset from metadata;
   execute; dequantize output to &mut [f32].
5. descriptor() returns BackendDescriptor::QualcommQnn.
6. Hardware test #[ignore]: compare top-1 class with CPU reference within tolerance.
7. CI test: QnnStubBackend from PARK-012 still compiles and outputs zeros.
```

---

## PARK-028 `backend-tidl` `feat`

**TIDL backend MVP — first implementation**

First real implementation of the TIDL backend via TI TIDL runtime C FFI,
cross-compiled to `aarch64-unknown-linux-gnu`. No prior TIDL backend code exists.
Target hardware: TDA4VM. Hardware test `#[ignore]`'d; CI uses the stub from PARK-012.

### Claude Code Prompt
```
Create parko-core/src/backends/tidl.rs. This is a first implementation.
Gate: #[cfg(feature = "backend-tidl")].

Requirements:
1. Use TI TIDL C FFI (tivxTIDLNode, TIDL_IOBufDesc_t).
2. Cross-compile target: aarch64-unknown-linux-gnu.
3. TidlBackend::new(model_path: &str) -> Result<Self, BackendError>.
4. run: copy &[f32] to TIDL input buffer; execute; copy to &mut [f32].
5. descriptor() returns BackendDescriptor::TiTidl.
6. Hardware test #[ignore]: compare output within 1e-3 of CPU reference.
7. Add parko-core/build.rs for TIDL C FFI linking if needed.
```

---

## PARK-029 `backend-openvino` `feat`

**OpenVINO backend MVP — first implementation**

First real implementation of the OpenVINO backend using `openvino-rs`. Unlike other
hardware backends, testable in CI using the OpenVINO CPU plugin. Integration test
uses an identity model fixture; output must match input within 1e-6. First
implementation; no prior OpenVINO backend code exists.

### Claude Code Prompt
```
Create parko-core/src/backends/openvino.rs. This is a first implementation.
Gate: #[cfg(feature = "backend-openvino")].

Requirements:
1. Add openvino = { version = "0.7", optional = true } activated by feature.
2. pub struct OpenVinoBackend { core, compiled, input_size, output_size }
3. OpenVinoBackend::new(model_xml: &str, model_bin: &str) -> Result<Self, BackendError>
4. run: validate lengths; set tensor; infer; copy to output slice.
5. Integration test (NOT #[ignore]; runs on CI via CPU plugin):
   Use tiny identity model in tests/fixtures/identity.xml + identity.bin.
   Assert output == input within 1e-6.
6. Stub (PARK-012) still usable when feature is off.
```

---

## PARK-030 `backend-amd` `feat` `docs`

**AMD backend MVP — decide Vitis AI vs ROCm, then implement**

Decide between AMD Vitis AI (Xilinx FPGA path) and AMD ROCm (GPU path) based on
available hardware and customer requirements. Record the decision in `decisions.md`.
Implement the chosen path as an MVP; hardware test `#[ignore]`'d in CI. First
implementation; no prior AMD backend code exists.

### Claude Code Prompt
```
Before writing code, record the AMD backend decision in decisions.md:
- Vitis AI: requires Xilinx FPGA, uses xrt crate or Vitis AI C API.
- ROCm: requires AMD GPU, uses migraphx crate or HIP C FFI.
- State which was chosen and why (hardware availability, customer pull).

Then implement the chosen backend:
Gate: #[cfg(feature = "backend-amd")].

Requirements:
1. pub struct AmdBackend { /* pre-allocated buffers */ }
2. AmdBackend::new(model_path: &str) -> Result<Self, BackendError>.
3. run: no per-inference alloc; copy to output slice.
4. descriptor() returns BackendDescriptor::AmdVitis.
5. Hardware test #[ignore].
6. Stub (PARK-012) still compiles when feature is off.
```

---

## PARK-031 `packaging` `chore`

**Normalize Kirra naming across Docker/Helm**

Remove remaining Aegis references from Docker image names, Helm chart values,
environment variable names, service unit files, and install scripts. All deployment
artifacts must use Kirra naming consistently. A `grep -r aegis` scan (case-insensitive)
should return only intentional or historical references after this task.

### Claude Code Prompt
```
Search for all remaining Aegis references in deployment artifacts:
  grep -ri "aegis" docker-compose.yml Dockerfile helm/ charts/ scripts/ install.sh

For each reference, either:
- Rename to Kirra equivalent (e.g. aegis-verifier → kirra-verifier)
- Or add a comment explaining why the legacy name is intentionally preserved

After renaming:
- Verify docker-compose.yml builds: docker compose build
- Verify helm chart lints: helm lint helm/kirra/
- Verify install.sh --help runs without error
- Run: grep -ri "aegis" docker-compose.yml Dockerfile helm/ charts/ scripts/ install.sh
  and confirm only intentional references remain
```

---

## PARK-032 `packaging` `feat`

**Add Parko runtime into Kirra Docker image**

Extend the Kirra Docker image to include parko-core, the InferenceLoop, and
`BackendSelector`. One image contains parko runtime + kirra-runtime-sdk +
KirraGovernor + dashboard. Configured by `KIRRA_BACKEND` env var; both `/health`
and the inference loop must respond in the combined image.

### Claude Code Prompt
```
Update the Kirra Dockerfile to include parko-core.

Requirements:
1. Add parko workspace to the Dockerfile build stage:
   COPY parko/ parko/
   RUN cargo build --release -p parko-core -p parko-onnx (+ any governor crate)
2. The combined image must start kirra_safety_runtime (or equivalent combined binary)
   and expose /health + /inference/status.
3. KIRRA_BACKEND env var selects backend (default: cpu).
4. Test: docker build -t kirra:test . && docker run --rm -e KIRRA_ADMIN_TOKEN=test
         kirra:test /health → 200
5. Update docker-compose.yml to use the combined image.
6. Image must be < 2 GB uncompressed (document any deviation).
```

---

## PARK-033 `packaging` `chore`

**Backend-aware installer**

Update `install.sh` to accept `--backend <cpu|tensorrt|qnn|tidl|openvino|amd>`.
Downloads the correct binary variant for the host architecture, configures the
systemd unit with the right `KIRRA_BACKEND` value, and completes without prompts
when `--yes` is passed.

---

## PARK-034 `packaging` `chore`

**systemd unit with watchdog**

Create `scripts/kirra-safety-runtime.service` with `WatchdogSec=5`,
`MemoryMax=512M`, `CPUQuota=80%`. The unit must restart automatically on watchdog
timeout or OOM kill. Verify with `systemd-analyze verify` before marking Done.

---

## PARK-035 `packaging` `qnx` `chore`

**QNX packaging stub**

Define the `kirra-qnx.tar.gz` artifact structure and a placeholder Makefile for QNX
deployment. This task is blocked until PARK-024 (QNX deployment spike) produces a
working binary. Create the stub so the release pipeline has a slot for the QNX
artifact when QNX work lands.

---

## PARK-036 `ros2` `robot` `chore`

**Bring up ROS2 Jazzy on Ubuntu 24.04**

Install and configure ROS2 Jazzy on Ubuntu 24.04 for the reference robot workspace.
Verify basic pub/sub with `ros2 topic echo`. Create the colcon workspace with the
`kirra_safety` package. BLOCKED: requires Hiwonder hardware delivery or an
alternative simulation environment.

---

## PARK-037 `ros2` `robot` `kirra-governor` `feat`

**Integrate Parko + KirraGovernor with ROS2 cmd_vel topics**

Wire the Parko control loop and KirraGovernor into ROS2 cmd_vel topics:
`cmd_vel` → governor → `filtered_cmd_vel`. The governor's hard-veto on
Degraded/LockedOut must be observable on the filtered topic. Depends on PARK-036.
KirraGovernor authority model: hard-veto on Degraded/LockedOut, clamp on Nominal,
conservative fallback if unreachable.

### Claude Code Prompt
```
PREREQUISITES: PARK-001, PARK-002, PARK-007 must be complete.
Hiwonder robot must be available. ROS2 Jazzy must be installed.
Verify:
  grep -n "with_governor" parko/parko-core/src/control_loop.rs
  grep -r "pub struct.*Governor\|impl SafetyGovernor" parko/ --include="*.rs" -n
  ros2 --version
  find ros2_ws/ -name "cmd_vel_interceptor.py"

Use actual file path found. Use actual governor struct name found.

======================================================================
AUTHORITY MODEL (corrected per ADL-001, commit 8be497e)
======================================================================

Degraded/LockedOut → MRC fallback (5.0 m/s cap), NOT hard zero.
/filtered_cmd_vel is never forced to exactly 0.0 by posture alone.

======================================================================
TASK: Wire KirraGovernor into ROS2 cmd_vel pipeline
======================================================================

File: ros2_ws/src/kirra_safety/kirra_safety/cmd_vel_interceptor.py
(use actual path from prerequisite check)

Requirements:
1. Subscribe to /cmd_vel (geometry_msgs/Twist).
2. Per message:
   a. Query FleetPosture from kirra-runtime-sdk (KIRRA_VERIFIER_ADDR).
   b. Map to PostureState: Nominal/Degraded/LockedOut.
   c. Call KirraGovernor.enforce(commanded_vel, posture).
   d. Publish to /filtered_cmd_vel.
3. Degraded or LockedOut: apply MRC cap (5.0 m/s) — NOT zero.
   Comment: "MRC cap per ADL-001 — not a hard zero veto"
4. Nominal: apply kinematic clamp from governor.
5. Unreachable: drop to Degraded locally, apply 5.0 m/s cap,
   log "governor_unreachable — applying local MRC fallback".
6. Kirra naming throughout. No new Aegis references.

Tests:
A: Degraded posture, cmd=10.0 → assert filtered <= 5.0 AND > 0.0
B: Nominal posture, cmd=2.0 → assert filtered == 2.0 (within tolerance)
C: LockedOut posture, cmd=10.0 → assert filtered <= 5.0 AND > 0.0

Verification:
  colcon build --packages-select kirra_safety
  ros2 launch kirra_safety kirra_safety.launch.py
  python3 -m pytest ros2_ws/src/kirra_safety/tests/ -v

Commit: feat(ros2): wire KirraGovernor into cmd_vel — MRC profile on Degraded/LockedOut
```

---

## PARK-038 `ros2` `robot` `simulation` `feat`

**Build full reference robot stack**

Full integration: Parko + KirraGovernor + ROS2 Jazzy + kirra_safety interlock +
CARLA simulation as hardware alternative. Depends on PARK-037 and Hiwonder hardware
availability. BLOCKED until PARK-037 is complete and physical hardware (Hiwonder
robot) is available or CARLA simulation is substituted.

---

## PARK-039 `safety-case` `docs`

**Map IEC 61508 SIL 3 requirements — first implementation**

IEC 61508 SIL 3 has been identified as a target standard but no mapping document
exists. Identify existing Kirra safety functions that can claim SIL 3 compliance;
identify gaps; document required mitigations or additional measures. Every SIL 3
safety function claim must have an implementation entry or explicit gap note.

---

## PARK-040 `safety-case` `docs`

**Map ASTM F3269-21 bounded-operation envelope — first implementation**

ASTM F3269 has been identified as a target standard but no mapping exists. Define
the Nominal, Degraded, and BLLOS (Beyond Line of Sight) operational envelopes per
§6; trace each to the posture engine states and KirraGovernor limits in the codebase.
Do not claim any ASTM F3269 compliance until this mapping is complete and reviewed.
