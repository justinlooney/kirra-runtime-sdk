# Occy / KIRRA — Focused Independent Detection Channel (IDC)

**Issue:** #124 — pull the independent detection channel forward (DFA C5/C7).
**Doc ID (proposed):** KIRRA-OCCY-IDC-001.
**Status:** Design spec for review. Closes the DFA's central common-cause hole
(omission of safety-critical objects) with the *minimum sufficient* independent
mechanism, not a full second world model. Sensor/algorithm choices are design
proposals to confirm.

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
| Crossing state (gate/lights/train) | SG5 | map-anchored prior (already) + crossing-signal detector + radar for a fast approaching track-bearing return | primarily map + localization (G2); IDC supplements | med (mostly map-covered) |

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

## 6. Decisions to confirm

- **D1 — dedicated vs shared sensors.** Dedicated safety sensors give the
  strongest independence (clears C7). Recommend dedicated radar (cheap) +
  thermal (worth it for driverless-with-pedestrians); lidar may be shared but
  processed independently.
- **D2 — v1 scope.** For a driverless urban ODD *with pedestrians*, VRU cannot
  be deferred. Recommend v1 = obstacle-in-path + VRU + water-surface; crossing
  via map + G2 + signal-detector supplement.
- **D3 — compute.** IDC on the Governor's independent compute — folds into the
  #114 compute-separation decision (argues for separate compute).

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
