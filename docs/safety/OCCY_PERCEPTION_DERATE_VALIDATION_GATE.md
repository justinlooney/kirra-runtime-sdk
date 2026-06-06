# Occy / KIRRA — Perception-Derate Validation Gate (Sim Tier)

**Doc ID (proposed):** KIRRA-OCCY-PMON-004.
**Status:** Decided design for review — the **sim tier** of the validation gate that earns
flipping `KIRRA_PERCEPTION_DERATE_ENABLED` from OFF to ON. Operationalizes the
**PMON-003 §D4** validation gate. No code in this commit; this is the spec the gate
implementation builds against.
**Why this doc ID:** continues the **PMON sequence** (PMON-001 guard → PMON-002 composition
→ PMON-003 ingest → **PMON-004 the validation gate**); it is the validation tier *of the
same Track-C mechanism line* and discharges PMON-003 §D4, so it sits beside the PMON docs
+ AoU register + safety case it feeds (rather than `docs/testing/`, where
`CARLA_SCENARIO_SUITE.md` is a broad scenario catalog — a different kind of doc). Registered
in `SAFETY_CASE_INDEX.md`.
**Base:** verified against `main @ 6a24bb4`.
**References:** KIRRA-OCCY-PMON-001/002/003, KIRRA-OCCY-AOU-001 (AOU-PERCEPTION-FRAME-001),
KIRRA-OCCY-DEPLOY-001 (bench/vehicle tiers), ADR-0004.

---

## 0. The boundary (why this gate exists, and what it must not become)

The Track-C perception derate is on `main` but **DARK** —
`KIRRA_PERCEPTION_DERATE_ENABLED` defaults OFF
(`crates/kirra-ros2-adapter/src/perception_ingest.rs:35-41`, default-off `:243`). Two
structural facts make this gate load-bearing:

1. **The enforcing wiring is CI-unreachable.** `parse_predicted_objects` (r2r decode,
   `crates/kirra-ros2-adapter/src/parsing.rs`, `#![cfg(feature="ros2")]`) and the node
   slow-loop tick (`crates/kirra-ros2-adapter/src/node.rs:364-380`) are `ros2`-gated and
   built by **no automated CI job**. The pure middle —
   `perceived_to_tracked` / `publish_perception_tick` / `resolve_perception_cap` /
   `apply_perception_cap` / `validate_trajectory_slow_capped`
   (`perception_ingest.rs:55,73`) — **is** CI-tested. **This gate is the only thing that
   exercises the gated wiring end-to-end.**
2. **Enabling requires discharging recorded pre-enable gates:** AOU-PERCEPTION-FRAME-001
   (OPEN — `ASSUMPTIONS_OF_USE.md`), PMON-003 §D4, and end-to-end freshness.

**Governor boundary (ADR-0004):** the gate's harness **drives perception input and observes
the gated command output**. It adds **no tracker, associator, or detector** — building
perception in the harness would defeat the independence the gate is meant to validate.

---

## 1. Tiered structure (sim → bench → vehicle) — DECIDED

| Tier | Discharges | CANNOT close |
|------|-----------|--------------|
| **SIM (this doc)** | The **gated wiring** (parse + node tick — the CI-unreachable path), the **general frame convention**, and **freshness**. Earns `KIRRA_PERCEPTION_DERATE_ENABLED` **in sim only**. | Target-deployment config; real-compute timing. |
| **BENCH** (later; KIRRA-OCCY-DEPLOY-001) | Real-Orin perception-tick **WCET** (credible budget), real ros2 timing/jitter, **physical fail-closed rehearsal** (MRC stop; Degraded decel). | The vehicle's DBW bridge, full sensors, ODD, deployment radar config. |
| **VEHICLE** (later, Pacifica) | AOU-PERCEPTION-FRAME-001 **deployment part** (radar ego-motion compensation on the target config), **AOU-VEHICLE-INTERFACE-TIMING**, real sensors/ODD, the **safety review**. Earns enabling **on the vehicle**. | (terminal) |

**Stated plainly: enabling in sim ≠ enabling on a vehicle.** The single env flag means
"enabled *in this deployment*"; a vehicle deployment re-runs its own (vehicle) tier.

---

## 2. The sim-tier split — DECIDED (mechanism-first)

The sim tier is split into two sub-gates:

### Sub-gate 1 — MECHANISM (design + build NOW)
A **synthetic `PredictedObjects` publisher** → the `ros2` adapter harness → scenarios
**(b)–(e)**. This is what **first exercises the CI-unreachable `parse_predicted_objects` +
node slow-loop tick**. **Buildable now, no AWSIM dependency.**

