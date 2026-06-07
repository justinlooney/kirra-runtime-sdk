# GovernorComparator Diversity — CERT-006 (L2)

**Doc ID:** KIRRA-OCCY-COMPARATOR-DIVERSITY-001
**Issue:** CERT-006 (L2)
**Pairs with:** `parko/crates/parko-kirra/src/comparator.rs` + `parko/crates/parko-kirra/src/diverse.rs`

---

> # ⚠️ DRAFT — pending formal safety-engineer review
>
> Same honesty bar as KIRRA-OCCY-ANGULAR-SOTIF-001. This is a
> first-implementation diversity argument: structural diversity
> between primary and shadow is real and reviewable, the
> correctness AGREEMENT property is testable + tested, and the
> DETECTION path is demonstrated. But a "diversity argument" is a
> safety claim a real assessor will scrutinize — treat the numbers
> and the fault-coverage statement as engineering analysis pending
> sign-off, not a validated safety claim.

---

## 1. Why this exists

Before L2 the `GovernorComparator` ran **identical** redundancy —
two `KirraGovernor` instances built from the same `PlatformParams`
and the same `VehicleKinematicsContract`. That catches:

- **Random / transient faults** in one channel (single-event upsets,
  cosmic-ray bit flips, memory corruption affecting one process).
- **State divergence** (caches getting out of sync, internal
  accumulators drifting).

It does **not** catch:

- **Systematic implementation faults**. A logic or numerical bug in
  `KirraGovernor::evaluate` — wrong sign, off-by-one boundary,
  dropped clamp return, wrong-order check, miscomputed
  intermediate — manifests identically in **both** copies. They
  agree on the wrong answer; the comparator passes it through.

L2 / CERT-006 closes this gap by replacing the shadow with a
**structurally diverse** second implementation,
`DiverseKirraGovernor`, that enforces the **same safety properties**
via **deliberately different computation**. A systematic
implementation bug in one is unlikely to manifest identically in the
other.

## 2. The three orthogonal claims

A diversity story has three distinct claims that demand separate
treatment:

| Claim | Nature | How we justify it |
|---|---|---|
| **Correctness (Agreement)** | testable property — diverse impl must produce a verdict equivalent to the primary on every valid input | `actions_diverge(primary_out, diverse_out, ..) == false` checked across a hand-built input sweep in `comparator::diversity_tests::diverse_agrees_with_primary_across_input_regimes` |
| **Diversity** | engineering ARGUMENT, not a test — the two implementations must be structurally different enough that an implementation bug wouldn't appear identically in both | §3 lists the structural differences concretely; you can't unit-test "a hypothetical bug would have been caught" |
| **Detection** | demonstrable property — when one channel IS wrong, the comparator MUST diverge → audit → escalate | `comparator::diversity_tests::comparator_detects_fault_injected_shadow_disagreement` + `sustained_divergence_escalates_to_locked_out` |

Each claim is verified differently. Don't conflate the three.

## 3. Concrete structural differences (the diversity argument)

`DiverseKirraGovernor::evaluate` enforces the same SPEC as
`KirraGovernor::evaluate` but the implementation differs in four
concrete dimensions. The structural differences are listed by
property so a safety reviewer can scan them in parallel with the
primary's code.

### 3.1 Verdict composition

| Primary (`KirraGovernor`) | Diverse (`DiverseKirraGovernor`) |
|---|---|
| Early-return cascade: `if LockedOut return Deny`, `if !rss return mrc(..)`, `match posture { ... }`, then per-axis enforcement, with nested `match` to combine | Single dispatch `match (posture, rss_safe)` then per-axis verdicts collected into a `AxisVerdict { Allow / Clamp / Deny }` intermediate, then a flat 4-way tuple-match composes the final `EnforcementAction` |
| Result built inline at the failing-check site | Result built ONCE at the end |

A dropped `return` in the primary would let a check fall through
to the next priority; in the diverse impl that same bug class
would show up as an accumulated wrong `AxisVerdict::Allow`. The
two artefacts are structurally different.

