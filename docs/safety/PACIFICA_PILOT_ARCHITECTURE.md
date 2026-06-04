# Kirra — Pacifica AV-Pilot Deployment Architecture (Roadmap Record)

**Document ID:** KIRRA-OCCY-DEPLOY-001.
**Status:** Draft — **roadmap / architecture record, NOT a build commitment.**
**Classification:** Deployment architecture (target-vehicle integration path).
**Cross-refs:** `SAFETY_CASE_INDEX.md` (AEGIS-SC-000), ADR-0004 (independent safety
channel), ADR-0001 (ODD speed cap), KIRRA-OCCY-PMON-001/002/003 (Track-C perception
derate), KIRRA-OCCY-AOU-001 (assumptions-of-use register),
`SAFE_STATE_SPECIFICATION.md` (MRC / Degraded behaviors), `PARKO_OCCY_TOPOLOGY.md`.

> **Why a standalone note, not an ADR.** The ADR series records *decisions taken* (e.g.
> ADR-0001 the cap value, ADR-0004 the doer/checker boundary). This document records a
> *target deployment architecture* and a *solved-vs-build split* for a pilot that is **not
> committed to build now** — there is no decision to ratify here. It is filed as a safety
> deployment note (alongside `PARKO_OCCY_TOPOLOGY.md`) and registered in the safety-case
> index so the integration path lives in the project rather than in chat. If/when the
> pilot is committed, the vehicle-selection and ODD decisions it implies would each become
> their own ADR.

---

## 1. Purpose / scope

Records the **target AV-pilot deployment architecture** — a Chrysler Pacifica Hybrid
research platform — for the Kirra governor: **where Kirra sits, what is already solved
versus what remains to build, and the assumptions the platform surfaces.** It is a
roadmap record to anchor future planning; it commits nothing to build and changes no
source. Enforcement on any vehicle remains gated on the pre-enable gates (§6, §8).

This sits in the **Kirra / Occy / Parko** frame (ADR-0004):
- **Occy** — the Autoware planning/perception stack (the *doer*; Track-A/B capability).
- **Parko** — the governed per-silicon inference runtime.
- **Kirra** — the independent governor (the *checker*) that gates the command stream and
  derates on perception output. Kirra builds no perception; it checks Occy's output.

---

## 2. The stack (top to bottom)

Paraphrased from the cited sources; see each URL for the authoritative detail.

### Vehicle + drive-by-wire — Chrysler Pacifica Hybrid + Dataspeed ADAS Kit
The pilot vehicle is a Pacifica Hybrid fitted with the **Dataspeed ADAS Kit** (the
2017–2021 Hybrid is a supported configuration). The kit provides computer control of
throttle, brake, steering, and shift through by-wire hardware plus power distribution and
a vehicle-network interface. It **preserves factory driver override** (brake / throttle /
shift / steering wrest control back from automation) and offers an **E-Stop accessory**.
Beyond raw pedal-position / steering-angle commands, it exposes **higher-level
speed/acceleration and yaw-rate / turning-radius command interfaces** — i.e. the actuation
abstraction Kirra's gated commands map onto.
- Supplier: https://www.dataspeedinc.com/
- By-wire kit brochure: https://levelfivesupplies.com/wp-content/uploads/2019/05/Dataspeed-drive-by-wire-kit-brochure.pdf

### Vehicle interface — Autoware ↔ Dataspeed bridge (BELIV, ASU)
**The bridge already exists, open-source.** ASU's `BELIV_vehicle_interface` translates
Autoware's Ackermann-style control commands into Dataspeed DBW commands (longitudinal +
lateral) and feeds vehicle status back to Autoware. This is the integration layer below
Kirra's gated output.
- Repo: https://github.com/BELIV-ASU/BELIV_vehicle_interface
- Method paper (ASU/SAE 2024-01-1981): https://saemobilus.sae.org/papers/developing-automated-vehicle-research-platform-integrating-autoware-dataspeed-drive-wire-system-2024-01-1981

### Autoware — perception / planning / control
Autoware produces the three outputs Kirra consumes: **perception → `PredictedObjects`**,
**planning → trajectory**, **control → `control_cmd`**. Autoware's `vehicle_interface`
consumes `/control/command/control_cmd` (and `actuation_cmd`); **Dataspeed and PACMod** are
community-tested DBW suppliers in the Autoware reference matrix.
- Vehicle-interface overview: https://autowarefoundation.github.io/autoware-documentation/main/design/autoware-interfaces/components/vehicle-interface/
- DBW suppliers: https://autowarefoundation.github.io/autoware-documentation/pr-493/reference-hw/vehicle_drive_by_wire_suppliers/

