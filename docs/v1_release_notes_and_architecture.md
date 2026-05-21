# Aegis v1.0.0 — Release Notes and Architecture Specification

This document serves as the definitive release announcement and architectural specification for the Aegis v1.0.0 Stable Control Plane baseline. It establishes the programmatic lineage, structural topology mappings, and core engineering philosophy behind the production-hardened legitimacy platform.

---

## 1. Release Declaration

Aegis v1.0.0 establishes a production-grade distributed legitimacy control plane combining:
- TPM-backed cryptographic attestation
- Recursive DAG trust propagation
- Fail-closed gateway enforcement
- Persistent verifier continuity
- Historical posture telemetry
- Active/passive operational redundancy
- Atomic disaster recovery restoration

> **The Aegis Thesis**: The system does not assume trust. The system continuously proves trust.

---

## 2. Milestone Lineage

The evolution of the Aegis codebase through its iterative milestones tracks a deliberate, uncompromising path toward architectural hardening. Every milestone preserved fail-closed semantics as a non-negotiable invariant.

| Version | Capability Introduced |
| :--- | :--- |
| **v0.9.3** | TPM attestation + nonce replay boundaries |
| **v0.9.4** | Recursive fleet legitimacy DAG + fail-closed gateway cache |
| **v0.9.5** | SQLite-backed durable verifier persistence |
| **v0.9.6** | HTTP command policy classification + Tower enforcement |
| **v0.9.7** | Posture history + flapping telemetry |
| **v0.9.8** | Active/passive verifier HA + backup export |
| **v0.9.9** | Atomic backup import + DR drill framework |
| **v1.0.0** | Stability audit + operational specification freeze |

---

## 3. Core Architectural Layers

The architecture is partitioned into distinct functional domains designed to isolate concerns, minimize thread-contention over the database layer, and guarantee high-throughput packet processing at the edge proxy boundary.

| Layer | Responsibility |
| :--- | :--- |
| **TPM Edge Node** | Generates hardware-rooted measurements, maps identities to specific public keys, and signs challenges. |
| **Verifier** | Conducts deep cryptographic validation, tracks active challenge lifecycles, and executes recursive topology DAG evaluation. |
| **SQLite Store** | Maintains durable, transaction-isolated persistence of node baselines, dependency mappings, and audit histories. |
| **Gateway Policy Layer** | Intercepts edge payload commands, tracks lookback freshness constraints, and enforces real-time posture routing. |
| **DR Pipeline** | Orchestrates token-protected JSON snapshot serialization, atomic file system clears, and memory cache rehydration. |
| **Observability Layer** | Exposes non-mutating APIs for structural auditing, time-series analysis, and flapping calculation telemetry. |

---

## 4. Full Architecture Diagram

The topology below describes the exact, unidirectional cryptographic validation chain and state rehydration path operating across the control plane infrastructure:

```text
┌─────────────────────┐
│    Edge TPM Node    │
├─────────────────────┤
│  AIK + PCR16 Quote  │
└──────────┬──────────┘
           │
           ▼
┌──────────────────────────┐
│      Aegis Verifier      │
├──────────────────────────┤
│  Nonce Validation        │
│  Signature Verification  │
│  PCR16 Digest Matching   │
│  Recursive DAG Engine    │
└──────────┬───────────────┘
           │
           ├────────────────────────────┐
           ▼                            ▼
┌───────────────────┐        ┌────────────────────┐
│SQLite Persistence │        │ Posture Event Log  │
├───────────────────┤        ├────────────────────┤
│  Nodes            │        │  Timeline History  │
│  Dependencies     │        │  Flapping Analysis │
│  Recovery Blocks  │        │  Audit Trail       │
└─────────┬─────────┘        └─────────┬──────────┘
          │                            │
          └────────────┬───────────────┘
                       ▼
┌─────────────────────────────────────────┐
│          Gateway Policy Layer           │
├─────────────────────────────────────────┤
│  TTL Cache Freshness Enforcement        │
│  Command Classification Engine          │
│  Fail-Closed Routing Decisions          │
└──────────┬──────────────────────────────┘
           │
           ▼
┌─────────────────┐
│ Protected Fleet │
├─────────────────┤
│  Telemetry      │
│  Actuation      │
│  System Control │
└─────────────────┘
```

---

## 5. Operational Security Guarantees

