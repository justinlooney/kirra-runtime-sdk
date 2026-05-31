# ADR-0004: Independent Safety Channel — D1–D3 settlement

> **SUPERSEDED by [ADR-0003](0003-two-tier-base-and-d1-addon.md)** — 2026-05-31.
>
> ADR-0003 (two-tier architecture: base downstream Governor + optional D1
> add-on) is the canonical record of the D1 add-on decision and its scope.
> This ADR was filed to formalize the D1–D3 technical settlement (sensor mix,
> v1 scope, compute placement) but is **superseded** by ADR-0003 per owner
> decision; ADR-0003 carries the D1–D3 settlement as part of the two-tier
> architecture record.
>
> The technical detail below — hybrid sensor mix per class, v1 scope decision,
> Governor + D1 as one independent safety channel — remains accurate and is
> retained in `docs/safety/OCCY_INDEPENDENT_DETECTOR.md` §6 (the canonical
> spec location). This file is kept for historical traceability and SHOULD
> NOT be referenced as authoritative; cite ADR-0003 instead.

| Field | Value |
|---|---|
| Status | **Superseded by ADR-0003** |
| Date | 2026-05-31 |
| Deciders | Project owner |
| Issues | #124 (D1 add-on), #114 (S2 DFA), #120 (S8 V&V), #123 (G2 localization), #122 (G1 occlusion) |
| Doc | docs/safety/OCCY_INDEPENDENT_DETECTOR.md (KIRRA-OCCY-IDC-001) §6 |
| Builds on | ADR-0003 (two-tier architecture — base + optional D1 add-on) |

## Context

