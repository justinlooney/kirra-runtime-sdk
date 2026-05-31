# Occy / KIRRA — D1 Independent Detection Channel (optional Tier-2 add-on)

**Issue:** #124 — D1 Independent Detection Channel as the Tier-2 add-on (closes DFA C5/C7 unilaterally).
**Doc ID:** KIRRA-OCCY-IDC-001.
**Status:** Design spec — D1–D3 settled. This is the **optional Tier-2 D1
add-on** per ARCH-001 / ADR-0003 (supersedes the earlier "core IDC" framing).
Integrators who run only the base tier (KIRRA Governor downstream of their own
perception) operate at an envelope bounded by their delivered coverage; adding
D1 closes the omission common-cause unilaterally and unlocks the premium
envelope (night VRU, water, larger ODD, the unilateral ASIL-D omission claim).

---

## 1. Purpose & non-goals

**Purpose.** An independent, diverse, high-integrity channel that detects the
*omission-critical* hazard classes the planner's perception could miss, and
feeds **only the Governor's veto path**. It answers narrow safety questions
("is there a thing here the Governor must veto for?"), nothing more.

**Non-goals.** Not a world model. No tracking, prediction, classification beyond
"hazard-class present in the safety-relevant zone," or planner input. It never
proposes a plan — it only forces conservative action. This keeps it a *checker*,
not a second *doer*.

## 2. Principles

1. **Diverse by design** — must fail *differently* than the planner's perception
   (assumed DL-camera-led). Different modality AND different algorithm class
   (classical over ML where feasible). Homogeneous redundancy (a second DL stack)
   does not satisfy ISO 26262 independence — correlated blind spots.
2. **High integrity** — narrow scope keeps it simple/deterministic/bounded, so it
   can credibly be the ASIL-D-grade element the DFA needs. Complexity is the
   enemy of the integrity claim.
3. **Veto-only, fail toward safe** — biased to false-positives (unnecessary
   veto = availability cost) over false-negatives (missed hazard = safety cost).
4. **Union fusion** — the Governor treats a hazard as present if **either** the
   planner world model **or** the IDC detects it. Never the intersection. Any
   channel's positive detection wins.
5. **Independent compute & sensors** — runs on the Governor's independent compute
   (ties the compute-separation decision on #114) and, where feasible, its own
   sensors (closing DFA C7); diverse *processing* of shared sensors is the weaker
   fallback.

## 3. Scoped classes + per-class diversity design

The class list **is** a coverage claim (PO-1): each omission-critical class is
here, or the ODD excludes that hazard.

| Class | Hazard / SG | Independent diverse detector | Why it's diverse | Difficulty |
|---|---|---|---|---|
| Obstacle in path (stopped vehicle, debris) | SG1 | 4D imaging radar + classical occupancy / CFAR free-space check | radar modality + classical algorithm; weather-robust; no shared ML | low–med |
| VRU presence incl. night (pedestrian/cyclist) | SG1, SG6 | thermal/IR + classical hot-blob; radar micro-Doppler for moving VRU | thermal sees body heat where visible camera is weakest (night); classical; radar for motion | **high (the hard one)** |
| Standing water / untraversable surface | SG4 | lidar return-anomaly (specular/absorption "holes") + polarization-camera signature; classical | optical-physics principle, diverse from DL segmentation; flags surface anomaly → untraversable-default (no depth claim) | high |
| Crossing state (gate/lights/train) | SG5 | v1: map-anchored prior + G2 localization. v2: crossing-signal detector + radar for a fast approaching track-bearing return | primarily map + localization (G2) in v1; D1 supplements in v2 | **v2 (deferred — map + G2 cover v1)** |

Notes on the hard ones (be honest about residuals):
- **VRU** is the highest severity and hardest. Thermal closes the night gap but
  has its own failure modes (hot ambient, occlusion); radar Doppler catches
  moving but not stationary VRUs. The combination raises coverage but is not
  perfect — characterize it (S8) and let residual drive the speed cap.
- **Water** depth is unrangeable by anything — the IDC detects the *surface
  anomaly* to trigger the untraversable-default + earn-back (#98), it does not
  measure depth.

## 4. Integration

- IDC emits hazard flags → consumed by the Governor's existing checks (RSS,
  kinematics, WATER_UNTRAVERSABLE, commit-zone), which act on the **union** of
  (planner world model ∪ IDC).
- **Fail-safe:** IDC fault/unavailable → conservative posture (speed derate /
  larger margin), and the loss is itself a degraded-posture trigger — never a
  silent loss of coverage.

## 5. Coverage-boundary clarifications

- **Occlusion (G1) is NOT an IDC class.** You cannot detect what is occluded;
  the mitigation is conservative speed for the available sightline — a Governor
  *caution rule* (G1/#122), not a detector. Keep them separate.
- IDC does presence / free-space / surface-anomaly only. Classification,
  tracking, prediction stay planner-side; the Governor does not need them to
  veto.

## 6. Decisions (settled — see ADR-0003 + ADR-0004)

D1–D3 were "to confirm" before ADR-0003 re-framed the IDC as the optional
Tier-2 D1 add-on. **ADR-0004** records the technical settlement below.

- **D1 — Sensor mix (hybrid, dedicated where diversity matters).** SETTLED:
  - **Radar — DEDICATED** (obstacle in path; moving-VRU micro-Doppler).
  - **Thermal / IR — DEDICATED** (night / stationary VRU — **the BOM cost
    item, accepted** as the price of closing the night-VRU omission, the
    highest-severity class).
  - **Lidar — SHARED, independently processed** (water-surface anomaly); **not
    sole-source** for any safety claim.
  - **Optical / polarization — DEDICATED** for water-surface detection, so
    SG4 isn't lidar-common-mode.

- **D2 — v1 scope.** SETTLED: **v1 = obstacle-in-path + VRU + water-surface**.
  Crossing state DEFERRED to **v2** — in v1 it's covered by the map-anchored
  Governor check + G2 (#123). VRU is non-deferrable for any driverless
  pedestrian-bearing ODD.

- **D3 — Compute.** SETTLED: **Governor + D1 form one INDEPENDENT SAFETY
  CHANNEL** on compute separate from the planner. **Separate SoC preferred;
  hardware-isolated partition is the minimum acceptable.** This closes the
  #114 compute-separation decision.

## 7. Validation (ties S8 / #120)

Each class detector needs its own **detection range + false-positive /
false-negative rate characterized in worst-case conditions** (wet / night /
fog). These feed:
- the speed-cap range assumption (the ~130 m figure → the real worst-case R),
- the look-ahead FTTI (SG4/SG5),
- the IDC's own PO-1 coverage evidence.

Cross-refs: OCCY_DFA.md / #114, OCCY_SAFETY_GOALS.md / #113, OCCY_SOTIF.md +
SPEED_ENVELOPE.md / #116, S8 / #120, G1 / #122, G2 / #123, flood earn-back #98.
Register as KIRRA-OCCY-IDC-001.
