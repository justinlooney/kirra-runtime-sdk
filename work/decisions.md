# Architecture Decision Log (ADL)

> Entries are immutable once written. Superseded decisions get a new entry
> referencing the old one by ADL number. Date format: YYYY-MM-DD.

---

## ADL-001 — Governor injection model: `Option<Box<dyn SafetyGovernor>>`

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

`ControlLoop` stores a governor as `Option<Box<dyn SafetyGovernor>>`. The
built-in scalar clamp is suppressed when a governor is present. Injection
is via a builder method `with_governor(Box<dyn SafetyGovernor>)`.

### Why

Safety policies are domain-specific (kinematics envelopes for ground vehicles
differ from aerial vehicles). A trait object lets each deployment inject its
own policy without forking `parko-core`. The `Option` preserves backward
compatibility: loops without an injected governor retain the existing built-in
clamp behaviour.

### Alternatives Considered

1. **Compile-time generics** (`ControlLoop<G: SafetyGovernor>`): Avoids heap
   allocation but forces every caller to specify the type parameter and makes
   default (no-governor) construction awkward. Rejected: usability cost
   outweighs the alloc saving on a non-hot init path.
2. **Function pointer** (`fn(f64, PostureState) -> f64`): Simpler, but
   stateless — governors that require calibration data (e.g., max lateral
   acceleration from a loaded config) need to close over state. Rejected.
3. **Global registry**: Couples crate to a global; breaks `Send + Sync`
   reasoning. Rejected.

### Consequences

- All `ControlLoop` tests that check the built-in clamp must remain and continue
  to pass (the no-governor code path must stay correct).
- A governor injected via `with_governor` is the sole output gate; any
  additional clamping must be done inside the governor itself.
- `SafetyGovernor` must be `Send + Sync` so loops can be sent across threads.

---

## ADL-002 — Built-in clamp suppression semantics

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

When a `SafetyGovernor` is injected, the built-in scalar clamp in
`ControlLoop::tick` is bypassed entirely. The governor receives the raw
proposed output and is solely responsible for producing a safe value.

### Why

Partial suppression (governor + clamp both active) creates ambiguity: which
bound wins, and does the order matter? Full suppression makes the contract
explicit: if you inject a governor, you own the safety guarantee. This is
verified by the `test_builtin_clamp_suppressed` acceptance test.

### Alternatives Considered

1. **Governor narrows, clamp catches escapes**: Safer but harder to reason
   about. A governor that intentionally allows values the default clamp would
   reject (valid for custom envelopes) would be silently overridden. Rejected.
2. **Both active, minimum wins**: Compositional but masks misconfiguration —
   a governor that returns a too-high value would silently be corrected rather
   than failing loudly in tests. Rejected.

### Consequences

- Any downstream crate injecting a governor must include its own property tests
  asserting its output is within the desired safety envelope.
- The `builtin_clamp_ceiling` helper (pub(crate)) is exposed so governor
  property tests can assert governor_out <= ceiling without duplicating logic.

---

## ADL-003 — Backend zero-copy contract: `run(&self, input: &[f32], output: &mut [f32])`

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

The `InferenceBackend` trait's hot-path method is:
```rust
fn run(&self, input: &[f32], output: &mut [f32]) -> Result<(), BackendError>;
```
All scratch memory is pre-allocated at backend `init()`. No heap allocation
on the hot path.

### Why

Inference is called at the control-loop tick rate (typically 50–200 Hz on
embedded targets). Dynamic allocation on every tick would cause latency spikes
and potential OOM in bounded-memory safety contexts. Passing output as a
caller-provided mutable slice avoids a per-call allocation and lets the caller
reuse the same buffer indefinitely.

### Alternatives Considered

1. **Return `Vec<f32>`**: Simple API but allocates on every call. Rejected.
2. **Interior mutability buffer (`&self` returns a reference)**: Unsafe in the
   presence of multiple callers; lifetime complexity outweighs the benefit.
   Rejected.
