# Occy / KIRRA — ASIL Decomposition + Dependent Failure Analysis

**Issue:** S2 (#114) — ASIL decomposition design + DFA (independence).
**Doc ID (proposed):** KIRRA-OCCY-DFA-001.
**Status:** Working analysis for review. The decomposition validity and the
independence claims are safety-assessor judgments; this is a methodologically
sound draft to be confirmed. It establishes *what must be true* for the ASIL-D
claim to hold — and surfaces one finding that changes the build plan.

---

## 1. The decomposition

The architecture is a **safety-monitor (simplex) pattern**, expressed as an
ISO 26262-9 Cl.5 decomposition of each ASIL-D safety goal:

    ASIL D  =  ASIL D(D) [KirraGovernor]  +  QM(D) [Occy planner]

The Governor carries the full ASIL-D integrity; the ML planner is QM with
respect to the safety goals, because any hazardous trajectory it emits is
detected and mitigated by the Governor. This is the only tractable path —
developing an ML planner to ASIL D (or even B) is infeasible, so all safety
integrity is concentrated in the simpler, deterministic, verifiable Governor.

**This is valid only if BOTH proof obligations hold:**

- **PO-1 — Diagnostic coverage (§2):** the Governor detects *every* hazardous
  trajectory class. Any class it cannot catch is uncovered — the planner's QM
  faults in that class are NOT mitigated, and the decomposition fails there.
- **PO-2 — Independence (§3, the DFA):** the QM planner cannot defeat or corrupt
  the ASIL-D Governor, and no common cause disables both. Without independence,
  ISO 26262-9 voids the decomposition and *each* element would have to meet the
  full ASIL D.

**Decision — planner rigor.** D(D)+QM(D) is the natural fit but an assessor will
scrutinize the PO-1 completeness claim. Options:
- (A) Strict QM planner — cheapest; leans entirely on PO-1/PO-2. *Recommended,
  with (B) as hedge.*
- (B) **Disciplined QM** — develop the QM planner with more process rigor than
  QM requires (defense-in-depth) without claiming an ASIL for it. Low cost,
  strengthens the assessment.
- (C) Symmetric B(D)+B(D) — both to ASIL B; conventional but requires ML-to-B
  (hard) and still needs the Governor. Not recommended.

---

## 2. PO-1 — Diagnostic coverage

The Governor must catch every hazardous-trajectory class implied by SG1–SG9:

| Hazard class | Governor check | Covered? |
|---|---|---|
| Longitudinal collision (SG1) | RSS over horizon | yes (lat. pending Ph3) |
| Road/lane departure (SG2) | per-step kinematics + drivable space | yes |
| Dynamic envelope (SG3) | per-step kinematics contract | yes |
| Untraversable water (SG4) | WATER_UNTRAVERSABLE | yes |
| Commit zone (SG5) | map-anchored block | yes (depends on localization — see C6) |
| Post-collision motion (SG6) | impact latch + veto | yes |
| Teleop unsafe command (SG7) | doer-agnostic check | yes |
| MRC reachability (SG8) | standing-MRC + commit-on-fail | yes |
| Fail-closed (SG9) | WCET / NaN / timeout | yes (bound via S3) |
| **Occlusion / limited visibility** | — | **GAP → G1 (#122)** |

**Coverage gaps = uncovered hazards.** G1 (occlusion-aware caution) is a known
hole: until closed, a planner that drives too fast for the available sightline
is NOT caught. PO-1 is only as complete as this list; new hazards (S4 catalog)
must each get a Governor check or be excluded from the ODD.

---

## 3. PO-2 — Dependent Failure Analysis

Coupling factors between doer (Occy) and checker (Governor), per ISO 26262-9 Cl.7:

| # | Coupling factor | Failure if coupled | Required mitigation | Residual |
|---|---|---|---|---|
| C1 | Shared compute (SoC/core) | HW fault / resource exhaustion downs both | Spatial+temporal FFI: MPU-isolated partition on a separate core (min); separate SoC (strong) | low (sep. SoC) / med (partition) |
| C2 | Shared power | common power fault disables both | independent/monitored power to Governor; safe-state on loss | low |
| C3 | Shared memory/state | planner corrupts Governor state | spatial FFI; immutable validated inputs; no shared mutable state | low |
| C4 | Shared scheduling | planner starves Governor → missed deadline | temporal FFI / separate compute; WCET bound (S3) | low (fail-closed) |
| **C5** | **Shared perception / world model (iii)** | **common-mode: a perception error corrupts BOTH plan and check → unsafe trajectory approved** | conservative re-derivation covers UNCERTAINTY only; OMISSION needs an independent (ii) detection channel | **HIGH — see §4** |
| **C6** | **Shared localization (G2)** | localization error misplaces map-anchored checks + plan | localization-confidence gating; combine perception+map; degrade on low confidence (#123) | med until G2 |
| C7 | Shared sensors | sensor fault/spoof hits both | diverse/independent sensing for safety-critical detection (True-Redundancy analog); part of (ii) | high until (ii) |
| C8 | Shared software/libraries | bug in shared code (RSS/math/parser) defeats both | minimize shared code on the safety path; develop Governor path independently to ASIL D; diverse impl where feasible | med |
| C9 | Shared systematic/design (same team/assumptions) | same wrong assumption in both | design/process diversity; independent review; **KIRRA vendor-independence** | low w/ independence |
| C10 | Shared egress/comms | unchecked command bypasses Governor | Governor in-line on actuation egress; no bypass; verify (teleop lesson) | low |
| C11 | Cascading (planner crashes checker) | malformed output crashes Governor | input validation; bounded processing; fail-closed (done: body-bound, NaN-trap) | low |
| C12 | Environmental | common temp/EMI/vibration stress | automotive qual; separate placement if co-located | low |

---

## 4. Central finding — the shared world model (C5/C7)

**This is the finding that changes the plan.** The (iii) conservative-shared
world view is a common-cause input: the Governor re-derives RSS at worst-case
bounds, but it re-derives from *the same perception the planner used*.

The critical distinction: **conservative bounds mitigate UNCERTAINTY, not
OMISSION.**
- *Uncertainty* (a detected-but-imprecise object): widen its bounds — the
  conservative Governor handles this. ✔
- *Omission* (an object/water/VRU never detected at all): you cannot be
  conservative about something you cannot see. The Governor shares the
  planner's blind spot and approves the unsafe trajectory. ✘

Omission is the highest-severity failure mode (the unseen pedestrian, the
undetected water edge), and it is **exactly the class the shared world view does
not cover.** Therefore the independent **(ii) world-state / detection channel
must be pulled forward** from Phase 4 — at least a focused independent detector
for the omission-critical classes (large obstacle, VRU, water boundary, commit
zone) — or the ASIL-D claim has a common-cause hole.

This does not require a *full* diverse world model immediately. The pragmatic
first step is an independent **detection** channel (diverse sensor/algorithm,
True-Redundancy style) for the few omission-critical classes, feeding the
Governor's veto path only. Full (ii) world-state diversity can still mature
later.

---

## 5. Freedom from interference (FFI)

For the Governor to be a valid ASIL-D element it needs:
- **Spatial** — memory protection / partitioning; Governor state not writable by
  the planner; inputs copied and validated.
- **Temporal** — guaranteed execution budget; the planner cannot starve the
  Governor; missed budget → fail-closed (SG9). Strongest via separate compute.
- **Communication** — the Governor sits in-line on the actuation egress; there
  is no path from planner (or teleop) to actuators that bypasses it.

**Decision — compute separation.** Partition-on-shared-SoC (MPU/hypervisor
isolation) is the minimum; **separate SoC** is the strong form and the only one
that fully clears C1/C2/C12. KIRRA's vendor-neutral, independent-runtime thesis
argues for separate compute.

---

## 6. Decisions surfaced (need owner sign-off)

1. **Pull the independent (ii) detection channel forward** — required to close
   C5/C7 for omission failures. Recommended: focused independent detector for
   omission-critical classes now; full (ii) later. *(The central finding.)*
2. **Compute separation level** — isolated partition vs. separate SoC.
3. **Planner rigor** — strict QM vs. disciplined-QM (defense-in-depth).
4. **Close G1 (#122)** to complete PO-1 coverage; **close G2 (#123)** to clear C6.

---

## 7. The independence differentiator

C9 (shared systematic/design faults) is where most architectures are weakest —
Mobileye and Nvidia build checker and planner in-house, same team, same
assumptions. KIRRA's **vendor-independent checker** is the strongest possible
mitigation for C9, and a clean independence argument for C1/C8 as well. The DFA
is precisely the artifact where that independence becomes evidence rather than
a claim — the part competitors hand-wave.

Cross-refs: OCCY_SAFETY_GOALS.md (#113), OCCY_SOTIF.md (#116), S3 WCET (#115),
S8 validation (#120), G1 (#122), G2 (#123). Register as KIRRA-OCCY-DFA-001.
