# Active Work

> Maximum 3 tasks in flight at once (WIP limit). Matches In Progress column on
> the project board. Pull from Ready when a slot opens; move to done.md on merge.
> Note: kirra-runtime-sdk holds ~333 tests; parko-core has its own separate test
> suite. Do not conflate the two when writing prompts or acceptance criteria.

---

## PARK-001 — Implement `ControlLoop::with_governor` builder

**Epic:** `epic:runtime-core` | **Milestone:** v0.1 | **Branch:** `park-001/governor-builder`
**Labels:** `feat`, `control-loop`, `in-progress`

### Summary

Adds a builder method to `ControlLoop` in `parko-core` that accepts a
`Box<dyn SafetyGovernor>` and stores it as `Option<Box<dyn SafetyGovernor>>`.
When a governor is present, the built-in scalar clamp in `tick()` is bypassed
entirely — the governor is the sole output gate. This is the foundation for
the Kirra governor crate's integration with the parko-core inference loop.

### Acceptance Criteria
- [ ] `with_governor(gov: impl SafetyGovernor + 'static) -> Self` added to `ControlLoop`
- [ ] Governor stored as `Option<Box<dyn SafetyGovernor>>` on the struct
- [ ] `test_builtin_clamp_suppressed`: a `ZeroGovernor` returning 0.0 wins
      over a large proposed output that would pass the built-in clamp
- [ ] `test_no_governor_uses_builtin_clamp`: without a governor the built-in
      clamp still fires
- [ ] All pre-existing parko-core tests remain green
- [ ] No `unsafe` code
- [ ] `cargo test -p parko-core` exits 0

### Claude Code Prompt

```
You are working in the parko-core crate. The project uses Kirra as its safety
runtime name — there is no "Aegis" in this codebase.

Task: Add a `with_governor` builder to `ControlLoop` in parko-core/src/control_loop.rs
(or the equivalent entry point for the control loop).

Requirements:
1. Add field: governor: Option<Box<dyn SafetyGovernor>> to ControlLoop struct.
2. Add method: pub fn with_governor(mut self, g: impl SafetyGovernor + 'static) -> Self
   that sets governor = Some(Box::new(g)) and returns self.
3. In tick(): if self.governor.is_some() {
                  let g = self.governor.as_ref().unwrap();
                  return g.enforce(proposed_output, self.posture_state);
              } else {
                  // run existing built-in clamp
              }
4. SafetyGovernor trait (if not yet defined) in parko-core/src/governor.rs:
     pub trait SafetyGovernor: Send + Sync {
         fn enforce(&self, proposed: f64, posture: PostureState) -> f64;
     }
   Re-export from parko_core lib.rs: pub use governor::SafetyGovernor;
5. Write test `test_builtin_clamp_suppressed` in parko-core/tests/governor.rs:
   - Define ZeroGovernor: always returns 0.0.
   - Inject via with_governor.
   - Call tick with a value above the built-in clamp threshold.
   - Assert result == 0.0.
6. Write test `test_no_governor_uses_builtin_clamp`:
   - Create ControlLoop without governor.
   - Call tick with a value above the clamp threshold.
   - Assert result == clamped_value (not the raw proposed output).
7. All existing parko-core tests must pass (do not assume any specific count;
   run `cargo test -p parko-core` and confirm exit 0).
8. No unsafe code.
```

---

## PARK-002 — Add `set_state_for_test` behind `#[cfg(test)]`

**Epic:** `epic:runtime-core` | **Milestone:** v0.1 | **Branch:** `park-002/test-state-setter`
**Labels:** `feat`, `control-loop`, `in-progress`

### Summary

Adds a `set_state_for_test(state: PostureState)` method to `ControlLoop` in
`parko-core`, guarded by `#[cfg(test)]`. This is a test seam — it mutates
internal posture state directly without transition validation, unblocking
posture-divergence property tests. The method must be absent from release
builds (verified with `nm`) and present only during `cargo test`.

### Acceptance Criteria
- [ ] `set_state_for_test` visible only under `#[cfg(test)]`
- [ ] `cargo build --release` binary has no symbol matching `set_state_for_test`
      (confirmed with `nm <binary> | grep set_state_for_test` → empty)
