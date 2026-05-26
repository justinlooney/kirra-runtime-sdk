# Architecture Decision Log (ADL)

> Each entry records a decision, its context, the alternatives considered, and
> the reasoning. Entries are immutable once written; superseded decisions get a
> new entry referencing the old one.

---

## ADL-001 — Governor injection model

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision
`SafetyGovernor` is injected into `ControlLoop` via a builder method
(`with_governor`) rather than being a required constructor argument or a global
singleton. The governor is stored as `Option<Box<dyn SafetyGovernor>>` and the
loop checks for its presence on every tick.

### Context
A required constructor argument would break all existing `ControlLoop` call
sites and make the no-governor case (pure inference, no safety enforcement)
impossible. A global singleton creates hidden coupling, makes unit tests
order-dependent, and prevents running multiple loops with different governors in
the same process.

### Alternatives considered
1. **Required constructor argument** — rejected: breaks existing API, forces
   every user to supply a governor even when not needed.
2. **Global/thread-local governor** — rejected: hidden coupling, not
   testable in parallel, violates the principle that safety state is explicit.
3. **Governor as a Tower middleware layer** — deferred: valid for the HTTP
   gateway path (`KirraPolicyLayer` already does this) but the inference loop
   is not HTTP-driven; adding Tower here introduces unnecessary overhead.

### Consequences
The `Option` check adds one branch per tick. At 1 kHz this is negligible.
The built-in scalar clamp must be explicitly suppressed when a governor is
present to prevent two enforcement paths from conflicting.

---

## ADL-002 — Built-in degraded-mode logic interaction

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision
When a `SafetyGovernor` is attached to `ControlLoop`, the loop's built-in
scalar clamp is fully suppressed. The governor is solely responsible for
enforcement. The built-in clamp remains active only when no governor is
attached (the `None` branch).

### Context
If both enforcement paths run, they can produce conflicting outputs — for
example, the built-in clamp allows a value the governor would deny, or the
governor allows a value the built-in clamp reduces further. The resulting
behavior is non-deterministic with respect to the safety contract and
impossible to certify.

### Alternatives considered
1. **Run both and take the more conservative result** — rejected: the
   "more conservative" comparison requires knowledge of the output domain
   (scalar, vector, command struct) that the generic loop does not have;
   adds complexity and a hidden dependency on output ordering semantics.
2. **Warn in dev builds when both paths would disagree** — deferred: useful
   for debugging but not a production invariant; can be added later as a
   `#[cfg(debug_assertions)]` check without changing the decision.

### Consequences
Any caller that previously relied on the built-in clamp as a backstop must
either keep the no-governor path or ensure their `SafetyGovernor` implementation
covers all cases the built-in clamp handled. This is documented in the governor
contract (`docs/backend_contract.md`).

---

## ADL-003 — Backend trait zero-copy boundary

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision
`InferenceBackend::run` accepts `input: &[f32]` and writes to `output: &mut [f32]`
— caller-allocated slices with no internal heap allocation on the hot path.
Backends that require a different memory layout (e.g. quantized int8 for QNN/TIDL)
perform the conversion internally and are responsible for any associated allocation.

### Context
Safety-critical runtimes targeting embedded silicon (Qualcomm QNN, TI TIDL) have
deterministic memory budgets. Heap allocation on the inference hot path can cause
non-deterministic latency spikes and is incompatible with ASIL-D timing analysis.
Forcing zero-copy at the trait boundary makes this constraint explicit and
compiler-enforced.

### Alternatives considered
1. **`Vec<f32>` return type** — rejected: heap allocation on every inference tick;
   incompatible with deterministic timing analysis.
2. **`ndarray` / `nalgebra` tensors** — rejected: adds large dependencies, version
   coupling, and layout ambiguity (row vs. column major) across backends.
3. **Backend-specific associated types** — deferred: would allow backends to expose
   their native types but breaks generic composition in `InferenceLoop`; revisit
   when the number of backends justifies it.

