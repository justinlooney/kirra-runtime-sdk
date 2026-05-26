# Completed Work

Completed tasks will be appended here weekly.

---

## PARK-001 — Attach `SafetyGovernor` to `ControlLoop`

**Completed:** 2026-05-26 | **Commit:** `10f8c88` | **Branch:** `claude/claude-md-reference-AtTWy`

- `with_governor(impl SafetyGovernor + 'static) -> Self` added to both `InferenceLoop` and `ControlLoop`; governor boxed internally.
- Built-in scalar clamp suppressed when governor is present (ADL-002).
- `test_builtin_clamp_suppressed` and `test_no_governor_uses_builtin_clamp` added.
- Stale Aegis references fixed in runtime.rs and scheduler.rs doc comments.
- 31 tests pass (28 unit + 3 integration). No unsafe code.

---

## PARK-002 — Add test-only posture state setter

**Completed:** 2026-05-26 | **Commit:** `c6bcb0a` | **Branch:** `claude/claude-md-reference-AtTWy`

- `set_state_for_test` gated with `#[cfg(any(test, feature = "test-helpers"))]`.
- `test-helpers` Cargo feature added; absent from release builds (nm confirmed).
- `[[test]] required-features = ["test-helpers"]` for test_posture_divergence target.
- Inline unit test `set_state_for_test_overrides_initial_warmup_state` added.
- 29 unit tests pass; 4 integration tests pass with `--features test-helpers`.

---

## PARK-003 — Write posture divergence property test

**Completed:** 2026-05-26 | **Commit:** TBD | **Branch:** `claude/claude-md-reference-AtTWy`

- Proptest suite in `tests/posture_divergence_proptest.rs`: 4 properties × 10,000 cases each.
- Properties verified: nominal ceiling ≤ 35.0, degraded ceiling ≤ 5.0, locked-out = fallback (5.0), locked-out ≡ degraded.
- Discovered: LockedOut uses MRC fallback profile (same as Degraded), not a hard-veto; nominal profile has stricter rate-of-change limits than fallback.
- proptest = "1" added to dev-dependencies; `*.proptest-regressions` added to .gitignore.
- All 29 unit + 4 proptest tests pass (`cargo test -p parko-core`). No unsafe code.