The DFA (#114) C5 / C7 require an independent, diverse, high-integrity
detection channel for the omission-critical classes. ADR-0003 established
the two-tier architecture (base downstream Governor + optional D1 add-on)
and referenced "the settled D1–D3 spec." This ADR records that technical
settlement: the sensors (D1), the v1 scope (D2), and the compute placement
(D3) for the D1 add-on, plus the framing of Governor + D1 as one
independent safety channel.

D1, D2, D3 are the three settled decisions called out in
`OCCY_INDEPENDENT_DETECTOR.md` §6 — previously "to confirm," now closed.

## Decision

### D1 — Sensor mix (hybrid, dedicated where diversity matters)

| Sensor | Status | Purpose |
|---|---|---|
| Radar | **DEDICATED** | Obstacle in path; moving-VRU micro-Doppler |
| Thermal / IR | **DEDICATED** | Night / stationary VRU — **the BOM cost item, accepted** |
| Lidar | **SHARED, independently processed** | Water-surface anomaly; **not sole-source** for any safety claim |
| Optical / polarization | **DEDICATED** | Water-surface detection, so SG4 is not lidar-common-mode |

Dedicated for the classes where diversity is the entire point of the
add-on; shared (but independently processed) for lidar, with a dedicated
optical/polarization sensor as the diversity partner so water (SG4) isn't
single-sensor.

### D2 — v1 scope

**v1 = obstacle-in-path + VRU + water-surface.** Crossing state **DEFERRED to
v2** — in v1 it's covered by the map-anchored Governor check + G2 (#123). VRU
is non-deferrable for any driverless pedestrian-bearing ODD (per ADR-0001:
all-weather, pedestrian-bearing urban ODD is the deployment scope).

### D3 — Compute placement

**Governor + D1 form one INDEPENDENT SAFETY CHANNEL** on compute separate
from the planner. **Separate SoC preferred; hardware-isolated partition is the
minimum acceptable.** This closes the compute-separation decision tracked on
#114.

The framing matters: Governor + D1 are not two channels living on shared
compute — they are one safety channel whose internal coupling is acceptable
(both run with the same ASIL-D rigor, by the same vendor, against the same
contract) so long as the *whole* safety channel is independent of the
planner.

## Consequences

**Positive:**

- **Closes DFA C7 (shared sensors) for the v1 classes** — radar + thermal +
  optical are independent of any integrator stack.
- **Closes DFA C1 / C2 / C12** (shared compute / power / environment) via
  separate-SoC-preferred placement of the whole safety channel.
- **Thermal closes the night-VRU omission**, which is the highest-severity
  unhandled class for the all-weather pedestrian ODD.
- **Water is not lidar-common-mode** — the dedicated optical/polarization
  sensor partners lidar so SG4 has genuine modality diversity.
- **Resolves the #114 compute-separation decision** — last technical block on
  the DFA closing.

**Negative / risk:**

- **Thermal is the BOM cost driver.** Accepted: closing the night-VRU
  omission is the explicit reason D1 exists; an integrator who can't justify
  the BOM runs the base tier.
- **Water v1 has shared-lidar exposure mitigated, not eliminated** — the
  dedicated optical sensor is the diversity partner, but optical has its own
  failure modes (sun angle, fogging). S8 (#120) characterizes the residual.
- **Crossing leans on map + G2 in v1** — G2 (#123) localization-integrity
  must land for SG5 to hold under the v1 scope. Acceptable: crossings can be
  geofenced out for early deployment if G2 lags.
- **Hardware-isolated-partition fallback** (instead of separate SoC) leaves
  some C1 residual; partition is the *minimum* acceptable, not the strong
  form.

**Alternatives considered:**

- *Shared sensors only:* rejected — C7 stays common-mode, defeats the entire
  point of the D1 add-on. The premium tier collapses.
- *Full second world model (a complete diverse perception stack):*
  rejected — re-creates the ML certification problem (you'd need to verify
  the second stack too), risks homogeneous redundancy (two ML stacks with
  correlated failure modes), enormous scope.
- *D1 on planner compute:* rejected — shares DFA C1, defeats independence,
  makes the safety channel internally coupled with the very element it's
  meant to check.
- *Single-sensor-per-class:* rejected for VRU (thermal alone misses dynamic
  edge cases; radar alone misses stationary VRUs; combined is the point).

## Open follow-up (LAST open decision on #114)

**Planner rigor — DISCIPLINED-QM (recommended).** With D1–D3 settled and
compute separation closed, the only remaining decision on the S2 (#114) DFA
is the QM-planner-rigor call:

- *Strict QM:* cheapest; leans entirely on PO-1 / PO-2.
- ***Disciplined QM (recommended):*** develop the QM planner with more
  process rigor than QM requires (defense-in-depth), without claiming an
  ASIL for it. Low cost, strengthens the assessment.
- *Symmetric B(D)+B(D):* rejected (ML-to-B is infeasible).

On sign-off of disciplined-QM, #114 can close.

## Links

- `docs/safety/OCCY_INDEPENDENT_DETECTOR.md` (KIRRA-OCCY-IDC-001) §6 — D1–D3
  settled per this ADR.
- `docs/safety/OCCY_DFA.md` (KIRRA-OCCY-DFA-001) — C5 / C7 disposition is
  tier-dependent (base: delegated; + D1: closed unilaterally) per ADR-0003;
  C1/C2/C12 closed via D3 compute placement per this ADR.
- `docs/safety/OCCY_ARCHITECTURE_TIERS.md` (KIRRA-OCCY-ARCH-001) — two-tier
  architecture context.
- ADR-0003 — two-tier architecture (base + optional D1 add-on); this ADR is
  the technical settlement that ADR-0003 references.
- ADR-0002 — sub-ODD + condition-dependent cap (the envelope function D1
  feeds as additive healthy coverage).
- Issue #124 — D1 add-on; moves from design to implementation per this ADR.
- Issue #114 — S2 DFA; compute-separation decision resolved here; planner
  rigor is the last open decision.
- Issue #120 — S8 V&V; characterizes D1 per-class detection range + FP / FN
  in worst-case conditions, feeds the speed cap via `range_supported(...)`.
- Issue #123 — G2 localization-integrity coupling; v1 crossing coverage
  depends on this landing.
- Issue #122 — G1 occlusion-aware caution; separate Governor rule, not
  a D1 class.
