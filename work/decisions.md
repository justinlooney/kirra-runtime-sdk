# Architecture Decision Log (ADL)

> Entries are immutable once written. Superseded decisions get a new entry
> referencing the old one by ADL number. Date format: YYYY-MM-DD.

---

## ADL-001 — Governor injection model: `Option<Box<dyn SafetyGovernor>>`

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

`ControlLoop` in `parko-core` stores a governor as
`Option<Box<dyn SafetyGovernor>>`. The built-in scalar clamp is suppressed when
a governor is present. Injection is via a builder method
`with_governor(impl SafetyGovernor + 'static)`.

### Why

Safety policies are domain-specific: kinematics envelopes for a ground vehicle
differ from those for an aerial platform. A trait object lets each deployment
inject the Kirra governor crate's implementation without forking `parko-core`.
The `Option` preserves backward compatibility — loops without an injected
governor retain the existing built-in clamp behaviour.

### Alternatives Considered

1. **Compile-time generics** (`ControlLoop<G: SafetyGovernor>`): Avoids heap
   allocation but forces every caller to specify the type parameter and makes
   default (no-governor) construction awkward. The alloc cost on a non-hot init
   path is not worth the API complexity. Rejected.
2. **Function pointer** (`fn(f64, PostureState) -> f64`): Simpler, but stateless.
   Governors that require calibration data (e.g., envelope limits loaded from a
   config file) must close over state. Rejected.
3. **Global registry**: Couples `parko-core` to a global; breaks `Send + Sync`
   reasoning and makes multi-instance testing impossible. Rejected.

### Consequences

- The no-governor code path (built-in clamp) must remain correct and tested;
  it is the production default for any loop not using the Kirra governor crate.
- A governor injected via `with_governor` is the sole output gate; any
  additional clamping must be implemented inside the governor itself.
- `SafetyGovernor` must be `Send + Sync` so `ControlLoop` instances can be
  sent across threads.

---

## ADL-002 — Built-in clamp interaction with injected governor

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

When a `SafetyGovernor` is injected, the built-in scalar clamp in
`ControlLoop::tick` is bypassed **entirely**. The governor receives the raw
proposed output and is solely responsible for producing a safe value.
The `builtin_clamp_ceiling` helper is exposed as `pub(crate)` so proptest
suites can verify that the governor's output never exceeds what the built-in
clamp would have allowed.

### Why

Partial suppression (both governor and clamp active) creates ambiguity: which
bound wins, does the order matter, and can a governor intentionally allow a
higher value for a platform with a wider safe envelope? Full suppression makes
the contract explicit — if you inject a governor, you own the safety guarantee.
The `test_builtin_clamp_suppressed` acceptance test enforces this invariant.

### Alternatives Considered

1. **Governor narrows, clamp catches escapes**: A governor that intentionally
   allows values the default clamp would reject (valid for custom envelopes)
   would be silently overridden. This defeats the purpose of injection. Rejected.
2. **Both active, minimum wins**: Compositional but masks governor
   misconfiguration — an overly permissive governor would be silently corrected
   rather than failing loudly in tests. Rejected.

### Consequences

- Any crate injecting a governor must include property tests asserting its
  output is within the desired safety envelope for all posture states.
- The `builtin_clamp_ceiling(proposed, state) -> f64` helper must track the
  production clamp logic exactly; if the clamp is changed, the helper must
  be updated in the same commit.

---

## ADL-003 — InferenceBackend trait: zero-copy hot-path contract

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

The `InferenceBackend` trait defines the hot-path method as:
```rust
fn run(&self, input: &[f32], output: &mut [f32]) -> Result<(), BackendError>;
```
All backend implementations must pre-allocate all scratch memory at init
(`new()`). No heap allocation is permitted on the `run` path.

The multi-silicon backend architecture (`BackendDescriptor`, `BackendSelector`,
QNN/TIDL/ROCm/OpenVINO backends) is defined and specced but **not yet
implemented** as of this ADL. Only the CPU ONNX backend (`parko-onnx`) and
the test-only `MockBackend` currently implement the trait.

### Why

Inference is called at the control-loop tick rate (50–200 Hz on embedded
targets). Dynamic allocation on every call would cause latency spikes and
potential OOM in bounded-memory safety contexts. A caller-provided output
slice allows buffer reuse across ticks. `BackendError::ShapeMismatch` provides
a typed failure path for length mismatches without panic.

### Alternatives Considered

1. **Return `Vec<f32>`**: Simple API but allocates on every call. Incompatible
   with real-time and ASIL-D constraints. Rejected.
2. **Interior mutability buffer (`&self` returns a reference to internal buffer)**:
   Unsafe under concurrent access; lifetime complexity outweighs the benefit.
   Rejected.
3. **`unsafe` raw pointer pair**: Zero-copy without Rust safety guarantees.
   Rejected unless a specific hardware SDK forces it (to be revisited in
   PARK-020 through PARK-023 if FFI requires it).

### Consequences

- Every backend must document required input/output slice lengths in its
  constructor or `BackendDescriptor`.