### Compute — Jetson Orin (the bench → vehicle through-line)
The pilot targets **Jetson Orin**, the same silicon class as the scaled bench. This is the
through-line that makes the bench predictive of the vehicle: the **Autoware build, the
`kirra-ros2-adapter` binary, and the WCET characterization transfer** from bench to vehicle
because the compute substrate is the same (the "Jetson lesson" / WCET determinism work in
the Parko track applies directly).

### Sensors (representative)
A representative Pacifica research build (University of Minnesota MnCAV) carries on the
order of **3 lidar + 4 cameras + 2 GPS + front and rear radar**. Exact suite is
deployment-specific; the front/rear radar is load-bearing for §6.
- https://www.cts.umn.edu/news-pubs/news/2021/august/mncav

---

## 3. Where Kirra sits — the command gateway

Kirra is the **command gateway between Autoware's control output and the vehicle
interface**:

```
Autoware (perception → PredictedObjects, planning → trajectory, control → control_cmd)
   │
   ▼
[ KIRRA governor ]
   • validate_vehicle_command  (P0..P6 kinematic envelope; verdict path, unchanged)
   • Track-C perception derate over PredictedObjects  (kinematic ceiling → cap, default OFF)
   • posture gate (Nominal / Degraded decel-to-stop-and-HOLD / LockedOut)
   │  gated command (Allow / ClampLinear / ClampSteering / MRC)
   ▼
BELIV / Dataspeed vehicle interface → DBW → CAN → Pacifica
```

**This maps onto the EXISTING `kirra-ros2-adapter` with no redesign.** That crate already
subscribes `~/input/objects` (`PredictedObjects`), `~/input/trajectory`, and
`~/input/control_cmd`, runs the two-rate Option-B check (`validate_trajectory_slow` /
conformance), composes the Track-C cap (PMON-003 slice-1, dark by default), and is
positioned to emit the gated `control_cmd`. **The Pacifica forces no architectural change**
— it is the same gateway position the adapter already occupies, scaled from sim/bench to a
real vehicle. The output side (publishing the gated `control_cmd` into the BELIV interface)
is the integration point that the vehicle adds (§4, §7).

---

## 4. Solved vs. to-build

### Solved (COTS or already built)
- **Dataspeed DBW** — COTS, Pacifica-supported; computer actuation with preserved override
  + E-Stop.
- **Autoware ↔ Dataspeed bridge** — BELIV, open-source; Ackermann → DBW + status feedback.
- **Autoware stack** — perception / planning / control producing the three Kirra inputs.
- **Kirra's command gateway + Track-C** — built and on `main`, **dark by default**
  (`validate_vehicle_command` + the PMON-001/002/003 derate composition; the adapter
  two-rate wiring).

### To build / integrate
- **Sensor calibration** (intrinsics/extrinsics across the lidar/camera/radar/GPS suite).
- **ODD definition** for the pilot (feeds the ADR-0001 cap and the sub-ODD partition of
  ADR-0002).
- **Safety review / safety case** for vehicle deployment (the GSN argument extended to the
  vehicle; HARA for the platform).
- **Inserting Kirra at the gateway in the real topic graph** (the adapter's gated
  `control_cmd` actually wired between Autoware control and the BELIV interface).
- **WCET characterization on the vehicle Orin** (re-measure the verdict-path + tick-rate
  guard budgets on the deployment silicon; S8 / target-hardware re-validation).
- **Discharging the pre-enable gates** — the AoU(s) in §6 + the PMON-003 validation gate
  before `KIRRA_PERCEPTION_DERATE_ENABLED` is turned on.

### Capital (do not assert a figure)
The Dataspeed kit + Pacifica + sensor suite is a **substantial** capital line. Pricing is
configuration-dependent and must be **quoted directly with Dataspeed**; no figure is
asserted here.

---

## 5. Layered safety (defense in depth)

The pilot is a **multi-barrier** argument, not a single point of trust:

- **Above** — Kirra governs the **AD-level command**: it gates / clamps / denies the
  Autoware command stream by kinematic envelope, posture, and (when enabled) the Track-C
  perception derate. Kirra's MRC / Degraded **decel-to-stop-and-HOLD** dispositions
  (`SAFE_STATE_SPECIFICATION.md` SS-002) are issued as commands.