3. **`unsafe` raw pointer pair**: Zero-copy but loses Rust safety guarantees.
   Rejected without a compelling hardware necessity.

### Consequences

- Every backend implementation must document the required input and output
  slice lengths in its `BackendDescriptor` or constructor.
- Callers must pre-allocate output buffers; the `InferenceLoop` struct owns
  and reuses these buffers for the lifetime of the loop.
- Shape mismatch (wrong slice length) must return `BackendError::ShapeMismatch`,
  never panic or write out-of-bounds.

---

## ADL-004 — Deterministic tick grid: `VirtualClock` / `SystemClock` abstraction

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

`ControlLoop` accepts a `Clock` trait object (`Arc<dyn Clock>`). Two
implementations ship with `parko-core`: `SystemClock` (wraps
`std::time::Instant`) and `VirtualClock` (manually advanceable counter).
All timing logic inside `ControlLoop` calls `self.clock.now_ms()`.

### Why

Tests that verify timing behaviour (watchdog timeouts, rate limiters, hysteresis
windows) cannot use wall-clock time reliably in CI. Injecting a `VirtualClock`
lets tests advance time by arbitrary amounts synchronously, making them fast,
deterministic, and independent of system load.

### Alternatives Considered

1. **`tokio::time::pause()`**: Only works in a Tokio context; `parko-core` must
   remain `async`-runtime agnostic. Rejected.
2. **`std::thread::sleep` in tests**: Slow and flaky under load. Rejected.
3. **Mocking at the `ControlLoop` level** (e.g., `#[cfg(test)]` field): Couples
   test infrastructure to the production type; harder to test across crates.
   Rejected.

### Consequences

- `ControlLoop::new()` accepts a `Arc<dyn Clock>` parameter; callers that
  don't care about time pass `Arc::new(SystemClock)`.
- `VirtualClock::advance(ms: u64)` is the only public mutation method; tests
  call it between ticks to simulate elapsed time.
- All constants that represent durations (watchdog timeout, hysteresis window)
  are compared against `clock.now_ms()` — never against
  `std::time::Instant::now()` directly.

---

## ADL-005 — Safety posture state machine: asymmetric transitions and 5-tick hysteresis

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision

`PostureState` has three variants: `Nominal`, `Degraded`, `LockedOut`.
Fault transitions are instantaneous (one bad tick → Degraded; configured
consecutive violations → LockedOut). Recovery requires
`AV_RECOVERY_STREAK_THRESHOLD = 5` consecutive clean ticks within
`AV_RECOVERY_WINDOW_MS = 10_000` ms. Recovery from LockedOut is manual
(requires supervisor reset key, not automatic hysteresis).

### Why

Symmetric transitions (same threshold for fault and recovery) lead to
"posture flapping" under noisy sensor streams — the system oscillates between
Nominal and Degraded rather than committing to either. Asymmetric hysteresis
ensures the system stays degraded long enough for an operator to diagnose the
cause. The distinction between Degraded (recoverable) and LockedOut
(manual-only) mirrors ASIL-D fail-safe architecture requirements.

### Alternatives Considered

1. **Two-state machine (Safe / Unsafe)**: Simpler but loses the distinction
   between "degraded but operable at reduced capability" and "fully locked out."
   Required for ISO 26262 ASIL decomposition. Rejected.
2. **Continuous health score**: More expressive but non-binary posture makes
   command routing policy complex; a score of 0.49 vs 0.51 has unclear
   semantics at the actuator gate. Rejected.
3. **Symmetric hysteresis (5 clean to recover, 5 bad to degrade)**: Too slow
   to detect faults in real-time. Rejected.

### Consequences

- `should_route_command` is the single authoritative gate for all commands;
  posture state must be read from the cache, not recomputed inline.
- Any path that transitions to LockedOut must emit a `LockoutReason` entry
  to the audit chain before changing state.
- Recovery streak resets on any new fault during the hysteresis window, not
  just at window expiry. This is the stricter interpretation; it must be
  preserved in all future changes to `recovery_hysteresis.rs`.
