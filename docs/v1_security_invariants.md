# Aegis v1.0.0 — Full Security Invariant Audit Specification

This document establishes the absolute security invariants of the Aegis distributed legitimacy engine. Every operational pathway, optimization, and recovery procedure must satisfy these ten structural properties. Any deviation or shortcut constitutes a fatal security failure.

---

## 1. Nonce Lifecycle & Replay Prevention
* **Invariant**: Cryptographic nonces must follow a strict single-use, time-bounded validation cycle.
* **Specification**: 
  * Nonces must be generated using an entropy pool backed by a cryptographically secure pseudo-random number generator (CSPRNG).
  * Active nonces must be cached purely in-memory with a hard expiration window enforced by `CHALLENGE_TTL_MS`.
  * Nonce consumption is completely destructive. Any incoming signature validation pass against a given node ID must pop and delete that node's registered challenge entry from `pending_challenges` immediately before signature or digest evaluation begins. If evaluation fails, the node must not be granted a retroactive fallback token.

## 2. Unmockable Cryptographic Verification
* **Invariant**: Attestation validations must execute full cryptographic mathematical checks; mock bypasses are strictly prohibited.
* **Specification**:
  * Every call entering `verify_attestation` must explicitly parse the Attestation Identity Key (AIK) from its persistent PEM format.
  * The verifier must validate the signature against the actual incoming payload bytes.
  * Platform Configuration Register 16 (`PCR16`) must be extracted directly from the quote data structure, re-hashed, and checked for binary equality against `expected_pcr16_digest_hex` read out of the database store. Hardcoded `NodeTrustState::Trusted` overrides are illegal.

## 3. Administrative Mutation Isolation
* **Invariant**: Structural configurations must be shielded behind strict cryptographic token verification barriers.
* **Specification**:
  * The endpoints `/attestation/register`, `/fleet/dependencies`, `/system/backup/export`, and `/system/backup/import` must remain tightly wrapped within the `require_admin_token` middleware matrix.
  * The system must execute header token comparisons inside constant-time string comparison blocks (`ConstantTimeEq`) to prevent timing-attack extraction of the cluster's administrative keys.

## 4. PassiveStandby Read-Only Enforcement
* **Invariant**: Nodes configured as replicas must be structurally incapable of introducing state mutations or data divergence.
* **Specification**:
  * On service initialization, if `AEGIS_VERIFIER_MODE` resolves to `PassiveStandby`, the gatekeeper helper `require_active_mode` must evaluate to a hard error boundary.
  * Any request attempting to hit `register_node`, `issue_challenge`, `verify_attestation`, or `register_dependencies` on a standby node must fail immediately, returning `503 Service Unavailable` without allocating internal query or execution resources.

## 5. Write-Through Persistence Ordering
* **Invariant**: Runtime memory states must always remain downstream of persistent database transactions.
* **Specification**:
  * To guarantee that the application's concurrent `DashMap` cache never misrepresents the system's actual, auditable baseline under high-concurrency race conditions, all writes must execute as write-through transactions.
  * The internal `VerifierStore` SQLite execution call (`save_node` or `save_dependencies`) must succeed perfectly *before* the matching entry can be inserted into the live `state.nodes` or `state.dependency_graph` memory tables.

## 6. Atomic Restoration State Cleansing
* **Invariant**: Disaster recovery ingestion routines must execute a complete, destructive reset of all operational and volatile security properties.
* **Specification**:
  * Invoking `POST /system/backup/import` must initiate a complete, unconstrained table wipe (`DELETE FROM`) of all data layers inside a single, atomic SQLite transaction block.
  * To prevent token replay windows or unverified node elevations across cluster boundaries, the in-memory `pending_challenges` cache must be explicitly purged.
  * All imported node metadata models must have their `last_nonce` allocations zeroed out, dropping them into an unverified state until they execute a clean, fresh challenge-response loop.

## 7. Dependency DAG Cycle & Depth Protection
* **Invariant**: Topological graph operations must prevent resource exhaustion and infinite evaluation recursion vectors.
* **Specification**:
  * Every execution of `register_dependencies` must evaluate the graph using a formal graph coloring algorithm (White/Gray/Black node tracking) to identify and block cyclic dependencies immediately.
  * The engine must enforce a hard constraint limit defined by `MAX_DEPENDENCY_DEPTH`. Any topological chain length exceeding this metric must be aborted and failed-closed to protect the verifier's call stack from memory exhaustion.

## 8. Gateway Cache TTL Expiration
* **Invariant**: Out-of-band posture telemetry must enforce strict temporal boundaries to prevent stale authorization inheritance.
* **Specification**:
  * The edge gateway proxy interceptor must calculate freshness metrics via `now_ms().saturating_sub(updated_at_ms)`.
  * If the lookback duration exceeds the configured `ttl_ms` barrier, the cache is classified as dead and unvalidated. The engine must drop permissions and evaluate to a hard `false` decision, forcing downstream execution channels to fail-closed.

## 9. Unclassified Command Rejection
* **Invariant**: The command parsing pipeline must treat all unknown or ambiguous instruction sets as explicit security threats.
* **Specification**:
  * The command classification engine `classify_http_command` must map unrecognized payloads to a fail-closed classification.
  * In compliance with core fail-closed invariants, the routing matrix must reject unclassified commands unconditionally across all posture states (including `Nominal`), eliminating backdoor exploitation vectors.
  * **Implementation note**: The current codebase satisfies this invariant by mapping unknown HTTP methods to `OperationalCommand::SystemMutation`, which is blocked in all non-Nominal postures and requires explicit Nominal status to pass. A dedicated `Unknown` variant is not required provided the chosen classification is equally or more restrictive.

## 10. Unified Backup Protection Matrix
* **Invariant**: System snapshots must enjoy the same programmatic security properties as core cryptographic data records.
* **Specification**:
  * The data backup pipeline must serialize and deserialize elements strictly via structured JSON snapshots (`BackupExport`), explicitly side-stepping low-level raw SQLite file-system hot copies to bypass database lock contention and race states.
  * Snapshot transmission and ingestion pathways must run exclusively within the admin token scope block, ensuring data cannot be harvested by unprivileged agents.
