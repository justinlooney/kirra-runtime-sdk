# Architecture Decision Log (ADL)

> Entries are immutable once written. Superseded decisions get a new entry
> referencing the old one by ADL number. Date format: YYYY-MM-DD.

---

## ADL-001 — Governor injection and authority model

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

`ControlLoop` in `parko-core` stores a governor as
`Option<Box<dyn SafetyGovernor>>`, injected via a builder method
`with_governor(impl SafetyGovernor + 'static)`. KirraGovernor holds final
authority over every command on the hot path. The authority model is:

- **LockedOut / Degraded:** hard veto — KirraGovernor returns 0.0 (or
  `EnforcementAction::Halt`) unconditionally.
- **Nominal:** KirraGovernor may clamp but not fully veto unless a safety
  constraint (RSS gate, kinematic envelope) is violated.
- **Governor unreachable (timeout / partition):** `ControlLoop` drops posture to
  Degraded, applies the built-in conservative fallback envelope, and logs a
  `governor_unreachable` safety event before the next tick.

The synchronous call path is: `planned_cmd → governor → final_cmd`. There is no
concurrent or asynchronous governor path in the control loop.

### Why

Safety policies are domain-specific; a trait object lets each deployment inject
KirraGovernor without forking `parko-core`. The `Option` preserves backward
compatibility. The hard-veto model on Degraded/LockedOut matches ISO 26262 ASIL-D
fail-closed semantics — a degraded system must never issue an unrestricted command.
The fallback-to-conservative-envelope rule ensures the loop never runs unguarded
even if the governor crate is temporarily unavailable.

### Alternatives Considered

1. **Compile-time generics** (`ControlLoop<G: SafetyGovernor>`): avoids heap
   allocation but makes default (no-governor) construction awkward. Rejected.
2. **Function pointer** (`fn(f64, PostureState) -> f64`): stateless; governors
   that require calibration data must close over state, which this can't express.
   Rejected.
3. **Best-effort async governor:** introduces non-determinism on the hot path.
   Rejected — synchronous path is required for determinism.

### Consequences

- The no-governor code path (built-in clamp) must remain correct and tested.
- Any crate injecting a governor owns the full safety guarantee for that loop.
- `SafetyGovernor` must be `Send + Sync`.
- The governor crate name must be verified in the repo before any rename task
  is written (it may still be `AegisGovernor` or similar).

---

## ADL-002 — Built-in degraded-mode clamp interaction with KirraGovernor

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

When a `SafetyGovernor` is injected, the built-in scalar clamp in
`ControlLoop::tick` is bypassed **entirely**. The governor receives the raw
proposed output and is solely responsible for producing a safe value. The
`builtin_clamp_ceiling(proposed, state) -> f64` helper is exposed as
`pub(crate)` so property tests can verify the governor never exceeds the
ceiling the built-in clamp would have applied.

If the governor becomes unreachable (see ADL-001), the built-in clamp is
re-activated as a conservative fallback, not as a co-enforcer — partial
suppression (both active simultaneously) is never the intended state.

### Why

Partial suppression creates ambiguity: which bound wins, does order matter, and
can a governor intentionally allow a wider envelope for a custom platform? Full
suppression makes the contract explicit. The `test_builtin_clamp_suppressed`
acceptance test enforces this invariant.

### Consequences

- `builtin_clamp_ceiling` must track production clamp logic exactly; any change
  to the clamp must update the helper in the same commit.
- Any crate injecting a governor must include property tests asserting its output
  is within the desired safety envelope for all posture states.

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

All backend implementations must pre-allocate all scratch memory at `new()`. No
heap allocation is permitted on the `run` path. Shape mismatch returns
`BackendError::ShapeMismatch`; it never panics.

The multi-silicon backend architecture (`BackendDescriptor`, TensorRT, QNN, TIDL,
OpenVINO, AMD) is defined and specced but **not yet implemented** as of this ADL.
Only the CPU ONNX backend (`parko-onnx`) currently implements the trait; its MNIST
integration test must be verified before being treated as green. `MockBackend` is
the preferred backend for all parko-core unit tests.

