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
