# ADR-0003: Two-tier KIRRA architecture — base downstream Governor (SEooC) + optional D1 independent-detection add-on

| Field | Value |
|---|---|
| Status | Accepted |
| Date | 2026-05-31 |
| Deciders | Project owner |
| Issues | #114 (S2 DFA), #119 (S7 Governor fault model), #120 (S8 V&V), #124 (D1 add-on, re-scoped here), base-tier input-contract issue (filed in this commit) |
| Doc | docs/safety/OCCY_ARCHITECTURE_TIERS.md (KIRRA-OCCY-ARCH-001) |

## Context

KIRRA lives downstream of perception and owns no sensors by default. Two
competing pressures shape the architecture:

- **Vendor-neutral pull.** A downstream checker dropped onto any stack
  maximizes integrator reach and protects KIRRA's commercial neutrality.
  Owning sensors at the core would force every integrator to standardize on
  KIRRA's sensor BOM and breaks the SEooC story.
- **Independence pull.** Pure-downstream **delegates** the omission
  common-cause (DFA C5/C7) to the integrator's perception. The omission
  failure mode — unseen pedestrian, undetected water edge — is exactly the
  class conservative re-derivation cannot close (you cannot be conservative
  about something you cannot see). Owning a diverse detection channel closes
  it **unilaterally** and unlocks night-VRU, water safety, larger operating
  envelope, the ASIL-D omission claim, and a True-Redundancy-class moat.

These don't have to be either/or. The decision: resolve via **two tiers**.

## Decision

**Two tiers, one envelope function.**

1. **Tier 1 — Base Governor (downstream SEooC).** KIRRA Governor consumes the
   integrator's world model. Provides conservative/formal checking, a
   **Perception Input Contract** with documented assumptions-of-use, and an
   operating envelope **bounded to the delivered, currently-healthy coverage**.
   Vendor- and sensor-config-neutral. Omission is delegated + envelope-bounded
   (not closed unilaterally); ASIL-D claim is conditional on the contract.
2. **Tier 2 — Optional D1 Independent Detection Channel (add-on).** KIRRA's
   own dedicated radar + thermal/IR + optical/polarization (water) on the
   Governor's independent compute — the settled D1–D3 spec. Plugs in as an
   **additive coverage source** through the same envelope function; no code
   fork. Closes the omission common-cause unilaterally for the
   omission-critical classes (obstacle-in-path, VRU including night, water
   surface).
3. **Unifying mechanism.** Both tiers run the same `cap = f(confirmed
   sub-ODD, conditions, healthy coverage)` function (ADR-0002). D1 is simply
   *additional healthy coverage*: more independent coverage → bigger envelope;
   lose D1 → contract to the base envelope. Adding / losing / omitting D1
   never silently compensates.
4. **Capability gating per the matrix** in OCCY_ARCHITECTURE_TIERS.md §2 —
   night VRU, water safety, envelope size, safety-case strength, and the
   independent ASIL-D omission claim are the rows that flip with D1.

## Consequences

**Positive:**

- **Vendor-neutral base** preserved — KIRRA drops onto any stack, integrator
  perception of any kind.
- **Premium tier = safety moat.** D1 closes omission unilaterally; competitors
  doing both halves in-house can't honestly claim the same independence (DFA
  C9).
- **Unlocks night VRU, robust SG4 water, larger envelope, and the
  unilateral ASIL-D omission claim** — exactly the things the base SEooC
  can't deliver alone.
- **No code fork.** D1 is additive coverage through the existing envelope
  function; one product path, one safety case skeleton, optional module.

**Negative / risk:**

- **Base tier carries an explicit perception assumption-of-use.** Integrators
  must meet the Perception Input Contract or run with a reduced envelope.
  This is honest and standard for an SEooC, but it's a real constraint to
  document and verify at runtime.
- **D1 reintroduces a sensor BOM** (radar + thermal + optical) for customers
  who opt in. Acceptable because it's opt-in, not core.
- **Two SKUs to validate** — base safety case and base + D1 safety case.
  Mitigated by the shared envelope function (most of the case is reused).

**Alternatives considered:**

- *Core-mandatory IDC:* rejected — breaks vendor-neutrality, forces a sensor
  BOM on every integrator, kills the SEooC story.
- *Pure-downstream-only:* rejected — cedes the strongest omission
  independence, caps the operating envelope at integrator coverage, no moat
  beyond "we're a checker."
- *Full owned perception layer:* rejected — massive scope, duplicates
  integrator capability, doesn't actually need a full world model to close
  omission (the focused D1 is sufficient).

## Links

- `docs/safety/OCCY_ARCHITECTURE_TIERS.md` (KIRRA-OCCY-ARCH-001) — canonical
  tier description, capability matrix, Perception Input Contract.
- `docs/safety/OCCY_INDEPENDENT_DETECTOR.md` (KIRRA-OCCY-IDC-001) — D1 add-on
  module design spec; D1–D3 settled per this ADR.
- `docs/safety/OCCY_DFA.md` (KIRRA-OCCY-DFA-001) — C5/C7 disposition is now
  tier-dependent.
- ADR-0002 — sub-ODD + condition-dependent cap; provides the envelope
  function this ADR makes additive across tiers.
- Issue #114 — S2 DFA; the C5 finding this ADR addresses with the two-tier
  resolution.
- Issue #119 — S7 Governor fault model + degraded-mode availability.
- Issue #120 — S8 V&V; characterizes integrator perception (base) and D1
  (add-on) coverage.
- Issue #124 — re-scoped here as the D1 add-on tracking issue.
- Base-tier Perception Input Contract issue (filed in this commit) — Tier-1
  contract definition + runtime verification + SEooC assumptions-of-use.
