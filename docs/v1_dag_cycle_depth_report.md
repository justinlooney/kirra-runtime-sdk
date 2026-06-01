# Kirra v1.0.0 — Dependency DAG Cycle and Depth Protection Report

This document establishes the definitive mathematical, algorithmic, and defensive specification for the Kirra v1.0.0 Directed Acyclic Graph (DAG) topology evaluation engine. It formally proves how the control plane shields itself against recursive processing exploitation, stack-exhaustion denial of service (DoS), graph poisoning, and adversarial topology manipulation.

---

## 1. DAG Security Model

The structural stability of the Kirra trust propagation engine depends on deterministic execution boundaries. Because trust states are inherited recursively through an asset dependency tree, the evaluation surface represents a high-leverage target for malicious inputs.

> **Core Performance Invariant**: Kirra topology evaluation must always terminate in bounded time.

### Threat Modeling & Mitigation Target Matrix

The topological execution space enforces strict structural controls to counter targeted architectural exploits:

| Threat | Target |
| :--- | :--- |
| Circular dependency injection | Infinite recursion |
| Excessive graph depth | Stack exhaustion |
| Dependency poisoning | False posture inheritance |
| Recursive amplification | CPU starvation |

> **Defensive Posture Rule**: The verifier treats malformed topology as hostile input.

---

## 2. White / Gray / Black Traversal Model

To achieve linear-time cycle detection during topological validation and posture calculation passes, the verifier implements a formal three-color graph depth-first search (DFS) traversal algorithm.

### Node Coloring Definition Matrix

| Color | Meaning |
| :---: | :--- |
| **White** | Unvisited |
| **Gray** | Currently evaluating |
| **Black** | Fully resolved |

> **The Core Mathematical Invariant**: Encountering a Gray node during traversal constitutes an active cycle.

---

## 3. Cycle Detection Flow

When a client attempts a structural modification via `POST /fleet/dependencies`, the graph coloring engine isolates the proposed topology to confirm strict acyclic properties prior to committing any disk or memory allocations.

### Recursive Cycle Tracing Sequence:

```text
node-a
 └── node-b
      └── node-c
           └── node-a  ← cycle detected
```

### Topological Enforcement Outcomes

| Condition | Result |
| :--- | :--- |
| Gray node re-entry | LockedOut |
| Cycle detected | Registration rejected |
| Invalid topology | Fail-closed |

> **Defensive Posture Rule**: Cycles are treated as security violations, not recoverable warnings.

---

## 4. MAX_DEPENDENCY_DEPTH Enforcement

To defend the verifier process execution loops against deep, linear-chain graph injection attacks designed to crash the control plane via stack overflow, the engine applies a hard recursive calculation ceiling.

### Structural Protection Objectives

| Protection Goal | Reason |
| :--- | :--- |
| Stack exhaustion prevention | Recursive depth bounded |
| CPU containment | Evaluation cost capped |
| Deterministic execution | No unbounded traversal |

---

## 5. Recursive Evaluation Guarantees

The core topology calculation framework (`calculate_posture()`) provides unyielding behavioral guarantees across all execution cycles:

 * **Every traversal path terminates.**
 * **Every node resolves exactly once.**
 * **No recursive branch can evaluate indefinitely.**
 * **Dependency inheritance is deterministic.**

### Targeted Dependency Blame Reporting:

To prevent telemetry explosion across large networks, the `blocked_by` block contains only direct failing dependencies, not full recursive ancestry chains. This ensures rapid issue localization for human operators under incident stress.

---

## 6. Malicious Graph Injection Scenarios

The graph evaluation suite is continuously tested against hostile, malformed architectural permutations to ensure the validation code remains fail-closed:

| Attack Pattern | Expected Result |
| :--- | :--- |
| Self-reference (A → A) | LockedOut |
| Two-node cycle (A → B → A) | LockedOut |
| Deep recursive chain (depth > MAX) | LockedOut |
| Unknown dependency target | Degraded |
| Parent Untrusted | Downstream Degraded |

> **Core Testing Invariant**: Malformed dependency graphs are interpreted as adversarial inputs.

---

## 7. Gateway Safety Relationship

The architectural separation between the verifier and the edge gateway proxy is designed to prevent topological processing overhead from lagging the edge network forwarding limits.

> **Boundary Invariant**: The gateway does not independently evaluate topology.

The proxy consumes only the pre-computed, flattened `FleetPosture` state delivered through a bounded TTL cache.

### Decoupled Operational Responsibilities

| Component | Responsibility |
| :--- | :--- |
| Verifier | Recursive topology truth |
| Gateway | Real-time enforcement |
| TTL cache | Temporal trust boundary |

---

## 8. Test Evidence Summary

The defensive capabilities of the graph compilation engine are continuously asserted via an isolated integration test layout:

| Test | Purpose |
| :--- | :--- |
| `test_cycle_detection` | Detect A→B→A loops |
| `test_depth_limit` | Reject excessive recursion |
| `test_nominal_chain` | Validate healthy inheritance |
| `test_untrusted_parent` | Downstream degradation |
| `test_missing_dependency` | Fail-closed unknowns |

> **Defensive Posture Rule**: All topology failures resolve into deterministic, fail-closed posture outcomes.

---

## 9. Forbidden DAG Regressions

 * Do not remove Gray-node detection.
 * Do not allow recursive traversal without depth bounds.
 * Do not silently ignore cycles.
 * Do not downgrade LockedOut cycles into Degraded.
 * Do not permit unbounded dependency expansion.
 * Do not evaluate malformed graphs optimistically.
 * Do not allow topology evaluation after cache expiry.
