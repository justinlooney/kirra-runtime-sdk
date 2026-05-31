# ADR-0002: Condition-dependent speed cap + sub-ODD partition

| Field | Value |
|---|---|
| Status | Accepted (extends ADR-0001) |
| Date | 2026-05-31 |
| Deciders | Project owner |
| Issues | S4 (#116, closed), S8 (#120), G2 (#123), IDC (#124), highway-activation tracking (filed in this commit) |
| Doc | docs/safety/OCCY_SOTIF.md §1.2 (canonical sub-ODD statement); docs/safety/SPEED_ENVELOPE.md (KIRRA-OCCY-SPEED-001) |

## Context

ADR-0001 locked a single 50 mph cap for the urban / surface deployment ODD. A
driverless service may later add controlled-access highway operation at a
higher cap. Without an architecture for cap variation, that expansion is a
re-architecture: every check, demo, and validation is hardcoded to one cap.

Generalize the cap so highway expansion becomes a config + validation event,
not a re-architecture. Keep the lower urban cap as the safe default; only
permit higher caps in a sub-ODD that genuinely earns them.

## Decision

**The deployment ODD is partitioned into sub-ODDs; the Governor enforces the
cap of the currently confirmed sub-ODD, with condition derate composed on top.**

Five rules:

1. **Sub-ODD partition.** Each sub-ODD has its own nominal cap and entry
   conditions.
   - **Sub-ODD A — Urban / surface (ACTIVE at launch):** surface streets, cap =
     50 mph (ADR-0001), full hazard profile (VRU / intersections / water / rail).
   - **Sub-ODD B — Controlled-access highway (DEFINED, NOT ACTIVATED):**
     divided, controlled-access, no at-grade crossings, no VRU / cross-traffic /
     driveways; higher cap earned later. **Keyed on access-control + divided
     road type, not posted speed** — a fast undivided arterial stays Sub-ODD A.

2. **Cap composition (enforced each cycle by the Governor):**

       cap = min( subODD_nominal(confirmed sub-ODD),
                  weather_derate(conditions),
                  range_supported(validated detection range) )

   The most-conservative input always wins.

3. **Mode determination is Governor-confirmed, default-deny.** HD-map road
   class + localization confidence + perception corroboration. Planner may
   *request* a sub-ODD; the Governor independently *verifies* before honoring
   it. Rides on localization integrity (common-cause input → DFA C6 / #123).

4. **Asymmetric transitions: earn up slowly, drop down instantly.**
   - **Raise** the cap only on positive high-confidence Sub-ODD-B confirmation
     AND validated detection range supporting the higher cap.
   - **Derate** to Sub-ODD A immediately on any loss of confirmation: off-ramp,
     localization-confidence drop, perception ambiguity, weather, sensor
     degradation. No hysteresis on the drop.

5. **Sub-ODD B activation is NOT this ADR.** Sub-ODD B is defined here so the
   architecture supports it, but it is **not active at launch**. Activation
   requires its own safety case (tracked by a dedicated issue, filed in this
   commit) — see "Activation gate" below.

## Activation gate (Sub-ODD B)

Activation requires (and is tracked by the deferred issue):

- Governor-confirmed Sub-ODD-B entry conditions (map + perception, default-deny)
- S8-validated detection range for highway object classes at the higher cap,
  including the residual: high-speed water-surface detection
- Breaking-point recomputed at the higher cap (per SPEED_ENVELOPE.md §4)
- IDC obstacle + water characterized at highway range (per KIRRA-OCCY-IDC-001
  §7)
- Earn-up / instant-derate-down implemented and tested

## Consequences

**Positive:**

- Highway expansion becomes a config + validation event, not a re-architecture.
- Lower (Sub-ODD A, 50 mph) cap stays the safe default — no risk of an
  accidental highway cap activation.
- `cap = min(...)` composes cleanly: any input can pull the cap down; only
  Sub-ODD-B confirmation + validated range can lift it.
- Asymmetric transitions match safety intuition: hard to claim more
  capability, easy to revert.

**Negative / risk:**

- Mode determination is now **safety-critical** — wrong sub-ODD = wrong cap.
  Rides on localization integrity (common-cause input, DFA C6 → #123).
- Higher Sub-ODD-B cap re-opens the breaking-point math (SPEED_ENVELOPE.md §4):
  the comfortable basis breaks at ~60 mph against current ~130 m detection
  range. Sub-ODD-B requires longer-range sensing validated by S8 (#120) before
  activation.
- More machinery to validate (sub-ODD entry, transitions, derate paths). The
  complexity is in *enabling future capability*, not in current operation.

**Alternatives considered:**

- *Single static cap forever:* rejected — every cap change becomes a
  re-architecture. Highway activation would require touching every check, demo,
  and validation.
- *Switch on posted speed limit:* rejected — conflates a fast undivided
  arterial (still SG4 / SG5 / VRU hazard profile) with a controlled-access
  highway. Dangerous.
- *Switch on geofence only:* insufficient — must also verify perception
  corroborates the map (Sub-ODD-B entry is a live safety claim, not a
  config flag).

## Links

- ADR-0001 — Occy ODD speed cap = 50 mph / 80 km/h (this ADR extends it).
- `docs/safety/OCCY_SOTIF.md` §1.2 — Sub-ODD model + condition-dependent cap
  function (canonical statement).
- `docs/safety/SPEED_ENVELOPE.md` (KIRRA-OCCY-SPEED-001) — SSD derivation; the
  cap-as-function-of-R rule generalized here as the `range_supported(...)` term.
- Issue #116 (S4 — closed) — original ODD lock.
- Issue #120 — S8 detection-range validation (required for Sub-ODD-B
  activation).
- Issue #123 — G2 localization-integrity coupling (mode determination rides on
  this).
- Issue #124 — IDC (independent detection channel; characterized at highway
  range as part of Sub-ODD-B activation).
- Highway-activation tracking issue (filed in this commit) — Sub-ODD-B
  activation gate.
