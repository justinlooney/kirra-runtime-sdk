# Active

> Max 3 tasks in flight at once. Pull from `backlog.md`. Move to `done.md` on merge.

---

## In Progress

### PARK-001 — Attach governor to ControlLoop
**Label:** `control-loop`
**Backlog ref:** Increment 1, task 1.1

Implement `ControlLoop::with_governor(impl SafetyGovernor)` builder in `parko-core`.
When a governor is attached the built-in scalar clamp must be suppressed so the two
enforcement paths cannot fight. The governor receives the raw proposed output and
returns an `EnforcementAction`; the loop applies it unconditionally.

**Acceptance criteria:**
- `ControlLoop::with_governor` compiles and the builder pattern chains correctly
- Built-in clamp is bypassed when governor is present (verified by `test_builtin_clamp_suppressed`)
- `ControlLoop::without_governor` (default) still applies built-in clamp as before
- No `unsafe` code introduced
- All existing `parko-core` tests continue to pass

**Files likely touched:**
- `parko/parko-core/src/control_loop.rs`
- `parko/parko-core/src/lib.rs`
- `parko/parko-core/tests/`

---

### PARK-002 — Add test-only state setter
**Label:** `control-loop`
**Backlog ref:** Increment 1, task 1.2

Add `set_state_for_test(state: PostureState)` to `parko-core` behind `#[cfg(test)]`.
This unblocks writing posture-divergence tests without exposing a mutation path in
production binaries. Confirm with `cargo build --release` that the method is absent
from the compiled artifact.

**Acceptance criteria:**
- Method visible inside `#[cfg(test)]` modules and integration tests
- `cargo build --release` produces no symbol for `set_state_for_test` (verify with `nm`)
- Method panics if called with an invalid state transition (optional: enforce state machine)
- Existing `parko-core` tests unaffected

**Files likely touched:**
- `parko/parko-core/src/posture.rs` (or wherever `PostureState` lives)
- `parko/parko-core/src/control_loop.rs`

---

### PARK-003 — Write posture divergence test (property-based)
**Label:** `control-loop`
**Backlog ref:** Increment 1, task 1.3

Write a `proptest` suite asserting that for all valid `(proposed_output, posture_state)`
combinations the governor's `EnforcementAction` result is at least as conservative as
the built-in clamp would have been. This is the core correctness property for
`KirraKernelGovernor` as a `SafetyGovernor` implementation.

**Acceptance criteria:**
- `proptest` generates ≥ 10 000 cases covering full float range (excluding NaN/Inf)
- Test passes for all `PostureState` variants: `Nominal`, `Degraded`, `LockedOut`
- Test explicitly asserts governor output ≤ built-in clamp output (not just "doesn't panic")
- Test is in `parko/parko-core/tests/posture_divergence.rs` or similar
- `cargo test` passes with `proptest` as a `dev-dependency`; no new non-dev dependencies

**Files likely touched:**
- `parko/parko-core/tests/posture_divergence.rs` (new)
- `parko/parko-core/Cargo.toml` (`proptest` dev-dep, already present in workspace)