| Guarantee | Programmatic Mechanism |
| :--- | :--- |
| **Replay immunity** | Destructive, single-use nonce consumption before signature/digest parsing occurs. |
| **Split-brain prevention** | Hard mutation rejection with 503 Service Unavailable inside PassiveStandby environments. |
| **Recursive safety** | White/Gray/Black graph coloring loops bounded strictly by `MAX_DEPENDENCY_DEPTH` limits. |
| **Cache freshness** | Real-time temporal evaluation checking via `now_ms().saturating_sub(updated_at_ms) > ttl_ms`. |
| **State durability** | Write-through transactional persistence; database commitments must complete prior to memory adjustments. |
| **DR consistency** | Enforces a full, destructive table wipe wrapped in a single SQLite transaction block. |
| **Route isolation** | Hard token authorization walls utilizing `require_admin_token` middleware validation filters. |

---

## 6. Failure Philosophy

Aegis is explicitly engineered to prioritize platform legitimacy above all else. When an anomaly occurs, the control-plane platform defaults to a defensive posture.

Aegis is engineered to fail closed. When uncertainty exists:
 * Permissions collapse
 * Mutations halt
 * Topology propagation stops
 * Stale trust expires
 * Malformed state is rejected

> **Core Axiom**: Availability never overrides legitimacy.

---

## 7. Operational Readiness Checklist

| Validation Area | Status | Verification Proof Vector |
| :--- | :--- | :--- |
| TPM verification | **Complete** | Cryptographic parsing of AIK PEM and PCR16 digest checks passing. |
| Replay protection | **Complete** | In-memory `pending_challenges` destructively cleared prior to signature validation. |
| DAG cycle safety | **Complete** | Graph coloring algorithm aborts and rolls back circular node mutations. |
| Gateway policy enforcement | **Complete** | Command classification strictly limits verbs to HTTP structures at the outer boundary. |
| Persistence durability | **Complete** | Write-through serialization models verified across crash-reboot scenarios. |
| Backup/export/import | **Complete** | JSON serialization skips SQLite file system locks to extract complete telemetry states safely. |
| DR simulation | **Complete** | Scripted test harness verifies perfect posture matching after total primary drop-out. |
| Passive standby enforcement | **Complete** | Secondary replicas run in immutable read-only orientation until administrative elevation. |
| Historical telemetry | **Complete** | Persistent tracking tracks analytical metrics and prevents out-of-order logs. |

---

## 8. Known Non-Goals in v1.0.0

To ensure strict engineering discipline and keep code complexities tightly bounded, the following design architectures are explicitly declared out-of-scope for the v1.0.0 release:

 * **Distributed consensus algorithms (e.g., Raft)**: Avoids complex internal state synchronization mechanics.
 * **Automatic leader election**: Failovers remain explicitly driven by external orchestration layers or human actions to eliminate flapping leadership states.
 * **Byzantine fault tolerance**: Assumes the verifier processes themselves are secure; protects against downstream device compromises, not inward software state corruption.
 * **Autonomous remediation**: System provides containment via state drops, avoiding programmatic topology self-healing adjustments.
 * **Self-healing topology rewriting**: Topology modifications require administrative token authentication.
 * **Distributed write replication**: State synchronization is managed via JSON data transfers.

> *Aegis v1.0.0 prioritizes deterministic control, auditability, and fail-closed operation over autonomous coordination complexity.*

---

## 9. Future Evolution Path

The architectural groundwork laid in v1.0.0 allows a clear path forward for subsequent expansion layers without rewriting core data invariants:

 1. **Consensus replication**: Introducing standard clustering mechanisms to eliminate manual operator backup payload injection steps.
 2. **OPA/Rego policy integration**: Migrating the path/verb classification engine to open policy models for dynamic, policy-driven edge evaluations.
 3. **Signed topology manifests**: Cryptographically signing configuration files to secure dependency data blocks before processing.
 4. **Streaming state telemetry**: Transitioning the asynchronous gateway proxy polling model into real-time streaming notifications.
 5. **Hardware root expansion**: Broadening attestation structures to validate nested execution metrics and multiple PCR states simultaneously.
 6. **Gateway proxy federation**: Scaling the proxy interceptors to manage access controls across multi-region deployments.

---

## 10. Final Release Statement

Aegis v1.0.0 establishes a deterministic, cryptographically anchored legitimacy fabric for distributed operational systems. Trust is continuously measured. Authorization is continuously re-evaluated. Failure collapses safely.
