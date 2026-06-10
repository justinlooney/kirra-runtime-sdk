# ADR-0005: Availability Model — Externally-Orchestrated Failover (No Autonomous Leader Election in v1)

| Field | Value |
|---|---|
| Status | **Accepted** |
| Date | 2026-06-09 |
| Deciders | Project owner |
| Issues | #81 (closed by-design); relates to #78, #79, #80, #83 |
| Doc | docs/v1_release_notes_and_architecture.md; src/standby_monitor.rs |
| Builds on | ADR-0003/0004 (independent safety channel) |

## Context

KIRRA is a fail-closed safety governor whose safety case rests on an
independent safety channel (ADR-0003/0004) and a deliberately minimal
autonomous-behavior surface within that channel. The HA deployment is a
two-node primary/standby pair (Active / PassiveStandby) over a
WAL-shipping-replicated SQLite store.

The current mechanism (src/standby_monitor.rs):
- The Active node writes a heartbeat token; the standby tracks its freshness
  as a monotonic change-token, skew-immune (#80).
- Promotion is one-way; a promoted standby never reverts.
- Cold start fails closed: the standby does NOT auto-promote on an absent
  heartbeat (no prior primary ⇒ no leader, rather than a wrong leader).
- A durable epoch claim (CAS) is the split-brain fence (#79): at most one node
  holds the current epoch; a fenced writer self-demotes and fails closed.

Issue #81 proposed adding autonomous leader election and startup
re-arbitration targeting "zero spurious failovers."

## Decision

v1 does **not** implement autonomous leader election. Leadership transitions
are driven **externally** — by the deployment's orchestration layer
(orchestrator / init system) or by operator action.

The architectural invariant: the governor owns **safety** (guaranteeing the
Active node is safe, and failing closed otherwise); the orchestration/operator
layer owns **availability** (deciding which node is Active). These concerns are
separated.

The split-brain **safety** guarantee is independent of who drives failover: the
durable epoch fence (#79) ensures at most one node can be an effective Active —
even an external mistake that promotes two nodes results in the fenced loser
self-demoting and denying. "Zero spurious failovers" holds by construction:
with no autonomous election, there is no autonomous failover to flap.

## Consequences

- (+) Minimal autonomous-behavior surface inside the safety channel — the
  property the safety case depends on (ADR-0003/0004). No election state
  machine to specify, verify, or certify; fewer autonomous transitions that
  could misfire.
- (+) "Zero spurious failovers" holds by construction: with no autonomous
  election there is no autonomous failover to flap, regardless of network
  jitter or heartbeat skew.
- (+) Split-brain safety is independent of failover correctness — the durable
  epoch fence (#79) guarantees at most one effective Active even if the
  external layer mistakenly promotes two nodes (the fenced loser self-demotes
  and denies, fail-closed).
- (+) Smaller, more auditable HA implementation: a heartbeat token + one-way
  promotion + an epoch CAS, rather than a consensus protocol.
- (−) Availability now depends on the orchestration/operator layer being
  present, correct, and timely. A missing or slow orchestrator lengthens
  downtime; cold start does not auto-promote (no prior primary ⇒ no leader),
  so an unattended single-node cold start stays PassiveStandby until acted on.
- (−) No autonomous recovery from primary failure: mean-time-to-recovery is
  bounded by the external layer's response, not by the governor.
- (−) Deployment assumption-of-use: the integrator MUST supply the failover
  driver (orchestrator health-check + promote, or an operator runbook). This
  is documented in docs/v1_release_notes_and_architecture.md.
- (~) Future-compatible: if a later product tier needs autonomous failover, it
  is added by a superseding ADR. The epoch fence (#79) already makes autonomous
  election SAFE to add without revisiting the split-brain guarantee — this ADR
  defers it as unnecessary for v1, not as unsafe.