### Consequences
Callers must pre-allocate output buffers. `InferenceLoop` owns these buffers for
the lifetime of the loop and passes slices to each tick. Backends that internally
require quantized formats allocate scratch space once at session creation, not per
inference.

---

## ADL-004 — Deterministic tick grid design

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision
`ControlLoop` advances on a fixed-period tick grid driven by a `Clock` trait
abstraction (`SystemClock` in production, `VirtualClock` in tests). The loop
does not self-pace based on inference completion time; if an inference tick
exceeds its deadline, the loop emits a `LatencyViolation` event and holds the
last safe output rather than blocking.

### Context
Deterministic timing is a hard requirement for ASIL-D certification. A
self-pacing loop (tick fires when inference finishes) has unbounded jitter that
cannot be analyzed statically. The `VirtualClock` design is already proven in
`kirra-runtime-sdk`'s `ScenarioRunner` and `telemetry_watchdog`; the same
pattern applies here.

### Alternatives considered
1. **Self-pacing loop (fire when ready)** — rejected: jitter is unbounded, not
   analyzable for worst-case execution time (WCET), incompatible with ISO 26262
   temporal analysis requirements.
2. **OS real-time scheduling (SCHED_FIFO)** — complementary, not alternative:
   OS-level RT scheduling reduces jitter but does not eliminate it; the
   `LatencyViolation` + hold-last-safe-output fallback is still required.
3. **Hardware timer interrupt-driven loop** — deferred: correct for bare-metal
   RTOS targets; `VirtualClock` abstraction allows this to be wired in later
   without changing `ControlLoop` internals.

### Consequences
The `Clock` trait must be passed at construction time (or defaulted to
`SystemClock`). All time-dependent tests use `VirtualClock::advance` to
simulate elapsed time without sleeping. The loop's maximum tick rate is bounded
by `InferenceBackend` latency P99; backend selection must be validated against
the target tick rate during integration testing.

---

## ADL-005 — Safety posture state machine

**Date:** 2026-05-26
**Status:** Accepted
**Deciders:** Justin Looney

### Decision
The safety posture state machine has three states — `Nominal`, `Degraded`,
`LockedOut` — with asymmetric transitions: degradation is immediate (one bad
event), recovery requires a configurable streak of consecutive healthy ticks
(default: 5, within a 10 s window). This matches the `AV_RECOVERY_STREAK_THRESHOLD`
already enforced in `kirra-runtime-sdk`'s `recovery_hysteresis` module.

### Context
Symmetric state machines (one bad event degrades, one good event recovers)
are unsafe in autonomous systems: a single noisy sensor reading would cause
rapid posture flapping, causing the safety layer to oscillate between allowing
and denying commands. The hysteresis model is directly derived from the
fail-closed safety philosophy: the cost of a false positive (staying degraded
too long) is far lower than a false negative (recovering prematurely).

### Alternatives considered
1. **Symmetric transitions** — rejected: leads to posture flapping under
   noisy sensor conditions; cannot be certified under ISO 26262 as it violates
   the fail-closed requirement.
2. **Exponential back-off recovery** — considered: more nuanced than a fixed
   streak but harder to analyze and certify; the fixed-streak model has a
   deterministic worst-case recovery time that can be documented in the HARA.
3. **External posture override via admin API** — complementary: `kirra-runtime-sdk`
   already supports operator-initiated recovery via `KIRRA_ADMIN_TOKEN`; this
   is an escape hatch, not the normal recovery path.

### Consequences
Any new subsystem that feeds into posture evaluation (RSS violations, backend
latency watchdog, telemetry silence) must declare its recovery threshold
explicitly and document it in the FMEA. The 5-tick / 10 s default must be
validated per deployment; high-speed systems (>100 Hz tick rate) may need a
larger streak count to cover the same wall-clock window.