### 3.2 Linear axis — not delegated

| Primary | Diverse |
|---|---|
| Calls `kirra_runtime_sdk::gateway::kinematics_contract::validate_vehicle_command` (the SDK's P0–P6 priority pipeline) | Reimplements the P0–P4 checks **inline** in `diverse_linear_check`, **does not** call `validate_vehicle_command` |
| `effective_max_speed` via SDK helper (`match` on `Option<f64>`) | Inline: `self.contract.max_speed_mps.min(self.contract.odd_speed_cap_mps.unwrap_or(f64::INFINITY))` — two `f64::min` calls, no `Option` `match` |
| NaN guard via `!cmd.linear_velocity.is_finite()` | NaN guard via `proposed.is_nan() \|\| proposed.is_infinite()` — two predicates, not the negated single predicate |
| Acceleration check: divisive form `implied_accel = dv/dt; if implied_accel > max_accel + ε` | Acceleration check: **multiplicative form** `if dv >= a_max * dt + ε` — no division on the hot path |
| Clamp value: `contract.max_speed_mps * v.signum()` | Clamp value: `v_max.copysign(v_proposed)` — different sign-handling idiom |
| Skips P5 / P6 by design (steering_deg always 0 in the bridge) but the SDK still runs the bicycle-model arithmetic | Skips P5 / P6 **explicitly** — does not run the bicycle-model arithmetic at all |
| Returns `EnforceAction::DenyBreach(DenyCode)` (typed enum mapped to a string) | Returns `AxisVerdict::Deny(String)` directly — different error-carrier shape |

An implementation bug in the SDK's `validate_vehicle_command` would
cascade through the primary; the diverse impl would not invoke that
code at all.

### 3.3 Angular axis — inline `ω_max`, rearranged algebra

| Primary | Diverse |
|---|---|
| Calls `AngularVelocityBound::omega_max(v)` (chained `min().min()` fold in the kernel) | Recomputes `ω_max(v)` INLINE in `diverse_omega_max` via `[r, s, f].iter().fold(f64::INFINITY, f64::min)` |
| Rollover: `g * params.track_width_m / (2.0 * params.cog_height_m * v)` | Rollover: `(G / v_abs) * (params.track_width_m / (2.0 * params.cog_height_m))` — same value, different floating-point intermediate |
| Sweep: `v_edge_safe / r_extent` | Sweep: `v_edge_safe * r_extent.recip()` |
| v=0 floor: `if v >= ROLLOVER_MIN { compute rollover } else { ∞ }` (guard the formula) | v=0 floor: `if v < ROLLOVER_FLOOR { ∞ } else { compute }` (mask the formula) |
| References the kernel constant `ROLLOVER_MIN_LINEAR_VELOCITY_MPS` | Duplicates the constant value (`0.05`) so the diverse code is not coupled to the kernel constant |

A bug in `AngularVelocityBound::omega_max` is not on the diverse
path at all. Same numeric answer; independent code.

### 3.4 MRC profile — different decomposition

| Primary | Diverse |
|---|---|
| `KirraGovernor::apply_mrc_profile` is one function with both axes interleaved (linear-clamped, angular-clamped, single tuple-match at the end) | `DiverseKirraGovernor::diverse_mrc` runs linear and angular as two independent helpers + a separate tuple-match composition |
| Linear cap: `proposed.linear_velocity.min(MRC_VELOCITY_CEILING_MPS)` (single `.min` call) | Linear cap: `if proposed.linear_velocity > MRC_VELOCITY_CEILING_MPS { Some(MRC_VELOCITY_CEILING_MPS) } else { None }` (explicit `if`) |

## 4. The fault class this diversity covers

**Covered (this is the value-add over identical redundancy):**

| Fault class | Why covered |
|---|---|
| Wrong-sign arithmetic in one impl | Unlikely to appear identically in two implementations written with different sign-handling idioms (`signum * abs` vs `copysign`) |
| Off-by-one boundary error | Different comparison directions (`v >= FLOOR` vs `v < FLOOR`) surface boundary disagreements |
| Wrong-priority check ordering | Primary uses sequential early-returns; diverse uses per-axis accumulator + single composition — a misordered check in one is visible in the other |
| Dropped `return` / fall-through | Single-composition style in the diverse path doesn't have early-returns to drop |
| Bug in `validate_vehicle_command` (kernel SDK) | Diverse path doesn't call the SDK function — bug stays in the primary's effective verdict only |
| Bug in `AngularVelocityBound::omega_max` (kernel SDK) | Diverse path doesn't call the kernel helper — bug stays in the primary's effective verdict only |
| Floating-point intermediate rounding cascade | Different algebra forms (divisive vs multiplicative; pre-grouping vs post-grouping) produce different intermediate rounding — a value-sensitive bug in one impl appears as a divergence in the other |
| Wrong clamp-value computation | Two different clamp idioms (`v_max.signum()*v` vs `v_max.copysign(v)`) require independent correctness |

**NOT covered (the explicit limit):**

| Fault class | Why NOT covered |
|---|---|
| **Spec-level systematic faults** | Both implementations enforce the SAME spec (same `VehicleKinematicsContract`, same `PlatformParams`, same `MRC_VELOCITY_CEILING_MPS = 5.0`, same `URBAN_ODD_SPEED_CAP_MPS = 22.35`). If the spec says "clamp at X" and the real-world correct answer is Y, both governors dutifully clamp at the wrong X. The comparator agrees and passes wrongly. **Spec-level coverage requires the full N-version step** (clean-room reimplementation from the spec only, not the primary's code) — out of scope for L2. |
| **Shared upstream bug in `parko-core`** | Both governors take `&ControlCommand` from parko-core's pipeline + return `EnforcementAction`. A parko-core bug that mis-stamps the command, or a `SafetyPosture` propagation error upstream, affects both equally. Out of scope for the governor-level diversity argument. |
| **Identical environmental fault** | A pathological floating-point platform (`f64` rounding behaviour that differs from IEEE 754) would corrupt both governors. We assume IEEE 754 conformance. |
| **A correlated cosmic-ray-class fault** | Two processes on the same CPU sharing the same scheduler / cache lines / branch predictor are not perfectly independent. Identical redundancy's "two channels" guarantee was already weak here; diversity narrows it further but doesn't eliminate it. Process-level / physical-channel isolation is a separate architectural concern. |

**This is the honest limit of L2.** A safety reviewer should treat
the diversity as covering "implementation-level systematic faults
that depend on the specific algebra / control flow of one of the two
implementations". Anything broader (spec faults, upstream faults,
correlated environmental faults) requires additional architectural
mitigation.

## 5. What N-version (Option B) would add

A full N-version implementation would clean-room reimplement the
shadow from the **spec only**, with no access to the primary's
code. That would:

- **Cover spec-level systematic faults** to the extent the
  independent author interpreted ambiguities differently.
- Force the spec to be precise enough to admit a clean-room
  implementation (a useful forcing function on the safety case).

Cost: a separate person / team, weeks of effort, and a higher
maintenance burden (every spec change touches two implementations
written from different baselines).

L2's Option A is a **first defensible step**; Option B is the
stronger-but-later option for a later phase.

## 6. Tests

### 6.1 Correctness (AGREEMENT — the critical one)

`parko/crates/parko-kirra/src/comparator.rs::diversity_tests`:

- `diverse_agrees_with_primary_across_input_regimes` —
  **30 hand-built inputs** spanning:
  - Tiny in-envelope under Nominal (Allow expected, both signs)
  - Above the conservative-default angular bound (~0.2 rad/s) —
    `ClampAngularVelocity`, both signs
  - Above the linear envelope — `ClampLinearVelocity`, both signs
  - Both axes excess — `ClampMotion`
  - Degraded posture (MRC binds on each axis)
  - LockedOut posture (Deny on every command)

  All 30 must satisfy
  `actions_diverge(primary_out, diverse_out, proposed, COMPARATOR_TOLERANCE) == false`.
  A diverse impl that introduces a false divergence on a valid
  input would fail this test. **Regression-guard for the SAME-SPEC
  invariant.**

- `diverse_agrees_with_primary_on_fail_closed_paths` —
  **6 fail-closed paths** (NaN on linear / angular, ±Inf, zero dt,
  negative dt). Both governors must reach an equivalent Deny.

### 6.2 Detection (DEMONSTRABLE)

- `comparator_detects_fault_injected_shadow_disagreement` —
  pair the `KirraGovernor` primary with an `AllowAlwaysFaulty`
  shadow stub (a `SafetyGovernor` that always returns `Allow`).
  Send a command the primary clamps (50 m/s, prev=0). Assertions:
  - The reconciled action's effective linear velocity is strictly
    less than the proposed 50 m/s (proves the comparator did not
    pass the faulty `Allow` through).
  - The `InMemoryDivergenceSink` recorded at least one
    `DivergenceEvent`.

- `sustained_divergence_escalates_to_locked_out` — repeat the
  divergent tick 200 times. At least one divergence event recorded;
  the leaky-bucket accumulator + escalation logic kicks in.

### 6.3 Pre-existing tests still pass

All 8 pre-L2 comparator tests + the 24 `KirraGovernor` tests + the
9 `AngularVelocityBound` tests continue to pass. The `Box<dyn>`
field-type change is API-compatible at the constructor surface — no
external caller needed an update.

## 7. Implementation reference

| Concern | File:line |
|---|---|
| `DiverseKirraGovernor` struct + `evaluate` | `parko/crates/parko-kirra/src/diverse.rs` |
| `diverse_linear_check` (inline P0–P4) | `diverse.rs::diverse_linear_check` |
| `diverse_omega_max` (inline ω_max with rearranged algebra) | `diverse.rs::diverse_omega_max` |
| `diverse_mrc` | `diverse.rs::diverse_mrc` |
| `RssAware` trait + impls for both governors | `parko/crates/parko-kirra/src/comparator.rs::RssAware` |
| `GovernorComparator::new_diverse` | `comparator.rs` |
| `GovernorComparator::with_diverse_shadow_and_sink` | `comparator.rs` |
| AGREEMENT + DETECTION tests | `comparator.rs::diversity_tests` |

## 8. Open items for safety-engineer review

1. **Coverage adequacy of the AGREEMENT input set.** 30 hand-built
   inputs is a reasonable first sweep but a property-based test
   (proptest) over a wider input space would strengthen the
   correctness claim. Filed as a follow-up.
2. **N-version reimplementation** of the shadow from spec only
   (Option B). Required if spec-level systematic-fault coverage
   becomes a target.
3. **Process / physical channel isolation.** The comparator runs
   primary + shadow in the same process; correlated environmental
   faults are not addressed by the diversity argument.
4. **Audit-sink wiring.** The `InMemoryDivergenceSink` is fine for
   tests; production deployments must wire a sink that persists
   `DivergenceEvent` into the hash-chained, Ed25519-signed audit
   ledger (`AuditChainLinker::append_audit_event_tx` in
   `kirra-runtime-sdk`). Tracked.
5. **Diversity-vs-throughput.** The diverse shadow doubles the
   per-tick computation; the existing comparator was designed for
   that overhead. Confirm the WCET impact has not regressed.

## 9. Document control

| Field | Value |
|---|---|
| Issue | CERT-006 (L2) |
| Status | **DRAFT — pending formal safety-engineer review** |
| Author | engineering analysis (L2 implementation branch) |
| Cross-refs | `docs/safety/ANGULAR_VELOCITY_SOTIF.md` (companion DRAFT), `parko/crates/parko-kirra/src/comparator.rs` (audit + escalation logic) |
| Code | `parko/crates/parko-kirra/src/diverse.rs` + `comparator.rs` |
| Tests | `comparator::diversity_tests`, `diverse::tests` |

When this document earns a safety-engineer sign-off, remove the
DRAFT banner at the top, change "Status" to "Reviewed", and add the
reviewer + date in §9.