> **STATUS: Layer-2 automated tests PASS on ROS 2 Jazzy (decode boundary,
> 2026-06-04).** The full node-tick over live DDS (`run_full_node_integration`) is
> now an **automated `#[ignore]` test** (run via `-- --ignored` on a ROS-sourced
> dev box) rather than a manual launch recipe: it spawns `run_adapter` over real
> DDS, drives each scenario's inputs, and asserts the slow-loop `TrajectoryVerdict`
> (read from the shared `AdaptorState`) — plausible→`Accept`, implausible/stale→
> `Clamp`, and every scenario→`Accept` with the derate OFF (negative control). It
> proves the derate **mechanism** is live through the node; it does **not** pin the
> exact graded m/s cap (the verdict is coarse — that stays the Layer-1 assertion),
> and it says **nothing** about real-Autoware frame correctness. Sub-gate 1 is now
> **discharged** (both the decode boundary and the node-tick are executable tests);
> sub-gate 2 is **pending**, AOU-PERCEPTION-FRAME-001 stays **OPEN**, and the flag
> stays **OFF**.

### Sub-gate 2 — FRAME (deferred, gated on AWSIM)
Scenario **(a)** — **real Autoware perception on AWSIM** (NOT AWSIM ground-truth objects:
that bypasses the tracker and the very frame question). Its own subsequent step.
**Specification deferred to a follow-on design** (see §5 flag) — this doc fixes the
*criterion* and the *environment requirement*, not the AWSIM stand-up.

### Gating logic
- **AOU-PERCEPTION-FRAME-001 stays OPEN until sub-gate 2 passes.**
- `KIRRA_PERCEPTION_DERATE_ENABLED` stays **OFF until BOTH sub-gates pass** — and then
  "enabled **in sim**" only.
- **Rationale (recorded):** fail-fast on the cheap, CI-unreachable wiring before standing up
  the heavy AWSIM environment. Splitting costs nothing — the flag can't flip until *both*
  pass, and the AoU is OPEN either way — so doing the mechanism first strictly de-risks.

---

## 3. Scenario set + measurable pass criteria

Contract in use by the adapter publisher: `KinematicPlausibilityContract::urban_reference()`
(`perception_monitor.rs`) — **nominal cap = `URBAN_ODD_SPEED_CAP_MPS` = 22.35 m/s**, **MRC
floor = 0.0**, **ceiling `V_OBJECT_MAX_MPS` = 60 m/s** (`node.rs:346` constructs the
publisher with it).

| # | Scenario | Sub-gate / env | Measurable pass criterion |
|---|----------|----------------|---------------------------|
| (a) | **Frame / absolute-velocity** | 2 / AWSIM | Known ground-truth object velocity `v_gt`; reported twist magnitude `sqrt(vx²+vy²) ≈ v_gt` within tolerance **AND ego-independent** — sweep ego speed, the reported object speed does **not** shift. (= AOU general-convention discharge.) |
| (b) | **Plausible → no derate** | 1 / synthetic | Object speed `< 60`: published cap = nominal (22.35); gated command **unchanged vs the disabled baseline**. |
| (c) | **Implausibly-fast → ceiling derate** | 1 / synthetic | See **§3.1** — split into (c1) single-object → MRC floor and (c2) mixed → graded cap, with confirmed values. |
| (d) | **Silent stream → MRC stop** | 1 / synthetic | Stop publishing `~/input/objects`; after `ttl` the resolved cap = `0.0`; gated command → controlled stop; `kinematics_sim` shows velocity → 0. |
| (e) | **Disabled → no-op** | 1 / synthetic | `KIRRA_PERCEPTION_DERATE_ENABLED` unset: resolver `None`; gated command **byte-identical** to the no-perception baseline (verdict path unaffected). |

### 3.1 Scenario (c) — confirmed against the guard's kinematic step table

**Correctness finding (record it):** the kinematic derate is keyed on the **implausible
fraction** over the snapshot, mapped through the monotone step table
(`perception_monitor.rs:80-85`):

```
(0.00 → 1.00), (0.10 → 0.75), (0.25 → 0.50), (0.50 → 0.25),  fraction > 0.50 → MRC floor
```

So a **single implausible object** has fraction `1/1 = 1.0 > 0.50` → **MRC floor (0.0)** —
**not** a graded cap. A one-object "fast object" scenario therefore yields `ClampLinear(0.0)`
(a stop), which is **indistinguishable from scenario (d)**. To exercise a *graded, non-zero*
ceiling derate the snapshot must be **mixed**. Hence (c) is two cases:

