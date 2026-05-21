# Aegis v1.0.0 — TPM Attestation Verification Specification

This document provides the definitive mathematical and structural specification for the Aegis v1.0.0 Trusted Platform Module (TPM) remote attestation architecture. It details the precise tracking mechanics that ensure the control plane relies exclusively on unmockable, cryptographically validated platform state assertions rather than transient network configurations.

---

## 1. Attestation Trust Model

The Aegis security model operates on a zero-trust network posture. Network layer identifiers (such as IP addresses, MAC coordinates, or transport-layer sessions) are treated as hostile, modifiable vectors.

> **Root Security Invariant**: Aegis does not trust network identity. Aegis trusts measured platform state.

Trust is established by building a rigid cryptographic chain of custody from physical silicon registers up to cluster-wide policy enforcement:

```
TPM Hardware Silicon Registers
  → Attestation Identity Key (AIK)
  → Signed Cryptographic Quote
  → PCR16 Runtime State Measurement
  → Verifier Validation Engine
  → FleetPosture Topology Promotion
```

* **Node Identity Binding**: A node's persistent operational identity is securely bound to the unique public component of its asymmetric Attestation Identity Key (AIK), configured during device enrollment.
* **Runtime Legitimacy Binding**: A node's operational integrity is determined exclusively by verifying its runtime software stack against binary measurements inside Platform Configuration Register 16 (`PCR16`). The resulting SHA-256 digest must match the verifier's expected baseline.
* **Temporal Freshness Binding**: Replay immunity is enforced by encapsulating every remote evaluation cycle within an explicit, short-lived, single-use cryptographic challenge-response loop.

---

## 2. End-to-End Verification Sequence

The runtime verification sequence is executed via an atomic transaction chain spanning the edge node, the storage engine, and the runtime state machine.

| Step | Executing Component | Targeted Action Description |
| :---: | :--- | :--- |
| **1** | Edge Client Node | Requests a new temporal challenge token from the verifier endpoint: `POST /attestation/challenge/:node_id`. |
| **2** | Verifier Control Engine | Generates a high-entropy, cryptographically secure random nonce using an underlying CSPRNG pool. |
| **3** | Volatile Memory Cache | Caches the generated nonce inside `pending_challenges`, indexed by `node_id`, and stamps it with an expiration timestamp bound by `CHALLENGE_TTL_MS`. |
| **4** | Client Hardware TPM | Executes an internal `TPM2_Quote` operation, reading the live runtime configuration registers (`PCR16`), embedding the verifier's nonce inside the quote body, and signing the complete block using the private AIK. |
| **5** | Edge Client Node | Submits the structured quote payload, binary signature, and node parameters back to the verifier: `POST /attestation/verify`. |
| **6** | Verifier Control Engine | **DESTRUCTIVELY CONSUMES THE NONCE BEFORE ANY VALIDATION STARTS.** The verifier pops the `node_id` entry from `pending_challenges`. If the nonce does not exist or has expired, processing aborts instantly. |
| **7** | Persistent Data Layer | Pulls the absolute reference AIK public key PEM structure out of the secure, persistent SQLite data table. |
| **8** | Cryptographic Engine | Parses the public key and validates the incoming signature against the immutable byte payload of the raw TPM quote block. |
| **9** | Cryptographic Engine | Parses the validated quote body, extracts the raw binary values of the `PCR16` registers, and passes them through a local SHA-256 calculation. |
| **10** | Validation Engine | Executes a binary comparison matching the freshly calculated SHA-256 digest against the node's registered `expected_pcr16_digest_hex` read out of the database store. |
| **11** | Concurrent Cache | Toggles the active memory cache layer from `NodeTrustState::Unknown` to `NodeTrustState::Trusted`. |
| **12** | DAG Topo Engine | Triggers a recursive graph recalculation traversing the White/Gray/Black dependency pathways affected by the trust state transition. |
| **13** | Edge Proxy Gateway | The updated `FleetPosture` state is fetched during asynchronous polling sweeps, altering global route authorization constraints instantly. |

> **CRITICAL INVARIANT**: Nonce consumption occurs before cryptographic validation. If a quote submission fails verification, its associated nonce is already destroyed. The client cannot attempt alternative signature iterations against the same challenge window.

---

## 3. Replay Protection Guarantees

The verification pipeline implements a multi-tiered defense matrix designed to ensure that compromised historical payloads are rendered cryptographically inert if intercepted by a malicious observer.

* **Single-Use Lifetime Constraint**: A nonce exists for exactly one extraction pass. Once popped from the active `pending_challenges` map during a validation entry request, it cannot be reclaimed, reused, or re-inserted.
* **Volatile Memory Isolation**: Nonce tracking structures are maintained strictly in volatile RAM. They are never written to the persistent SQLite layer, removing the risk of post-crash state residue leakage.
* **Temporal Windows (`CHALLENGE_TTL_MS`)**: Active challenges enforce an uncompromising expiration limit. If a client fails to return a signed quote before the lookback window closes, the memory slot is cleared.
* **Destructive Restoration Cleansing**: Invoking disaster recovery data ingest routines (`POST /system/backup/import`) triggers an instantaneous, global clear of the `pending_challenges` map.
* **Process Lifetime Coherence**: Restarting the verifier process completely invalidates all active challenge windows globally.

