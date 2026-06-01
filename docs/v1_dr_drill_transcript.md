# Kirra v1.0.0 — Disaster Recovery Drill Transcript

This transcript serves as the official cryptographic and operational validation record for the Kirra v1.0.0 Disaster Recovery (DR) pipeline. The operations recorded herein confirm that the system can survive an abrupt primary control plane failure, maintain zero-leak security invariants, and restore topological consistency onto a blank-slate instance without state drift.

---

## Environment
- **Primary Verifier Network Endpoint**: `127.0.0.1:8088`
- **Secondary Verifier Network Endpoint**: `127.0.0.1:8089`
- **Primary Operational Mode**: `Active`
- **Secondary Operational Mode**: `PassiveStandby` (Fails closed on mutations)
- **Persistence Storage Backend**: SQLite Enabled (`WAL` journaling mode)
- **Access Guard Infrastructure**: `require_admin_token` Middleware Enabled

---

## Step 1 — Generate Active State

To establish a production-grade operational baseline, the primary verifier was populated with a multi-layered cryptographic and topological state representation.

### Execution Log:
1. **Node Registrations**: Registered two core edge nodes via token-authorized channels, binding their identity public keys and expected Platform Configuration Register (PCR) configurations.
2. **Dependency Graph Creation**: Established an explicit structural dependency where `node-01` relies on the health of `node-02`.
3. **Successful Attestations**: Executed complete challenge-response loops. `node-01` and `node-02` requested nonces, signed them using physical TPM simulators, and submitted quotes. Both elevated to `NodeTrustState::Trusted`.
4. **Posture Event Generation**: The successful attestation loops and topological adjustments forced the engine to emit three chronological entries into the `posture_events` time-series ledger (`DEPENDENCY_UPDATED`, followed by two `ATTESTATION_TRUSTED` sequences).

---

## Step 2 — Export Snapshot

With the primary verifier in a state of terminal stability, an administrative snapshot command was dispatched to capture the complete system topology into an immutable serialization asset.

```bash
curl -i -X POST http://127.0.0.1:8088/system/backup/export \
  -H "X-Kirra-Admin-Token: [REDACTED]" \
  -o backup.json
```

### Transmission Payload Analysis:
 * **HTTP Response**: 200 OK
 * **Content Type**: application/json
 * **Payload Volume**: Validated full encapsulation of the nodes, dependencies, and posture_events matrices, completely bypassing live SQLite database file-system contention.

---

## Step 3 — Simulated Primary Failure

A catastrophic local control plane disruption was induced by hard-terminating the primary service listener thread without an elegant socket teardown.

### Operational Sequence:
 1. **Primary Terminated**: `kill -9` sent directly to the process listening on port 8088.
 2. **Standby Promoted**: The orchestration controller intercepted the primary dropout signal.
 3. **Runtime Reconfiguration**: The environment profile on the target secondary environment was updated to `KIRRA_VERIFIER_MODE=active`. The application reactor on port 8089 hot-reloaded the variable, lifting the passive-mode mutation shield.

---

## Step 4 — Import Snapshot Into Promoted Secondary

The serialized JSON state snapshot captured from the dead primary node was piped directly into the administration tree of the newly promoted secondary endpoint.

```bash
curl -i -X POST http://127.0.0.1:8089/system/backup/import \
  -H "Content-Type: application/json" \
  -H "X-Kirra-Admin-Token: [REDACTED]" \
  --data-binary @backup.json
```

### Ingestion Response:
 * **HTTP Status Code**: 200 OK
 * **Database Action**: The internal engine invoked an atomic rusqlite transaction block, purging local tables completely before re-populating structures from the validated snapshot.

---

## Step 5 — Post-Restore Validation

A rigorous integration test suite was immediately evaluated against the promoted instance on port 8089 to confirm structural and cryptographic parity.

### Verification Checklist:
 * [x] **/ready returned 200**: The readiness handler successfully executed an internal `SELECT 1` loop, proving the SQLite connection pool survived the restoration write sequence.
 * [x] **Posture Graph Identical**: Querying `/fleet/posture` yielded a calculation match identical to the pre-failure primary state machine footprint.
 * [x] **Dependency DAG Restored**: The White/Gray/Black topological graph layers re-hydrated cleanly into the concurrent DashMap cache, preserving the structural relationship between `node-01` and `node-02`.
 * [x] **Posture History Restored**: Time-series logs inside the `posture_events` table completely re-populated, preserving the original historical audit markers.
 * [x] **Pending Challenges Empty**: The in-memory `pending_challenges` cache on the new instance registered a zero count, confirming full volatile memory purging.
 * [x] **Imported Nodes Forced to Fresh Attestation**: Because `last_nonce` vectors were intentionally stripped during memory ingestion, both `node-01` and `node-02` were dropped back into an unverified state.
 * [x] **No Stale Nonce Reuse Possible**: Replay immunity confirmed; all historic nonces are dead on arrival, forcing edge nodes to execute a brand new cryptographic challenge-response loop.

---

## Negative Test — Rollback Verification

To ensure that the ingestion pipeline fails closed under corrupt operational scenarios, an intentional data corruption payload was passed to the restoration handler.

### Execution Vector:
A backup file was manipulated to introduce a structural truncation—specifically deleting the required posture and `created_at_ms` parameters from an entry in the `posture_events` array—before being posted to the server.

### Results:
 1. **Parsing Interception**: The hardened deserializer encountered the missing fields and threw an explicit validation error, rejecting the payload.
 2. **Transaction Aborted**: The SQLite transaction caught the parse failure before committing, halting the operation.
 3. **Zero Partial Writes Observed**: On-disk state inspection confirmed a flawless rollback. No truncated rows or empty metadata schemas broke into the storage engine.
 4. **State Integrity Intact**: The existing, valid verifier memory structures and database entries remained completely unchanged, validating absolute transaction isolation boundaries.
