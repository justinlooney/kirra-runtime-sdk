# Angular-Velocity Bound — SOTIF Derivation

**Doc ID:** KIRRA-OCCY-ANGULAR-SOTIF-001
**Issue:** #136
**Replaces:** the H1 placeholder constants
`MAX_ANGULAR_VELOCITY_RAD_S_PLACEHOLDER = 1.5` and
`MRC_ANGULAR_VELOCITY_CEILING_RAD_S = 0.5` (now deleted from
`parko-kirra`).

---

> # ⚠️ DRAFT — pending formal safety-engineer review
>
> This document is a **draft engineering analysis** that a human
> safety engineer must review and validate before it can be treated
> as authoritative. The improvement over the H1 status quo is real
> (defensible values with explicit reasoning where there were none),
> but **this is not yet a validated safety claim.**
>
> The numbers below were derived from first-principles physics +
> cited contact-velocity standards + the existing safety case's
> FTTI budget. They have not been bench-tested on a real platform
> and they have not been signed off by a safety engineer. Treat
> every "we choose X" as a starting point, not a settled value.

---

## 1. Scope

`parko-kirra::KirraGovernor` enforces an absolute upper bound on
`|angular_velocity|` for every command leaving the inference loop
(introduced in H1, issue #134). H1 shipped with two **placeholder**
numeric values flagged `// TODO(SOTIF)`; this document is the
derivation that replaces those placeholders.

The bound is now a function of the proposed command's linear
velocity:

  **ω_max(v) = min(ω_rollover(v), ω_sweep, ω_ftti)**

with `ω_rollover(v)` masked below a low-speed floor to avoid the
v → 0 singularity. The three constraints are derived in §2.

The implementation lives in
`parko/crates/parko-kirra/src/angular_bound.rs`. The integration into
the governor's evaluate path is in
`parko/crates/parko-kirra/src/lib.rs::KirraGovernor::nominal_angular_clamp`
and `apply_mrc_profile`.

## 2. The three constraints

### 2.1 Dynamic rollover (binds at high linear speed)

A rigid platform turning at linear velocity `v` and angular velocity
`ω` follows a circular path of radius `R = v / ω` with lateral
(centripetal) acceleration

```
a_lat = v · ω           [m/s²]
```

Static stability factor for a wheeled body with track width `t` and
centre-of-gravity height `h` (CoG assumed centred above the wheelbase
— see §3.1 for the assumption):

```
a_tip = g · (t / 2) / h           [m/s²]
```

where `g = 9.81 m/s²` is standard gravity.

Setting `a_lat ≤ a_tip`:

```
v · ω ≤ g · t / (2 · h)
⇒ ω_rollover(v) = g · t / (2 · h · v)        for v > 0
```

**v → 0 singularity:** `ω_rollover(v) → ∞` as `v → 0`. The formula
does not apply to in-place rotation (no forward motion means no
centripetal acceleration is built up before the rotation reverses
sign at the control loop's tick rate). The implementation masks
`ω_rollover` below `ROLLOVER_MIN_LINEAR_VELOCITY_MPS = 0.05 m/s`;
sweep + FTTI then bind.

### 2.2 Sweep / contact velocity (binds at low linear speed, including v=0)

The robot's outermost point at radius `r_extent` (the bounding circle
of the platform's footprint, including any cantilevered payload) moves
at tangential velocity

```
v_tangential = r_extent · ω
```

For safe contact with a human bystander, bound this by a safe
contact velocity `v_edge_safe`:

```
r_extent · ω ≤ v_edge_safe
⇒ ω_sweep = v_edge_safe / r_extent
```

**Basis for `v_edge_safe`:** [ISO/TS 15066:2016](https://www.iso.org/standard/62996.html)
§5.5.5 power-and-force-limiting contact-velocity envelopes for
collaborative robots. The standard gives a per-body-region table; the
**conservative end of the range covers vulnerable regions** (head,
neck, face — `v_edge_safe ≈ 0.05–0.10 m/s`). Less vulnerable regions
(upper arm, chest) tolerate up to ~0.25 m/s. **We choose the
conservative end for the default**; integrators with characterized
exposure profiles may relax.

ISO 13482:2014 (personal-care mobile robots) provides an alternative
basis with similar magnitudes; we cite ISO/TS 15066 because it has
explicit per-body-region values and is more commonly referenced in
collaborative-robot safety cases.

### 2.3 Perception / FTTI coupling (often binds at low v)

Rotating fast changes the robot's heading inside one fault-tolerant
time interval. If the heading change exceeds the validity range of
the perception / policy reasoning (the "look-where-you-haven't-looked"
problem), the safety case can no longer reason about what happens in
the next cycle. Bound the heading change to `θ_max` per FTTI `τ`:

```
ω · τ ≤ θ_max
⇒ ω_ftti = θ_max / τ_FTTI
```

**Basis for `θ_max`:** a heading uncertainty above which the
governor's policy-validity argument breaks down. For the parko
inference-loop tick rate, `θ_max = 5° ≈ 0.087 rad` per `τ = 0.1 s`
gives `ω_ftti = 0.87 rad/s`. Tighten for platforms with
narrow-field-of-view sensors or longer FTTI budgets.

### 2.4 Composite bound

```
ω_max(v) = min(ω_rollover(v), ω_sweep, ω_ftti)
```

with `ω_rollover(v) = +∞` whenever `v < ROLLOVER_MIN_LINEAR_VELOCITY_MPS`
(handles the v → 0 case cleanly).

## 3. Assumptions

- **3.1 Centred CoG.** The static stability factor `g·t/(2·h)`
  assumes the CoG is centred above the wheelbase. Off-centre CoGs
  produce a directional rollover bound (tighter in one yaw direction
  than the other). Out of scope for M1; a non-centred CoG should be
  modelled with a directional bound and is filed as a follow-up.
- **3.2 Rigid body.** Suspension travel, tyre sidewall deflection,
  and payload compliance are ignored. A real platform will tip at
  a lower `a_lat` than the rigid-body threshold; the rigid-body
  number is therefore an upper bound on the tip-over threshold, and
  the bound we enforce is conservative (we use `a_tip` as the
  *ceiling* that `a_lat` must stay under).
- **3.3 Constant surface friction.** Loss of traction (`μ` drop)
  produces sideslip, not rollover, but it also caps the lateral
  acceleration the platform can build up. The rollover threshold is
  the binding constraint when `μ` is high; sideslip dominates when
  `μ` is low. This bound does not protect against low-`μ` slip
  events — that's a separate constraint (out of scope here).
- **3.4 v=0 rollover masking.** Below
  `ROLLOVER_MIN_LINEAR_VELOCITY_MPS = 0.05 m/s` we treat
  `ω_rollover` as non-binding. Rationale: the lateral acceleration
  `a_lat = v · ω` at very low `v` is bounded by `v` itself; even
  with very high `ω`, the centripetal force built up over one
  control-loop tick is negligible at v < 0.05 m/s. Sweep + FTTI
  bind the angular axis in the in-place-rotation regime; they are
  more appropriate constraints for that physics.
- **3.5 MRC posture factor.** Under Degraded posture we derate
  `v_edge_safe` and `θ_max` by `mrc_posture_factor` (default `0.5`).
  Rollover is **not** derated — the vehicle's geometry doesn't shrink
  in degraded posture. This may need revisiting: a Degraded sensor
  suite means the perception confidence interval widens, which
  arguably means `θ_max` should derate more aggressively than
  `v_edge_safe`. Flagged for safety-engineer review.

## 4. Worked reference example

`PlatformParams::urban_service_robot_reference()` — a small mobile
service robot approximately TurtleBot-4 scale:

| Parameter | Value | Source |
|---|---|---|
| `track_width_m`     | 0.50 | Typical small mobile base |
| `cog_height_m`      | 0.40 | Battery + chassis at mid-height |
| `robot_extent_m`    | 0.30 | Bounding-circle radius incl. payload |
| `v_edge_safe_mps`   | 0.25 | ISO/TS 15066 upper-arm/chest contact |
| `theta_max_rad`     | 0.087 (≈ 5°) | Sensor FoV + policy validity heuristic |
| `ftti_s`            | 0.10 | parko inference-loop tick budget |
| `mrc_posture_factor`| 0.5  | (this doc, §3.5 — pending review) |

### 4.1 ω_max(v) — Nominal posture

| v (m/s) | ω_rollover (rad/s) | ω_sweep (rad/s) | ω_ftti (rad/s) | **ω_max(v)** | Binding |
|---|---|---|---|---|---|
| 0.00 | — (masked) | 0.833 | 0.870 | **0.833** | sweep |
| 0.10 | 61.3       | 0.833 | 0.870 | **0.833** | sweep |
| 1.00 | 6.13       | 0.833 | 0.870 | **0.833** | sweep |
| 5.00 | 1.23       | 0.833 | 0.870 | **0.833** | sweep |
| 7.50 | 0.817      | 0.833 | 0.870 | **0.817** | rollover |
| 10.00| 0.613      | 0.833 | 0.870 | **0.613** | rollover |

For this platform, **sweep binds across the practical operating
range** (v ≤ ~7 m/s). Rollover only starts binding at extreme speeds
(7+ m/s), well past the urban-service-robot operating envelope.

### 4.2 ω_max(v) — MRC posture (derate by 0.5)

| v (m/s) | ω_sweep_eff | ω_ftti_eff | **ω_max(v)** |
|---|---|---|---|
| 0.00 | 0.417 | 0.435 | **0.417** |
| 1.00 | 0.417 | 0.435 | **0.417** |
| 5.00 | 0.417 | 0.435 | **0.417** |

Sweep (with halved `v_edge_safe`) binds throughout. The Degraded
posture's effective sweep bound `0.4167 rad/s` is about half the
Nominal `0.833 rad/s` — the envelope contracts ~2× when posture
degrades, matching the linear-axis MRC philosophy.

### 4.3 Comparison to the H1 placeholder

| | H1 placeholder | SOTIF-derived (urban ref) | Direction |
|---|---|---|---|
| Nominal | 1.500 rad/s | 0.833 rad/s | **Tighter** (1.8×) |
| MRC | 0.500 rad/s | 0.417 rad/s | **Tighter** (1.2×) |

The H1 placeholder was 1.8× too permissive on the Nominal axis for
the reference platform. The SOTIF-derived value is tighter and now
has reasoning behind it.

## 5. Conservative default — uncharacterised platforms

`PlatformParams::conservative_default()` is intended for deployments
that have NOT yet characterised their platform geometry. Every
parameter is chosen so the resulting `ω_max` is tighter than the
reference platform at every plausible v:

| Parameter | Default | Rationale |
|---|---|---|
| `track_width_m`     | 0.20  | Small base |
| `cog_height_m`      | 0.50  | Top-heavy |
| `robot_extent_m`    | 0.50  | Large payload assumption |
| `v_edge_safe_mps`   | 0.10  | ISO/TS 15066 conservative-end (vulnerable body regions) |
| `theta_max_rad`     | 0.05 (≈ 2.9°) | Tight perception/policy budget |
| `ftti_s`            | 0.10  | Same as urban reference |

Produces `ω_max(0) = min(∞, 0.20, 0.50) = 0.20 rad/s ≈ 11.5°/s` — a
slow, deliberate turn. A misconfigured / unprofiled deployment using
the default produces a tight bound that fails toward safe. Test
`omega_max_conservative_default_is_at_or_below_reference_platform`
pins this property.

## 6. Implementation reference

- **Module:** `parko/crates/parko-kirra/src/angular_bound.rs`
- **Types:** `PlatformParams`, `AngularVelocityBound`,
  `ROLLOVER_MIN_LINEAR_VELOCITY_MPS`.
- **Governor wiring:**
  `parko/crates/parko-kirra/src/lib.rs::KirraGovernor::nominal_angular_clamp`
  and `apply_mrc_profile` both call `bound.omega_max(v_proposed)`
  per tick.
- **Builder API:**
  - `KirraGovernor::new()` — uses the conservative default.
  - `KirraGovernor::with_platform_params(PlatformParams)` —
    integrator passes platform-specific geometry + budgets.
  - `KirraGovernor::with_angular_bounds(nom, mrc)` — back-compat
    v-independent scalar override.
- **SAFETY tag:** `SG8 SG9 | REQ: angular-velocity-bound-sotif`.

## 7. Tests

### Pure derivation (`angular_bound::tests`)

- `omega_max_in_place_rotation_returns_sweep_or_ftti`
- `omega_max_at_v_zero_is_finite` (no singularity leak)
- `omega_max_at_v_below_rollover_floor_ignores_rollover`
- `omega_max_sweep_binds_at_low_v` / `omega_max_rollover_binds_at_high_v` / `omega_max_ftti_binds_when_theta_is_tight`
- `omega_max_conservative_default_is_at_or_below_reference_platform`
- `omega_max_mrc_is_tighter_than_nominal`
- `omega_max_scalar_variant_is_v_independent`
- `omega_max_mrc_at_v_zero_for_reference_platform`
- Param validation tests (`platform_params_validate_*`).

### Governor integration (`parko_kirra::tests`)

- `derived_bound_changes_verdict_between_platforms` — swapping
  PlatformParams changes the verdict for the same command.
- `derived_in_place_rotation_clamps_to_sweep_bound`
- `derived_mrc_in_place_rotation_is_tighter_than_nominal`
- `with_angular_bounds_scalar_back_compat_is_v_independent`
- All 8 H1 enforcement-logic tests (sign preservation, multi-axis
  ClampMotion, sticky behaviour) still pass under
  `legacy_scalar_gov()` (calls `with_angular_bounds(1.5, 0.5)` to
  preserve the H1 numeric values for those tests).

## 8. Open items

1. **Off-centre CoG** — current derivation assumes centred CoG.
   Directional rollover bound is a follow-up.
2. **Low-μ sideslip** — not addressed; needs a separate friction-aware
   constraint.
3. **MRC posture factor split** — currently the same `0.5` for both
   `v_edge_safe` and `θ_max`. The argument that `θ_max` should derate
   more aggressively under sensor degradation (wider perception
   uncertainty) deserves separate analysis. Flagged for review.
4. **Per-platform `v_edge_safe` characterisation** — the default
   uses the conservative end of the ISO/TS 15066 table.
   Platform-specific contact exposure profiles (which body regions
   can the platform realistically contact?) should refine this.
5. **Bench validation** — none of the derived numbers have been
   tested on a real platform. This must happen before the bound
   is treated as a validated safety claim.

## 9. Document control

| Field | Value |
|---|---|
| Issue | #136 |
| Status | **DRAFT — pending formal safety-engineer review** |
| Author | engineering analysis (#136 implementation branch) |
| Review status | not yet reviewed |
| Cross-refs | KIRRA-OCCY-SPEED-001 (linear analog), KIRRA-OCCY-OPTIONB-001 |
| Code | `parko/crates/parko-kirra/src/angular_bound.rs` |
| Tests | `angular_bound::tests`, `parko_kirra::tests::derived_*` |

---

When this document earns a safety-engineer sign-off, remove the
DRAFT banner at the top, change "Status" to "Reviewed", and add the
reviewer + date in §9.