- [ ] `cargo test -p parko-core` exposes the method to tests
- [ ] All pre-existing parko-core tests remain green
- [ ] `cargo test -p parko-core` exits 0

### Claude Code Prompt

```
You are working in parko-core/src/control_loop.rs. The project uses Kirra as
its safety runtime name.

Task: Add a cfg(test) method `set_state_for_test` to `ControlLoop`.

Requirements:
1. Add this block to the ControlLoop impl:
     #[cfg(test)]
     pub fn set_state_for_test(&mut self, state: PostureState) {
         self.posture_state = state;
     }
   Do not add a doc comment referencing internal details; a single-line
   inline comment is acceptable: // Test seam — absent from release builds.
2. Do not touch any production code paths.
3. Write a test in parko-core/tests/posture_state.rs that:
   - Creates a ControlLoop.
   - Calls set_state_for_test(PostureState::Degraded).
   - Calls tick with a nominal input.
   - Asserts the output is consistent with Degraded behaviour
     (e.g., clamped to a lower ceiling than Nominal).
4. After building in release mode, run:
     nm target/release/<binary> | grep set_state_for_test
   Confirm the output is empty. (The binary name may be kirra_verifier_service
   or whichever binary links parko-core; check workspace Cargo.toml.)
5. cargo test -p parko-core exits 0. Do not assume a specific test count.
6. No unsafe code.
```

---

## PARK-003 — Posture-divergence proptest suite

**Epic:** `epic:runtime-core` | **Milestone:** v0.1 | **Branch:** `park-003/posture-divergence-proptest`
**Labels:** `test`, `control-loop`, `in-progress`

### Summary

Uses `proptest` to assert that for every valid `(proposed_output, PostureState)`
pair the Kirra governor's output is at least as conservative as the built-in
clamp ceiling. This is the core correctness invariant for the governor
integration: a governor may only tighten or match the built-in limits, never
loosen them. Depends on PARK-002 (`set_state_for_test`) to inject posture states.

### Acceptance Criteria
- [ ] Property test in `parko-core/tests/posture_divergence.rs`
- [ ] At least 10 000 cases per PostureState variant (Nominal, Degraded, LockedOut)
- [ ] Asserts `governor_output <= builtin_clamp_ceiling(proposed, state)`
      for every generated case
- [ ] `cargo test -p parko-core` exits 0

### Claude Code Prompt

```
You are working in the parko-core crate. The project uses Kirra as its safety
runtime name. PARK-002 must be complete (set_state_for_test available) before
this task can be implemented.

Task: Write a proptest suite asserting the Kirra governor output is always at
least as conservative as the built-in clamp ceiling.

Requirements:
1. Add proptest = "1" to parko-core dev-dependencies if not already present.
2. Create parko-core/tests/posture_divergence.rs.
3. Expose a pub(crate) helper in parko-core/src/control_loop.rs:
     pub(crate) fn builtin_clamp_ceiling(proposed: f64, state: PostureState) -> f64
   This must return the exact ceiling the built-in clamp would apply, so tests
   verify real production logic rather than a copy of it.
4. Write three proptest! blocks (one per PostureState variant):
   proptest! {
       #[test]
       fn governor_never_exceeds_builtin_clamp_nominal(
           proposed in -1000.0f64..1000.0f64
       ) {
           prop_assume!(!proposed.is_nan() && !proposed.is_infinite());
           let ceiling = builtin_clamp_ceiling(proposed, PostureState::Nominal);
           // instantiate the Kirra governor (from the Kirra governor crate;
           // verify the crate name in workspace Cargo.toml before importing)
           let gov_out = governor.enforce(proposed, PostureState::Nominal);
           prop_assert!(gov_out <= ceiling,
               "governor {} > ceiling {} for proposed {}", gov_out, ceiling, proposed);
       }
   }
   Repeat for Degraded (assert gov_out <= ceiling) and LockedOut
   (assert gov_out == 0.0).
5. Set cases = 10_000 per block via ProptestConfig or the #[proptest] attribute.
6. Add the Kirra governor crate as a dev-dependency in parko-core/Cargo.toml.
   Check workspace Cargo.toml for the exact crate name before editing.
7. Run `cargo test -p parko-core -- --test-threads=1` and confirm all pass.
8. Do not assume any specific existing test count in parko-core.
9. No unsafe code.
```
