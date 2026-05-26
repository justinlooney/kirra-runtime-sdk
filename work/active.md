# Active Work

> Maximum 3 tasks in flight at once (WIP limit). Matches In Progress column on
> the project board. Pull from Ready when a slot opens; move to done.md on merge.
>
> **Authority model reminder (all active tasks):** KirraGovernor hard-vetoes on
> Degraded/LockedOut, clamps on Nominal, falls back to conservative built-in
> envelope if unreachable. Synchronous path: `planned_cmd → governor → final_cmd`.
>
> **Test count reminder:** parko-core has ~30–40 tests (NOT 333). kirra-runtime-sdk
> holds ~333 tests. Do not conflate the two.

---

## PARK-001 — Attach `SafetyGovernor` to `ControlLoop`

**Epic:** `epic:runtime-core` | **Milestone:** v0.1 | **Branch:** `park-001/governor-builder`
**Labels:** `feat`, `control-loop`, `in-progress`

### Summary

Add `with_governor(impl SafetyGovernor + 'static)` builder to `ControlLoop` in
`parko-core`. The governor is stored as `Option<Box<dyn SafetyGovernor>>`. When a
governor is present, the built-in scalar clamp is suppressed — both enforcement
paths must not fire on the same tick. This is the foundation for KirraGovernor's
integration with the parko-core inference loop.

**Authority model:** KirraGovernor hard-vetoes on Degraded/LockedOut, clamps on
Nominal, falls back to conservative envelope if unreachable.

### Acceptance Criteria
- [ ] `with_governor(gov: impl SafetyGovernor + 'static) -> Self` added to `ControlLoop`
- [ ] Governor stored as `Option<Box<dyn SafetyGovernor>>` on the struct
- [ ] `test_builtin_clamp_suppressed`: a `ZeroGovernor` returning 0.0 wins over a
      large proposed output that would pass the built-in clamp
- [ ] `test_no_governor_uses_builtin_clamp`: without a governor the built-in clamp fires
- [ ] All pre-existing parko-core tests remain green
- [ ] No `unsafe` code
- [ ] `cargo test -p parko-core` exits 0

### Claude Code Prompt

```
You are working in the parko-core crate. Before writing any code, search the
workspace to find the actual crate and struct names:

  find parko/ -name "*.toml" | xargs grep -l "\[package\]"
  grep -r "SafetyGovernor\|Governor\|AegisGovernor\|KirraGovernor" parko/ --include="*.rs" -l

If the governor struct is named AegisGovernor or similar, rename it to
KirraGovernor in the same commit. Use Kirra naming in all new comments and docs.

Task: Add a `with_governor` builder method to `ControlLoop`.

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
5. Write test `test_builtin_clamp_suppressed`:
   - Define ZeroGovernor: always returns 0.0.
   - Inject via with_governor.
   - Call tick with a value above the built-in clamp threshold.
   - Assert result == 0.0 (governor wins).
6. Write test `test_no_governor_uses_builtin_clamp`:
   - Create ControlLoop without governor.
   - Call tick with value above clamp threshold.
   - Assert result == clamped value (not the raw proposed output).
7. Run `cargo test -p parko-core` and confirm exit 0.
   Do NOT assume any specific test count — confirm what exists before running.
   Do NOT assume the MNIST integration test is passing or relevant here.
8. No unsafe code.
```

---

## PARK-002 — Add test-only posture state setter

**Epic:** `epic:runtime-core` | **Milestone:** v0.1 | **Branch:** `park-002/test-state-setter`
**Labels:** `feat`, `control-loop`, `in-progress`

### Summary

Add `set_state_for_test(state: PostureState)` to `ControlLoop` in `parko-core`
behind `#[cfg(test)]`. This is a pure test seam — it mutates internal posture state
directly without transition validation, unblocking posture-divergence property tests.
The method must be absent from release builds (verified with `nm`).

**Authority model:** KirraGovernor hard-vetoes on Degraded/LockedOut, clamps on
Nominal, falls back to conservative envelope if unreachable.

### Acceptance Criteria
- [ ] `set_state_for_test` visible only under `#[cfg(test)]`
- [ ] `cargo build --release` binary has no symbol matching `set_state_for_test` (nm check)
- [ ] `cargo test -p parko-core` exposes the method to tests
- [ ] All pre-existing parko-core tests remain green
- [ ] `cargo test -p parko-core` exits 0

### Claude Code Prompt