```
Historic Attestation Packets + Destroyed/Missing Nonce → Instant 401 Unauthorized / Reject
```

---

## 4. PCR16 Measurement Binding

A valid cryptographic signature only proves that a packet originated from an authorized TPM. Real legitimacy requires proving that the platform is running an uncorrupted software environment. `PCR16` acts as the definitive anchor for runtime state validation.

### Verification Failure Matrix

The verifier asserts five distinct evaluation checks to determine legitimacy. If any condition fails, the system defaults to a fail-closed response:

| Encountered Processing Condition | Structural Result | Imposed State Consequences |
| :--- | :---: | :--- |
| Asymmetric Signature Invalid / Broken | **Untrusted** | Node status set to `Untrusted`. Topological dependencies break instantly. |
| `PCR16` Digest Hash Mismatch | **Untrusted** | Node status set to `Untrusted`. Triggering instant downstream containment. |
| Challenge Nonce Missing from Cache | **Reject** | Aborts processing immediately with HTTP 400. Existing cache states are preserved. |
| Challenge Nonce Expired (`> CHALLENGE_TTL_MS`) | **Reject** | Aborts processing immediately with HTTP 400. Clears the expired challenge entry. |
| Unregistered / Unknown `node_id` | **Reject** | Aborts processing immediately with HTTP 404. No state processing occurs. |

---

## 5. TPM Simulation Evidence

To enable comprehensive integration testing within automated CI/CD environment runners and local engineering workspaces without requiring a physical hardware cryptographic chip attachment, the platform relies on synthetic TPM simulation frameworks.

* **Mock TPM Quote Architecture**: Test harnesses simulate a hardware state environment by generating binary blocks matching the precise schema serialization specifications of true TPM2.0 quotes.
* **Fixed Key Fixtures**: Tests leverage synthetic AIK asymmetric PEM key pairs to sign test assertions, validating that the underlying OpenSSL or Ring cryptographic modules execute accurate verification passes.
* **Synthetic PCR Vectors**: Integration matrices specify realistic `PCR16` register tracking sequences, generating known-good SHA-256 target digests alongside intentionally flawed test entries.

> **Core Testing Invariant**: Mock vectors simulate hardware behavior. They do NOT bypass cryptographic verification logic. The validation code runs identical parsing, signature evaluation, and hash comparison steps regardless of whether it interacts with a physical or a simulated TPM chip.

---

## 6. Posture Promotion Rules

The system maps local node attestation states directly to systemic global authorization metrics through recursive Directed Acyclic Graph (DAG) inheritance passes:

| Verified `NodeTrustState` | Structural Topology State | Evaluated `FleetPosture` Result |
| :--- | :--- | :---: |
| `Trusted` | All dependent parent chains are `Trusted` & healthy | **Nominal** |
| `Trusted` | At least one parent dependency is `Untrusted` / `Unknown` | **Degraded** |
| `Untrusted` | Irrespective of downstream dependency layouts | **LockedOut** |
| `Unknown` | Base state upon fresh registration or system boot | **Degraded** |

```text
  [node-02: Untrusted] ── (Breaks Chain) ──> [node-01: Trusted]
                         │
                         ▼
             Global Posture = Degraded
```

---

## 7. Forbidden Cryptographic Regressions

> **CRITICAL CRYPTOGRAPHIC ENFORCEMENT PROTECTION RULES**

 * **Do not hardcode `NodeTrustState::Trusted`.** Trust status must be earned through successful cryptographic verification on every cycle; static validation short-circuits are forbidden.
 * **Do not skip nonce consumption.** Nonces must be popped from volatile memory *before* signature and payload decoding begin to eliminate double-submission vulnerabilities.
 * **Do not persist nonces to SQLite.** Challenges must remain strictly in volatile RAM caches to prevent post-incident state exploitation.
 * **Do not trust unsigned PCR payloads.** Platform measurement configurations must be extracted strictly from inside cryptographically verified TPM quote bodies.
 * **Do not bypass AIK signature validation.** The system must calculate and verify the signature against the actual public components of the registered node record for every incoming quote.
 * **Do not accept expired challenges.** Nonce timestamp deltas must be evaluated stringently against `CHALLENGE_TTL_MS` rules on every pass.
 * **Do not evaluate posture before verification completes.** Dependency hierarchy updates must execute downstream of successful cryptographic and database checkpoint saves.
 * **Do not downgrade Untrusted into Degraded.** If a node actively fails a cryptographic challenge or measurement validation, it must trigger an absolute **LockedOut** isolation directive instantly.
