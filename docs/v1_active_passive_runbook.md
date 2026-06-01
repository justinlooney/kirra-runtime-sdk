# Kirra v1.0.0 — Active / PassiveStandby Operations Runbook

This document serves as the formal engineering handbook for running, promoting, and maintaining high-availability Kirra verifier node cluster infrastructure under standard and incident-response operational parameters.

---

## 1. Runtime Modes

The verifier cluster utilizes the configuration environment flag `KIRRA_VERIFIER_MODE` to designate cluster topology and prevent data corruption.

### Capability Behavior Matrix

| Capability | Active Mode | PassiveStandby Mode |
| :--- | :---: | :---: |
| Register Node Identity (`/attestation/register`) | **Yes** | No (503 Service Unavailable) |
| Issue Cryptographic Challenges (`/attestation/challenge/*`) | **Yes** | No (503 Service Unavailable) |
| Verify Attestation Quotes (`/attestation/verify`) | **Yes** | No (503 Service Unavailable) |
| Register Dependency Topologies (`/fleet/dependencies`) | **Yes** | No (503 Service Unavailable) |
| Read Fleet Posture, History, Flapping Status | **Yes** | **Yes** (Read-Only Mirroring) |
| Export JSON Backup Snapshots | **Yes** | **Yes** |
| Import JSON Backup Snapshots | **Yes** | No (503 Service Unavailable) |

---

## 2. Boot Procedures

### Active Node Initialization

To stand up a node as the primary read-write control authority, instantiate the runtime environment as follows:

```bash
export KIRRA_VERIFIER_MODE=active
export KIRRA_ADMIN_TOKEN="your-secure-token-hash"
cargo run --bin kirra_verifier_service
```

#### Expected Log Signature:
```text
[KIRRA VERIFIER] mode=Active
```

### Passive Standby Node Initialization

To stand up a node as a secure, high-performance read-only mirror capable of taking offloaded analytic queries, instantiate the runtime environment as follows:

```bash
export KIRRA_VERIFIER_MODE=passive_standby
export KIRRA_ADMIN_TOKEN="your-secure-token-hash"
cargo run --bin kirra_verifier_service
```

#### Expected Log Signature:
```text
[KIRRA VERIFIER] mode=PassiveStandby
```

---

## 3. Failure Isolation Procedure

In the event of a primary active node degradation or hardware failure, operators must execute the following isolation steps under pressure:

 1. **Detect Malfunction**: Identify liveness or database connectivity collapse via a non-200 return from the `/ready` probe endpoint on the primary active node.
 2. **Isolate Routing**: Instantly drop the degraded primary node out of the active upstream application load balancer rotation to stop edge transport traffic.
 3. **Freeze Mutation Traffic**: Block or reject incoming registration requests at the gateway proxy layer during the failover event window.
 4. **Export Final State (If Salvagable)**: If the primary process is responsive but exhibiting disk degradation, force an immediate cold snapshot dump via `POST /system/backup/export` to capture the latest time-series and dependency layouts.
 5. **Prepare Promotion Target**: Verify the backup target standby node is online, reporting a clean `/health` status, and synchronized up to its last automated checkpoint.

---

## 4. Promotion Procedure

To promote a running PassiveStandby instance into a fully mutable Active primary controller node, execute the configuration transition:

 1. **Lift the Mutation Shield**: Update the running host process environment configurations to toggle the mode boundary:
    ```bash
    export KIRRA_VERIFIER_MODE=active
    ```
 2. **Force Process Re-Initialization**: Restart or signal the service binary to boot under the `Active` context flag.
 3. **Validate Liveness & Connectivity**: Verify the node rehydrated its stored cache and can communicate with its underlying database by dispatching an unauthenticated local check:
    ```bash
    curl -i http://127.0.0.1:8089/ready
    ```

#### Expected Target Return:
```json
HTTP/1.1 200 OK
{"status":"ready"}
```

---

## 5. Restore Procedure

Once a secondary target node has been safely promoted to Active status, the operator must rehydrate the node to match the known-good cluster baseline before reconnecting production edge traffic.

### 1. Ingest State Snapshot Payload

Pipes the extracted JSON backup archive directly into the newly promoted control instance:

```bash
curl -i -X POST http://127.0.0.1:8089/system/backup/import \
  -H "X-Kirra-Admin-Token: [REDACTED]" \
  -H "Content-Type: application/json" \
  --data-binary @backup.json
```

*Note: This operation initiates a single, atomic SQLite transaction block that completely wipes local tracking structures and replaces them with the validated baseline.*

### 2. Verify Post-Restore Memory Parity

Ensure the application layer, concurrent memory tables, and persistent storage are completely unified. Operators must cross-validate:

 * **Posture Graph Parity**: Verify that querying `/fleet/posture` matches the pre-failover primary cryptographic matrix.
 * **Dependency Topology Parity**: Run targeted graph checks to confirm the structural White/Gray/Black node coloring layout matches perfectly.
 * **Posture History Parity**: Verify time-series audit rows inside the `posture_events` ledger match.
 * **Challenge Volatility Cleansing**: Verify that the volatile `pending_challenges` map is verified empty (0 entries). All previously active nonces must be dead on arrival, forcing re-entrant edge nodes to perform a fresh challenge-response attestation loop.

---

## 6. Forbidden Operator Actions

> **CRITICAL OPERATIONAL ENFORCEMENT PROTECTION RULES**

 * **Never run two Active verifiers against divergent databases.** This triggers rapid split-brain data corruption and shatters the distributed legitimacy consensus model.
 * **Never expose backup endpoints publicly.** Snapshot data contains administrative cryptographic indices and full machine layout topologies. They must remain isolated within the token-protected admin matrix.
 * **Never manually inject nonce state into SQLite.** Nonces must follow a volatile, in-memory, CSPRNG lifecycle. Forcing manually generated tokens into the state structures introduces immediate signature replay exploit windows.
 * **Never bypass `require_admin_token` during incident response.** Disabling authentication middleware to "accelerate" an emergency restoration introduces an unmitigated attack window for hostile entities to poison the topology.
 * **Never restore partial or unvalidated JSON snapshots.** If an export artifact fails checksums or is missing tracking metadata fields, it must be rejected. Partial database rehydration leaves memory tables in a broken state.
 * **Never promote a standby without validating `/ready`.** Promoting a secondary instance while its database engine is locked or its connection pool is failing creates a cascade failure scenario.