```
You are working in parko-core/src/control_loop.rs. Before writing any code,
search the workspace for the actual governor struct and crate names:

  find parko/ -name "*.toml" | xargs grep -l "\[package\]"
  grep -r "AegisGovernor\|KirraGovernor\|Governor" parko/ --include="*.rs" -l

If the governor struct is named AegisGovernor or similar, note the rename
needed (KirraGovernor) but do not rename in this task — keep scope minimal.
Use Kirra naming in all new comments and docs.

Task: Add a cfg(test) method `set_state_for_test` to `ControlLoop`.

Requirements:
1. Add this block to the ControlLoop impl:
     #[cfg(test)]
     pub fn set_state_for_test(&mut self, state: PostureState) {
         self.posture_state = state;
     }
2. Do not touch any production code paths.
3. Write a test in parko-core/tests/ that:
   - Creates a ControlLoop.
   - Calls set_state_for_test(PostureState::Degraded).
   - Calls tick with a nominal input.
   - Asserts the output is consistent with Degraded behaviour (clamped to a
     lower ceiling than Nominal, or 0.0 if KirraGovernor is injected).
4. After a release build, run:
     nm target/release/<binary> | grep set_state_for_test
   Confirm the output is empty. (Locate the binary name from workspace Cargo.toml.)
5. cargo test -p parko-core exits 0.
   Do NOT assume any specific test count — confirm what exists.
   Do NOT assume the MNIST integration test is relevant here.
6. No unsafe code.
```

---

## PARK-003 — Write posture divergence property test

**Epic:** `epic:runtime-core` | **Milestone:** v0.1 | **Branch:** `park-003/posture-divergence-proptest`
**Labels:** `test`, `control-loop`, `in-progress`

### Summary

Proptest suite asserting: for every valid `(proposed_output: f64, posture_state: PostureState)`
pair, the KirraGovernor output is at least as conservative as the built-in clamp
ceiling. This is the core correctness invariant for the governor integration. Depends
on PARK-002 (`set_state_for_test`) to inject posture states; requires ≥ 10,000 cases
per PostureState variant.

**Authority model:** KirraGovernor hard-vetoes on Degraded/LockedOut, clamps on
Nominal, falls back to conservative envelope if unreachable.

### Acceptance Criteria
- [ ] Property test in `parko-core/tests/posture_divergence.rs`
- [ ] At least 10,000 cases per PostureState variant (Nominal, Degraded, LockedOut)
- [ ] Asserts `governor_output <= builtin_clamp_ceiling(proposed, state)` for every case
- [ ] `cargo test -p parko-core` exits 0

### Claude Code Prompt

```
You are working in the parko-core crate. PARK-002 must be complete
(set_state_for_test available) before this task can proceed.

Before writing any code, search the workspace for the actual governor crate
and struct names — the governor struct may be named AegisGovernor or similar:

  find parko/ -name "*.toml" | xargs grep -l "\[package\]"
  grep -r "SafetyGovernor\|Governor\|impl.*SafetyGovernor" parko/ --include="*.rs"

If the governor struct is named AegisGovernor or similar, note the rename needed
(KirraGovernor) in a TODO comment but do not rename in this task.
Use Kirra naming in all new comments and docs.

Task: Write a proptest suite asserting governor output is always at least as
conservative as the built-in clamp ceiling.

Requirements:
1. Add proptest = "1" to parko-core dev-dependencies if not present.
2. Create parko-core/tests/posture_divergence.rs.
3. Expose a pub(crate) helper in parko-core/src/control_loop.rs:
     pub(crate) fn builtin_clamp_ceiling(proposed: f64, state: PostureState) -> f64
   This must return the exact ceiling the built-in clamp applies — not a copy.
4. Write three proptest! blocks (one per PostureState variant):
     proptest! {
         #[test]
         fn governor_never_exceeds_builtin_clamp_nominal(
             proposed in -1000.0f64..1000.0f64
         ) {
             prop_assume!(!proposed.is_nan() && !proposed.is_infinite());
             let ceiling = builtin_clamp_ceiling(proposed, PostureState::Nominal);
             let gov_out = /* instantiate KirraGovernor and call enforce */;
             prop_assert!(gov_out <= ceiling,
                 "governor {} > ceiling {} for proposed {}", gov_out, ceiling, proposed);
         }
     }
   Repeat for Degraded and LockedOut.
   For LockedOut: assert gov_out == 0.0 (hard veto).
5. Set cases = 10_000 per block via ProptestConfig or the #[proptest] attribute.
6. Add the Kirra governor crate as dev-dependency in parko-core/Cargo.toml.
   Verify the crate name from workspace Cargo.toml before editing.
7. Run `cargo test -p parko-core -- --test-threads=1` and confirm all pass.
   Do NOT assume any specific test count.
   Do NOT assume the MNIST integration test is passing or relevant here.
8. No unsafe code.
```
