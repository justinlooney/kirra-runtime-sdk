# Per-Class Contract Profiles — the Kinematic Contract Family

| Field | Value |
|---|---|
| Issues | **#312** (per-class profiles), **#313** (VRU-dense courier profile) |
| Status | **NORMATIVE** for the per-class numbers — both code sides cite this table by parameter id |
| Req id | **KIRRA-CLASS-PROFILES-001** |
| Owns | `src/gateway/contract_profiles.rs` (envelope) · `parko-core/src/impact.rs::impact_cfg_for_class` (SG6 threshold) |

> **This table is the single source of truth for the per-class numbers.** The two
> code sides live in **separate workspaces with no dependency edge** (the SDK
> gateway and parko-core), so they **cannot share values by import** — each carries
> a deliberate, *cited* copy keyed to the parameter ids below. **Change a number ⇒
> change it in all three places** (this table, `contract_profiles.rs`,
> `impact_cfg_for_class`). A hidden copy is a future divergence; a cited copy is a
> maintained one.

---

## The sibling rule (the held line)

The frozen instance `src/gateway/kinematics_contract.rs`
(`nominal_reference_profile` / `mrc_fallback_profile`, talisman blob
`997fb7ae15ce3e11adec9218044c7c84b049ad3b`) is **NOT edited**. Per-class profiles
are **siblings**: new constructors that *return the existing public
`VehicleKinematicsContract` struct*, exactly the idiom the two canonical
constructors already establish. The **robotaxi** class member **IS** the frozen
instance — `contract_for(Robotaxi)` delegates to `nominal_reference_profile()`
verbatim (zero new numbers), proven by a field-for-field equality test. A profile
that required changing the talisman's layout would be a **finding, not a feature**.

The family grows *beside* the frozen instance; the governor, the signed audit
chain, and the operator console are **unchanged** across classes — the per-class
delta is confined to the parameters below. See `docs/MARKET_AUTONOMOUS_SERVICES.md`
§3c (why the market needs this) and `docs/ARCHITECTURE_STACK.md` §2 (the
three-domain model + the frozen-talisman rule).

## Status legend

- **INHERITED-FROZEN** — equals the frozen instance; zero new number (robotaxi).
- **VALIDATION-PENDING** — a flagged placeholder with a stated basis; **not** a
  certified value (track-test / SOTIF / bench characterization pending). Same
  honesty as the frozen instance's own footprint numbers and `ImpactCfg::default`.
- **CONFIRMED** — a certified/validated value. *(None yet — this family is new.)*

---

## The class table (Nominal envelope)

Every number below is **VALIDATION-PENDING** unless marked INHERITED-FROZEN.
Units: speed/cap m/s; accel/brake/lat m/s²; steering deg; rate deg/s; distances m.