### Why

Inference runs at 50–200 Hz on embedded targets. Dynamic allocation on every call
causes latency spikes and potential OOM in bounded-memory safety contexts. A
caller-provided output slice allows buffer reuse across ticks.

### Consequences

- Every backend must document required input/output slice lengths.
- `InferenceLoop` owns and reuses output buffers for its lifetime.
- When TensorRT and QNN backends are implemented (PARK-020–PARK-027), this ADL
  must be revisited if any SDK cannot satisfy the no-alloc constraint.

---

## ADL-004 — Deterministic tick grid: `RuntimeClock` / `MockClock` abstraction

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

`ControlLoop` and `InferenceLoop` in `parko-core` accept a `Clock` trait object
(`Arc<dyn Clock>`). Two implementations ship: `RuntimeClock` (wraps wall clock)
and `MockClock` (manually advanceable `AtomicU64`). All timing logic inside the
loop calls `self.clock.now_ms()`; no direct use of wall-clock APIs inside
timing-sensitive code.

The same `Clock` abstraction is used in `kirra-runtime-sdk` (`src/clock.rs`) for
`ScenarioRunner` and the telemetry watchdog. The two crates share the concept but
may not share the same type; use the parko-core definition for parko-core tests.

### Why

Tests that verify timing behaviour cannot use wall-clock time reliably in CI.
`MockClock::advance(ms)` lets tests advance time synchronously, making them fast,
deterministic, and independent of system load. This also eliminates
`std::thread::sleep` from any test in these crates.

### Consequences

- All duration constants are compared against `clock.now_ms()`, never against
  a wall-clock call directly.
- `ControlLoop::new()` accepts `Arc<dyn Clock>`; callers that don't care pass
  `Arc::new(RuntimeClock)`.
- The QNX deployment path (PARK-024) must verify that the clock abstraction
  compiles cleanly on QNX's POSIX subset before assuming compatibility.

---

## ADL-005 — Safety posture state machine: asymmetric transitions and hysteresis

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

`FleetPosture` (in `kirra-runtime-sdk`) has three variants: `Nominal`, `Degraded`,
`LockedOut`. Fault transitions are instantaneous (one bad event → Degraded;
configured consecutive violations → LockedOut). Recovery from Degraded requires
`AV_RECOVERY_STREAK_THRESHOLD = 5` consecutive clean reports within
`AV_RECOVERY_WINDOW_MS = 10,000` ms. Recovery from LockedOut is manual (requires
supervisor reset key; no automatic hysteresis).

`PostureState` in `parko-core` mirrors this three-variant structure for use in
governor and control-loop logic.

KirraGovernor authority maps onto this state machine:
- `Nominal` → KirraGovernor clamps; does not fully veto.
- `Degraded` → KirraGovernor hard-vetoes all commands.
- `LockedOut` → KirraGovernor hard-vetoes; manual reset required.

IEEE 2846 behavioral safety integration is planned but not yet implemented. When
implemented (PARK-015), RSS violations must reset the recovery streak to 0 on
every violation tick. IEC 61508 SIL 3 and ASTM F3269 safety case mappings
(PARK-039, PARK-040) will trace to this state machine when written.

### Why

Symmetric transitions cause posture flapping under noisy sensor streams.
Asymmetric hysteresis keeps the system degraded long enough for an operator to
diagnose the root cause. The Nominal/Degraded/LockedOut distinction maps to
ISO 26262 ASIL-D fail-safe decomposition.

### Consequences

- `should_route_command` is the single authoritative gate; posture must be read
  from the cache, never recomputed inline in a handler.
- Any path that transitions to LockedOut must emit a `LockoutReason` audit chain
  entry before changing state.
- Recovery streak resets on any new fault during the hysteresis window.