| Case | Snapshot | Fraction | Factor | Expected published cap | Expected gated clamp |
|------|----------|----------|--------|------------------------|----------------------|
| **(c1)** single implausible | 1 object, speed > 60 | 1.0 | (tail) | **0.0** (MRC floor) | `ClampLinear(0.0)` → stop |
| **(c2)** mixed (1 of 10) | 10 objects, 1 over 60 | 0.10 | 0.75 | **0.75 × 22.35 = 16.7625 m/s** | `ClampLinear(16.7625)` (when below the config envelope) |

(Optional extra bins for completeness: 2/10 → 0.50×22.35 = 11.175; 4/10 → 0.25×22.35 =
5.5875; 6/10 → MRC floor 0.0.) The **observed gated clamp = the published cap** provided the
integrator `VehicleConfig` envelope (`config.to_kinematics_contract()`) is above it — the
perception cap is `min`'d into `odd_speed_cap_mps` and the most-conservative bound wins.

**Pass criterion (c):** the published cap equals the table value for the constructed
fraction (c1 = 0.0, c2 = 16.7625), and the emitted `control_cmd` is clamped to that cap
(within tolerance). This both confirms the ceiling fires *and* documents that a single
implausible object is, by design, a stop.

---

## 4. Harness (sub-gate 1)

A `ros2`-feature integration test / launch that feeds
`autoware_perception_msgs/PredictedObjects` (+ a trajectory on `~/input/trajectory`) into
`run_adapter`, capturing the emitted gated `control_cmd` and (optionally) the published cap.

**Reuse (pattern only) / build (fresh):**
- **Reuse the #159 negative-control rigor** (`tests/governor_closes_loop_proof.rs:9-13,197-289`):
  every governed assertion paired with a disabled/baseline control — the *delta* is the
  evidence (scenarios (b) and (e) are explicit on-vs-off deltas).
- **Reuse `kinematics_sim`** (`src/kinematics_sim.rs:128 step`, `:206 apply_enforcement`,
  `:269 run_simulation`) to integrate the gated command into vehicle motion — needed to show
  scenario (d)'s velocity → 0.
- **Reuse `scenario_runner`** (`src/scenario_runner.rs:164,207-214`) for the timed
  drive-then-go-silent timeline (scenario (d)).
- **BUILD the adapter-path harness fresh.** #159 drives the **verifier-service**
  (`enforce_actuator_safety_envelope`), **not** the adapter; the gated parse + node tick
  (`node.rs:364-380`) are a different path and need their own harness.

**Freshness mechanics the harness drives** (scenario d): each batch stamps `touch_objects`
→ `last_objects_ms` (`state.rs:444-445`); the slow loop publishes the cap stamped with the
objects' timestamp when fresh, else `sweep_staleness` (`node.rs:364-372`);
`resolve_perception_cap` maps stale/None/poison → `0.0`. Stop publishing → after `ttl` the
gated command falls to a controlled stop. The adapter already logs "objects subscription
stream closed" (`node.rs:300`).

**Governor boundary:** drive `PredictedObjects` **in**, observe `control_cmd` **out** — no
perception is built inside the harness.

---

## 5. Exit criteria per pre-enable item (sim discharges vs. remains)

| Pre-enable item | SIM discharges | REMAINS |
|-----------------|----------------|---------|
| **AOU-PERCEPTION-FRAME-001** | The **general convention** (scenario (a), sub-gate 2). | The **target-deployment radar config** (Pacifica `radar_tracks_msgs_converter` ego-motion compensation; real sensor frames) → **VEHICLE**. |
| **PMON-003 §D4** | The frame (a) + mechanism (b)–(e) **in sim**. | Re-run on the **target** Autoware version/config → **VEHICLE**. |
| **Freshness** | **Fully** (scenario (d): `touch_objects`→sweep→MRC). | Real-timing-under-load → **BENCH**. |
| **WCET (perception-tick)** | Reuse the existing CI **bounded-shape / no-regression** gate (`GOVERNOR_PERCEPTION_GUARD_WCET_CI_THRESHOLD_MICROS`, `src/wcet_gate.rs:141`). | The **certified** perception-tick budget on the deployment **Orin** → **BENCH** (the S3/S8 CI-relative-now / target-measured-later pattern). |

**Both sub-gates passing earns enabling `KIRRA_PERCEPTION_DERATE_ENABLED` in the sim
deployment** and records partial discharge of AOU-PERCEPTION-FRAME-001 (general convention)
+ PMON-003 §D4 (in sim). Nothing here authorizes a vehicle.

---