| Param id | Courier (sidewalk) | Delivery-AV (road pod) | Robotaxi | Status (courier/dav) |
|---|---|---|---|---|
| `*.max_speed` | 3.0 | 12.0 | 35.0 | VALIDATION-PENDING / INHERITED-FROZEN |
| `*.odd_cap` | **2.5** | **11.0** | 22.35 (`URBAN_ODD_SPEED_CAP_MPS`, ADR-0001) | VALIDATION-PENDING / INHERITED |
| `*.accel` | 1.0 | 1.8 | 2.5 | VALIDATION-PENDING |
| `*.brake` | 3.0 | 4.0 | 4.5 | VALIDATION-PENDING |
| `*.steering` | 30.0 | 33.0 | 35.0 | VALIDATION-PENDING |
| `*.steering_rate` | 30.0 | 40.0 | 45.0 | VALIDATION-PENDING |
| `*.follow` | 2.0 | 3.5 | 2.0 | VALIDATION-PENDING |
| `*.lat_accel` | 1.5 | 2.5 | 3.5 | VALIDATION-PENDING |
| `*.wheelbase` | 0.5 | 1.9 | 2.8 | VALIDATION-PENDING |
| `*.footprint` (w×l, overhangs) | 0.6 × 0.9 (0.2/0.2) | 1.1 × 2.9 (0.5/0.5) | 1.85 × 4.8 (0.9/1.1) | VALIDATION-PENDING |
| `*.impact_spike` (SG6, parko; **deviation** `\|‖a‖−G\|`, #321) | **2.5** | **8.0** | **22.0** | VALIDATION-PENDING |
| `*.impact_confirm` (M / N consecutive, #321) | **2 / 3** | **2 / 3** | **1 / 1** | VALIDATION-PENDING |
| **convention** (SG6 decel, #321 / ADL-013) | `\|‖a‖ − G\|`, `G = 9.80665 m/s²` (ISO 80000-3); confirm = M consecutive of last N | same | same | DECIDED (residual: orientation-corrected projection, named future) |

**MRC fallback** (degraded posture) is a stricter sibling per class: every limit
≤ that class's Nominal limit, `follow` ≥ Nominal (the conservative direction), and
the **footprint identical** (the vehicle does not shrink in degraded posture).
These relations are asserted as structural invariants (see the validation gate).

> **#321 / ADL-013 — `impact_spike` is now a gravity-DEVIATION threshold, not a raw
> norm.** The old courier `8.0` was a **raw-norm** number BELOW the ~9.81 m/s² gravity
> floor — a static, level courier read `‖a‖ ≈ 9.81 > 8.0` and **latched on gravity
> alone**. The convention is now `\|‖a‖ − G\|` (≈ 0 at rest), debounced by an
> **M-consecutive-of-N** window (a single-tick jolt does not latch; `M=1/N=1` =
> single-tick / frozen behavior). Robotaxi moves off the raw-norm `30.0` default to a
> `22.0` deviation (M=1/N=1 — a highway crash is unambiguous in one tick; the FTTI
> permits no confirmation delay). `ImpactCfg::default()` keeps `30.0` (M=1/N=1) for
> zero regression in the default path. Residual: the deviation under-represents a
> purely horizontal impulse (vector combination with gravity); orientation-corrected
> projection is the named future improvement, gated on a reliable quaternion.

### Ordering sanity (asserted)
`courier.effective_cap (2.5) < delivery-av (11.0) < robotaxi (35.0)` and
`courier.impact_spike (2.5) < delivery-av (8.0) < robotaxi (22.0)` (deviation units).

---

## VRU-dense rationale (#313 — why each courier bound is shaped by pedestrian proximity)

The sidewalk-courier class operates in **VRU-dense pedestrian space**; commercial
sidewalk-delivery fleets run at roughly **1.5–3 m/s** (walking-pace multiples).
Every courier bound is shaped by that proximity:

- **`odd_cap` = 2.5 m/s** — ~1.8× a 1.4 m/s walking pace; inside the 1.5–3 m/s
  operating band. The pedestrian-space operational ceiling (sibling of
  `URBAN_ODD_SPEED_CAP_MPS`, same ADR-0001 framing of an ODD cap distinct from the
  mechanical max).
- **`accel` = 1.0 m/s²** — gentle starts near pedestrians.
- **`brake` = 3.0 m/s²** — firm service brake → **short absolute stopping distance**
  (≈ 1.04 m at 2.5 m/s). Short stopping distance is the VRU-dense priority, and
  `brake ≥ accel` holds.
- **`lat_accel` = 1.5 m/s²** — gentle lateral comfort near VRUs (matches the frozen
  MRC lateral limit); the bicycle-model clamp further bounds steering at speed.
- **`follow` = 2.0 m** — conservative *relative to* the low speed (~0.8 s headway at
  2.5 m/s plus the robot's short reaction).
- **footprint** (0.6 × 0.9 m, 0.5 m wheelbase) — a small sidewalk robot; tight
  envelopes for pedestrian-space maneuvering. All dimensions strictly positive.
- **`impact_spike` = 2.5 m/s² DEVIATION, confirm 2-of-3 (parko / SG6, #321)** — a
  sidewalk collision at walking pace produces a **small** decel deviation, far below a
  road crash, so the trigger is more sensitive — but the **gravity-deviation
  convention** (`\|‖a‖ − G\|`, ≈ 0 at rest) means it is no longer below the gravity
  floor (the old `8.0` raw-norm was, and a static courier latched on gravity), and the
  **2-of-3 consecutive** window debounces the curb/bump jolts a courier hits often.
  **Still genuinely needs bench characterization of low-speed collision decel
  signatures**; a flagged placeholder, not a guessed certified number. See ADL-013.

---

## The validation gate (the family's certification story)

A profile that fails the frozen instance's properties **does not ship.** That
inheritance is the gate: the proptest battery in
`src/gateway/kinematics_proptest.rs` is **parameterized over every family member**
(courier / delivery-av / robotaxi, Nominal + MRC). Each member must pass the same
profile-agnostic properties — no panic, clamp-in-bounds, allow-implies-safe,
bicycle-model-after-clamp, deterministic — plus the structural invariants
(`brake ≥ accel`, `mrc ≤ nominal` per limit field, footprint positive, `cap ≤
max_speed`). The robotaxi member's field-for-field equality with the frozen
instance is the zero-drift proof.

---

## Selection rule — FAIL-CLOSED

`VehicleClass::from_str` accepts case-insensitive `"courier"` / `"delivery-av"` /
`"robotaxi"`. **Any other string is an `Err`** (the `KIRRA_BACKEND` pattern): a
typo'd class must **never** silently select another class's (e.g. faster) envelope.
There is no default class.

## Deployment note (named, not built)

Class **selection is integrator configuration** — the deployment chooses the class
its vehicle belongs to, and the governor loads `contract_for(class)` /
`impact_cfg_for_class(class)`. **Wiring class selection into the service / node
binaries (an env var or config field) is a later step — named here, not built in
this change.** This change delivers the profile *family* + the validation gate +
the normative table; the binary plumbing is the remaining #312 work.

---

## Cross-references

- `src/gateway/contract_profiles.rs` — the envelope family + `VehicleClass`
  (fail-closed) + the per-class ODD-cap consts.
- `parko-core/src/impact.rs::impact_cfg_for_class` — the SG6 per-class threshold
  (the cited cross-workspace sibling).
- `src/gateway/kinematics_contract.rs` — the **frozen instance** (talisman); never
  edited.
- `docs/MARKET_AUTONOMOUS_SERVICES.md` §3c · `docs/ARCHITECTURE_STACK.md` §2 ·
  `docs/adr/0001-occy-odd-speed-cap.md` (the ODD-cap framing).