- **Through** — those MRC / Degraded commands flow **through the DBW** as speed / decel
  (and steering) commands — the Dataspeed higher-level speed/accel interface is exactly the
  channel a controlled stop rides on.
- **Below** — the **Dataspeed layer** preserves **factory driver override + E-Stop +
  the vehicle's own safety systems**, independent of and beneath the AD command path.

For the safety case: **Kirra is one independent barrier among several** (independent
governor above; preserved factory/override/E-Stop below). A failure of the AD command
path (including Kirra itself) is backstopped by the lower layer; a perception/planning
fault is caught by Kirra before it reaches the DBW. This layering is the deployment-level
expression of the ADR-0004 doer/checker separation.

---

## 6. Assumptions of use surfaced by the platform

Cross-referenced to the register (KIRRA-OCCY-AOU-001):

- **AOU-PERCEPTION-FRAME-001 — radar precondition is LIVE here.** The Pacifica fuses
  **front and rear radar**, so `radar_tracks_msgs_converter` **ego-motion compensation must
  be enabled** for radar-sourced object twist to be absolute (map/world-frame). On this
  platform the radar path is the concrete way the frame assumption can silently break; it
  is **load-bearing for this deployment** and part of the pre-enable gate.
- **NEW candidate — flag for the register: AOU-VEHICLE-INTERFACE-TIMING (proposed).** A
  command-latency + status-feedback-freshness assumption: (a) Kirra's gated command must
  reach the DBW within the control-loop timing budget (Kirra's WCET + the BELIV/Dataspeed +
  CAN latency must close the loop inside the FTTI/reaction budget — ties to the ADR-0001
  loop-closure and the S3 WCET chain), and (b) the **vehicle-status feedback**
  (speed / steering / odometry) Kirra relies on for its rate-of-change and current-state
  checks must be **fresh and accurate**. Stale or wrong feedback degrades the verdict the
  same way stale perception does. **This is not yet a register entry — proposed for a
  follow-up AoU PR.**
- **D1 (tracked-not-raw) applies.** The Pacifica perception must feed **Autoware
  tracking/prediction** (the `PredictedObjects` output), not raw detections — raw
  detections would force association inside Kirra (an ADR-0004 boundary violation) and would
  also fail the absolute-velocity assumption.

---

## 7. Validation ladder

Each rung validates more, with the vehicle adding what the bench cannot exercise:

| Rung | Setup | Validates | New vs. the rung below |
|------|-------|-----------|------------------------|
| **Sim** | AWSIM + Autoware | The full topic graph + Kirra gateway logic + Track-C composition end-to-end against simulated perception/planning; the immediate next step. | (baseline) — no real timing or hardware. |
| **Scaled bench** | Autoware-on-Orin, a scaled R2 Ackermann platform, the **real `kirra-ros2-adapter` wiring** | **WCET on real silicon** (the Orin through-line), **physical fail-closed rehearsal** (MRC / Degraded actually stopping a moving platform), real ros2 timing/freshness. | Real compute + real actuation timing + a physical safe-stop. |
| **Pacifica** | Bench stack + **DBW bridge + full sensor suite + ODD + safety review** | The production DBW path (BELIV → Dataspeed → CAN), the real sensor suite + calibration, the deployment ODD, and the vehicle safety case. | **The DBW bridge, the sensors/ODD, and the safety review** — the parts no bench can exercise. |

The bench is predictive of the vehicle for **logic + WCET** (same Orin, same adapter), but
**not** for the DBW bridge, the sensor suite, or the ODD — those are first exercised at the
vehicle and are exactly what the §4 "to-build" list and the §6 AoUs cover.

---

## 8. Status

**Roadmap / architecture record — not a current build commitment.** The immediate next
step remains the **sim-first validation gate** (AWSIM + Autoware). Before Track-C
enforcement is enabled on **any** vehicle, the **pre-enable gates must discharge**:
- **AOU-PERCEPTION-FRAME-001** (incl. the radar ego-motion-compensation precondition,
  verified per deployment — KIRRA-OCCY-AOU-001), and
- the **PMON-003 §D4 validation gate** (known-velocity object → ego-independent twist
  magnitude) + end-to-end freshness verification + a sim/bench validation pass.

`KIRRA_PERCEPTION_DERATE_ENABLED` stays **OFF** until these pass. Nothing in this document
changes that posture; it records the path, not a decision to walk it.