## 6. Flagged sub-decisions (options + lean; not decided)

1. **Synthetic-publisher mechanism (sub-gate 1):** a small `ros2`-feature **test node** that
   publishes hand-built `PredictedObjects` **vs** a **rosbag** of recorded/hand-built
   messages replayed into the adapter. **Lean: a test node** (programmatic, parameterizable
   per scenario — trivial to vary object count/speed for (b)/(c1)/(c2)/(d); a rosbag is
   better for *capturing real* Autoware output, which belongs to sub-gate 2). **Flagged.**
2. **Scenario-(a) tolerance:** the `≈ v_gt` band and the ego-sweep range. **Lean:** a tight
   relative tolerance (e.g. a few %) over a representative ego-speed sweep; set with the
   AWSIM stand-up (sub-gate 2). **Flagged — deferred to sub-gate 2's design.**
3. **Scenario-(c) expected cap:** **confirmed in §3.1** against the table (c1 = 0.0;
   c2 = 16.7625 for 1/10). Flagged only insofar as the *snapshot composition* (which mix to
   assert) is an implementer choice — recommend at least c1 + one graded bin (c2).
4. **Sub-gate 2 (AWSIM frame) specification:** **in this doc or a follow-on?** **Lean:
   follow-on** — this doc fixes the criterion (scenario (a)) + the environment requirement
   (real Autoware perception on AWSIM, not ground-truth); the AWSIM/Autoware stand-up,
   tolerance (flag 2), and rosbag capture are their own design once sub-gate 1 lands.
   **Flagged.**

---

## 7. Decisions — resolved