- Callers pre-allocate output buffers; `InferenceLoop` owns and reuses these
  for its lifetime.
- Shape mismatch returns `BackendError::ShapeMismatch`; it never panics.
- When real hardware backends are implemented (PARK-020–PARK-023), this ADL
  must be revisited if any SDK cannot satisfy the no-alloc constraint.

---

## ADL-004 — Deterministic tick grid: `VirtualClock` / `SystemClock` abstraction

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

`ControlLoop` and `InferenceLoop` in `parko-core` accept a `Clock` trait object
(`Arc<dyn Clock>`). Two implementations ship: `SystemClock` (wraps
`std::time::Instant`) and `VirtualClock` (manually advanceable `AtomicU64`).
All timing logic calls `self.clock.now_ms()`; no direct use of
`std::time::Instant::now()` inside timing-sensitive code.

The same `Clock` abstraction is used in `kirra-runtime-sdk` (`src/clock.rs`)
for `ScenarioRunner` and the telemetry watchdog. The two crates share the
concept but may not share the same type; use the parko-core definition for
parko-core tests and the kirra-runtime-sdk definition for integration tests.

### Why

Tests that verify timing behaviour (watchdog timeouts, rate limiters, the
5-tick recovery hysteresis) cannot use wall-clock time reliably in CI.
`VirtualClock::advance(ms)` lets tests advance time synchronously, making
them fast, deterministic, and independent of system load. This also eliminates
`std::thread::sleep` from any test in these crates.

### Alternatives Considered

1. **`tokio::time::pause()`**: Only works in a Tokio context. `parko-core` must
   remain async-runtime agnostic. Rejected.
2. **`std::thread::sleep` in tests**: Slow and flaky under CI load. Rejected.
3. **`#[cfg(test)]` mock field on the struct**: Couples test infrastructure to
   the production type; harder to compose across crates. Rejected.

### Consequences

- All duration constants (watchdog timeout, hysteresis window, tick period) are
  compared against `clock.now_ms()`, never against `Instant::now()` directly.
- `ControlLoop::new()` accepts `Arc<dyn Clock>`; callers that don't care about
  time pass `Arc::new(SystemClock)`.
- IEEE 2846 behavioral safety integration (PARK-013–PARK-019) will use
  `VirtualClock` for the adversarial simulation in PARK-019.

---

## ADL-005 — Safety posture state machine: asymmetric transitions and hysteresis

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

`FleetPosture` (in `kirra-runtime-sdk`) has three variants: `Nominal`,
`Degraded`, `LockedOut`. Fault transitions are instantaneous (one bad event →
Degraded; configured consecutive violations → LockedOut). Recovery from
Degraded requires `AV_RECOVERY_STREAK_THRESHOLD = 5` consecutive clean reports
within `AV_RECOVERY_WINDOW_MS = 10 000` ms. Recovery from LockedOut is manual
(requires supervisor reset key; no automatic hysteresis).

PostureState in `parko-core` mirrors this three-variant structure for use in
governor and control-loop logic.

This state machine governs behavioral safety (IEEE 2846 RSS integration,
PARK-013–PARK-019) and all other fault sources. IEEE 2846 is planned but not
yet implemented; the hysteresis constants are already in production for other
fault types (attestation, telemetry watchdog).

The safety case mappings for IEC 61508 SIL 3 and ASTM F3269 (PARK-032,
PARK-033) will trace to this state machine when written.

### Why

Symmetric transitions (same threshold for fault and recovery) cause posture
flapping under noisy sensor streams — the system oscillates between Nominal and
Degraded rather than committing to either. Asymmetric hysteresis keeps the
system degraded long enough for an operator to diagnose the root cause. The
Nominal/Degraded/LockedOut distinction maps to ISO 26262 ASIL-D fail-safe
decomposition: Degraded is reduced-capability operation; LockedOut is
fail-closed with no automatic recovery.

### Alternatives Considered

1. **Two-state machine (Safe / Unsafe)**: Loses the distinction between
   "degraded but operable at reduced capability" and "fully locked out."
   ISO 26262 ASIL decomposition requires this separation. Rejected.
2. **Continuous health score**: Expressive but non-binary posture makes command
   routing policy ambiguous (a score of 0.49 vs 0.51 has no clear semantics at
   the actuator gate). Rejected.
3. **Symmetric hysteresis**: Too slow to detect faults in real-time; a 5-bad
   streak before Degraded would allow 5 unsafe ticks to pass. Rejected.

### Consequences

- `should_route_command` is the single authoritative gate; posture must be read
  from the cache, never recomputed inline in a handler.
- Any path that transitions to LockedOut must emit a `LockoutReason` audit chain
  entry before changing state.
- Recovery streak resets on any new fault during the hysteresis window — this is
  the stricter interpretation and must be preserved in all future changes to
  `recovery_hysteresis.rs`.
- When IEEE 2846 integration is implemented (PARK-015), RssViolation events
  must reset the streak to 0 on every violation tick.
