# Active Work

> Maximum 3 tasks in flight at once (WIP limit). Matches In Progress column on
> the project board. Pull from Ready when a slot opens; push to done.md when
> the PR merges.

---

## PARK-001 — Implement `ControlLoop::with_governor` builder

**Epic:** `epic:runtime-core` | **Milestone:** v0.1 | **Branch:** `park-001/governor-builder`
**Labels:** `feat`, `control-loop`, `in-progress`

### Summary

Adds a builder method to `ControlLoop` that accepts a `Box<dyn SafetyGovernor>`
and stores it as `Option<Box<dyn SafetyGovernor>>`. When a governor is present
the built-in scalar clamp in `tick()` must be bypassed entirely — the governor
is the sole output gate. This is the foundation for pluggable safety policies
used by all downstream tests.

### Acceptance Criteria
- [ ] `with_governor(gov: Box<dyn SafetyGovernor>) -> Self` added to `ControlLoop`
- [ ] Governor stored as `Option<Box<dyn SafetyGovernor>>` on the struct
- [ ] `test_builtin_clamp_suppressed`: governor returning 0.0 beats a large
      proposed output; built-in clamp would have passed it
- [ ] All pre-existing `parko-core` tests still pass
- [ ] No `unsafe` code
- [ ] `cargo test -p parko-core` exits 0

### Claude Code Prompt

```
You are working in the parko-core crate of the kirra-runtime-sdk repository.
The file to edit is parko-core/src/control_loop.rs.

Task: Add a `with_governor` builder method to `ControlLoop` that accepts an
injected SafetyGovernor and suppresses the built-in scalar clamp when one is
present.

Requirements:
1. Add a field `governor: Option<Box<dyn SafetyGovernor>>` to `ControlLoop`.
2. Add `pub fn with_governor(mut self, gov: Box<dyn SafetyGovernor>) -> Self`
   that sets the field and returns self (builder pattern).
3. In `ControlLoop::tick`, when `self.governor.is_some()`, skip the built-in
   scalar clamp and pass `proposed_output` directly to
   `governor.enforce(proposed_output, self.posture_state)`. When
   `self.governor.is_none()`, preserve existing built-in clamp logic.
4. `SafetyGovernor` trait (if not yet defined) must be in
   parko-core/src/governor.rs, re-exported from the crate root:
     pub trait SafetyGovernor: Send + Sync {
         fn enforce(&self, proposed: f64, posture: PostureState) -> f64;
     }
5. Write test `test_builtin_clamp_suppressed` in parko-core/tests/governor.rs:
   - Create a `ZeroGovernor` that always returns 0.0.
   - Inject it via `with_governor`.
   - Call `tick` with a large proposed output that would pass the built-in clamp.
   - Assert the result equals 0.0.
6. Ensure all existing tests in `parko-core` continue to pass.
7. No unsafe code.
8. Run `cargo test -p parko-core` and confirm exit 0 before declaring done.
```

---

## PARK-002 — Add `set_state_for_test` behind `#[cfg(test)]`

**Epic:** `epic:runtime-core` | **Milestone:** v0.1 | **Branch:** `park-002/test-state-setter`
**Labels:** `feat`, `control-loop`, `in-progress`

### Summary

Adds a `set_state_for_test(state: PostureState)` method to `ControlLoop`,
guarded by `#[cfg(test)]`, so posture-divergence property tests can drive the
loop into Degraded or LockedOut without exposing a mutation path in release
binaries. Confirmed absent from release builds via `nm`.

### Acceptance Criteria
- [ ] `set_state_for_test` visible only under `#[cfg(test)]`
- [ ] `cargo build --release` binary has no symbol matching `set_state_for_test`
      (verified with `nm target/release/... | grep set_state_for_test` → empty)
- [ ] `cargo test` exposes the method so posture tests can call it
- [ ] All existing tests pass
- [ ] `cargo test -p parko-core` exits 0

### Claude Code Prompt

```
You are working in parko-core/src/control_loop.rs in the kirra-runtime-sdk
repository.

Task: Add a `set_state_for_test` method to `ControlLoop` that is compiled
only under `#[cfg(test)]`.

Requirements:
1. Add the following block to the `ControlLoop` impl:
     #[cfg(test)]
     pub fn set_state_for_test(&mut self, state: PostureState) {
         self.posture_state = state;
     }
2. Do not modify any production code paths.
3. Write a test in parko-core/tests/posture_state.rs that:
   - Creates a `ControlLoop`.
   - Calls `set_state_for_test(PostureState::Degraded)`.
   - Calls `tick` and asserts the output is consistent with Degraded behaviour
     (e.g., clamped to a lower ceiling, or rejected if the loop enforces that).
4. After building, run:
     nm target/release/kirra_verifier_service | grep set_state_for_test
   and confirm the output is empty (symbol must not appear in release binary).
5. `cargo test -p parko-core` must exit 0.
```

---

## PARK-003 — Posture-divergence proptest suite

**Epic:** `epic:runtime-core` | **Milestone:** v0.1 | **Branch:** `park-003/posture-divergence-proptest`
**Labels:** `test`, `control-loop`, `in-progress`

### Summary

Uses `proptest` to assert that for every valid `(proposed_output, posture_state)`
pair the governor's output never exceeds the built-in clamp ceiling. This is the
core correctness invariant: a governor can only tighten or match the built-in
limits, never loosen them.

### Acceptance Criteria
- [ ] Property test in `parko-core/tests/governor_proptest.rs`
- [ ] At least 10 000 cases per PostureState variant (Nominal, Degraded, LockedOut)
- [ ] Asserts `governor_output <= builtin_clamp_ceiling(proposed_output, state)`
      for every generated case
- [ ] `cargo test -p parko-core` exits 0

### Claude Code Prompt

```
You are working in the parko-core crate of the kirra-runtime-sdk repository.
Add proptest at dev-dependency level if not already present.

Task: Write a proptest suite that asserts the governor output is always <= the
built-in clamp ceiling for all valid inputs.

Requirements:
1. Add to Cargo.toml of parko-core (dev-dependencies):
     proptest = "1"
2. Create parko-core/tests/governor_proptest.rs.
3. Write a property test using the proptest! macro:
   proptest! {
       #[test]
       fn governor_never_exceeds_builtin_clamp(
           proposed in -1000.0f64..1000.0f64,
           state in prop_oneof![
               Just(PostureState::Nominal),
               Just(PostureState::Degraded),
               Just(PostureState::LockedOut),
           ]
       ) {
           let ceiling = builtin_clamp_ceiling(proposed, state);
           let governor_out = /* instantiate a PassthroughGovernor and call enforce */;
           prop_assert!(governor_out <= ceiling);
       }
   }
4. `builtin_clamp_ceiling(proposed: f64, state: PostureState) -> f64` must be a
   pub(crate) helper in parko-core/src/control_loop.rs that returns the same
   ceiling value the built-in clamp would produce (so the test verifies the
   real production logic, not a copy of it).
5. Run with `cargo test -p parko-core -- --test-threads=1` to confirm >= 10 000
   cases pass for each PostureState variant. Set proptest cases to 10_000 via
   `#[proptest(cases = 10_000)]` or via ProptestConfig.
6. No unsafe code.
```