| ID | Decision | Rationale |
|----|----------|-----------|
| Tiering | sim → bench → vehicle, each with explicit discharge + what it cannot close. | Honest sim-vs-real boundary; enabling in sim ≠ on a vehicle. |
| Sim split | **Mechanism-first** (sub-gate 1 synthetic, now) + **Frame** (sub-gate 2 AWSIM, deferred). | Fail-fast on the CI-unreachable wiring before the heavy AWSIM stand-up; splitting costs nothing (can't enable until both pass; AoU OPEN regardless). |
| Scenario (c) | Two cases — single → MRC floor (0.0), mixed → graded cap (1/10 → 16.7625). | The step table maps fraction 1.0 (single object) to the MRC tail; a graded cap needs a mixed snapshot. |
| Harness | Fresh adapter-path harness; reuse #159 negative-control rigor + `kinematics_sim` + `scenario_runner`. | #159 drives the verifier-service, not the adapter; the gated parse+tick need their own harness. |
| WCET | Sim = bounded-shape/no-regression (reuse CI); certified budget → bench. | Matches the established CI-relative-now / target-measured-later (S3/S8) pattern. |

**Governor boundary:** the gate drives perception input and observes the gated command
output — no tracker/associator/detector. Building perception in the harness would defeat the
independence it validates.

---

## 8. Sub-gate 1 — Execution Record (2026-06-04, ROS 2 Jazzy)

Attested bench run (not re-runnable in CI — there is no ros2 CI job; recorded here as a
dated validation-log entry).

### Environment
- **Date:** 2026-06-04.
- **Host:** Ubuntu 24.04 (Noble) + ROS 2 Jazzy; rustup stable; `r2r = "=0.9.5"`.
- **Build:** `cargo build/test -p kirra-ros2-adapter --features ros2` on branch
  `feat/ros2-feature-decouple-lanelet2` (PR #186, merged) — i.e. **WITHOUT** the `lanelet2`
  feature, so no lanelet2 C++ corridor bridge in the build (the perception-governance path
  only). PR #186's feature decouple is what made this build possible on Jazzy.

### Result — `cargo test -p kirra-ros2-adapter --features ros2`
- `tests/perception_mechanism_gate_ros2.rs` (**Layer 2**): **2 passed, 1 ignored**
  - `parse_predicted_objects_roundtrips_all_scenarios` … **ok**
  - `decoded_objects_produce_expected_caps` … **ok**
  - `run_full_node_integration` … **`#[ignore]`** — now an **automated live-DDS
    harness** (spawns `run_adapter`, drives inputs, asserts the slow-loop
    `TrajectoryVerdict` from shared state); run via `-- --ignored` on a ROS-sourced
    dev box. Its dated green result on the dev box is recorded by the operator run.
- `tests/perception_mechanism_gate.rs` (Layer 1): **8 passed**.
- `tests/validation_tests.rs`: **14 passed**; lib unit + conformance suites: green.

### DISCHARGED (be precise — do not overclaim)
The **sub-gate-1 mechanism + decode boundary, in real ROS 2.** The two passing Layer-2
tests prove that `parse_predicted_objects` correctly decodes a **real r2r-generated**
`autoware_perception_msgs::PredictedObjects` into the adapter's `PerceivedObject`, and that
the **decoded** objects drive the expected caps. This is the boundary Layer 1 (which starts
from hand-built fixtures) structurally cannot reach: it confirms the CI-unreachable r2r
decode path against a genuine message.

### NOT discharged (unchanged status — stays as below)
- **FRAME convention** — the test twists are **synthetic / chosen**, not real Autoware
  output, so this proves the *mechanism + decode*, **not** that real Autoware emits absolute
  map/world-frame twist. **AOU-PERCEPTION-FRAME-001 stays OPEN.**
- **`KIRRA_PERCEPTION_DERATE_ENABLED` stays OFF** — sub-gate 2 is still pending.
- **`run_full_node_integration`** is now an **automated `#[ignore]` test** (no longer a
  manual recipe), but its assertion is **verdict-level** — `TrajectoryVerdict` from the
  shared `AdaptorState` (plausible→`Accept`, implausible/stale→`Clamp`) — because the node
  emits no output topic yet (Phase 4). It does **not** pin the exact graded m/s cap (0.0 vs
  16.7625); that stays the Layer-1 assertion. The mechanism-vs-frame caveat below is unchanged.
- **Sub-gate 2** (frame confirm on AWSIM + real Autoware perception, GPU host) is
  **unchanged / pending**.

### Environment constraints observed (new — recorded for the bench/vehicle tiers)
1. **r2r 0.9.5 cannot codegen Jazzy's full `autoware_planning_msgs`.** Its binding
   generator (`r2r_msg_gen`) panics on the route messages (`LaneletPrimitive`, the
   `ClearRoute` service) and on `autoware_common_msgs/ResponseStatus`, and a single
   un-generatable type aborts the **entire** binding run — including the `Trajectory` the
   adapter needs. r2r's `IDL_PACKAGE_FILTER` is include-only with no nested-dependency
   resolution, so it cannot exclude a bad message inside a needed package.
   - **Workaround used on the bench laptop (DEV/SIM ONLY):** replace the apt
     `autoware_planning_msgs` with a trimmed overlay containing only `Trajectory` +
     `TrajectoryPoint` (verbatim official `.msg`).
   - **Deployment implication — TRACKED as a vehicle-tier precondition:** a real
     bench/vehicle integration must carry the **genuine, full** `autoware_planning_msgs`, so
     the r2r codegen limitation must be **resolved properly** (bump r2r off `=0.9.5` once it
     handles these messages, an upstream r2r fix, or a sanctioned minimal-interface package)
     — **NOT** by shipping the trimmed package. Recorded as **AOU-MSG-TOOLCHAIN-001** in
     `ASSUMPTIONS_OF_USE.md` (OPEN).
2. **The `lanelet2` C++ corridor bridge does not compile on Jazzy** — lanelet2's
   serialization API differs from what `corridor/lanelet2_bridge.cpp` was written against
   (`lanelet::LaneletMap` has no member `serialize`). **Out of scope for the perception
   gate** — the `ros2`/`lanelet2` feature split (PR #186) isolates it — but a known
   constraint for whenever the corridor bridge is needed on Jazzy. (Build note; see the
   adapter `README.md`.)
3. **The `lanelet2` C++ build also needs Eigen3** on the include path (`libeigen3-dev` + an
   Eigen include dir); `build.rs` does not auto-discover Eigen. (Build note; see the adapter
   `README.md`.)

Constraints 2 and 3 are **build notes for the `lanelet2` feature only** and do not affect
the `ros2` perception build that this record attests. Constraint 1 is the one with a
deployment consequence and is tracked as an AoU.

### Net status after this run
| Item | Status |
|------|--------|
| Sub-gate 1 — decode boundary (Layer 2 automated) | **PASS** (ROS 2 Jazzy, 2026-06-04) |
| Sub-gate 1 — full node tick (`run_full_node_integration`) | **automated** (`--ignored` live-DDS test; verdict-level, exact cap stays Layer-1) |
| Sub-gate 2 — frame confirm (AWSIM) | **pending** |
| AOU-PERCEPTION-FRAME-001 | **OPEN** (unchanged) |
| `KIRRA_PERCEPTION_DERATE_ENABLED` | **OFF** (unchanged) |
| AOU-MSG-TOOLCHAIN-001 (full-message-set / r2r codegen) | **OPEN** (new; vehicle-tier precondition) |
