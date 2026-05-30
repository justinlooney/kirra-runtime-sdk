# KIRRA runtime-SDK — Security/Safety Review Wrap-Up

Document ID: KIRRA-REV-001
Version: 1.0
Status: Final
Date: 2026-05-30

Branch reviewed: `claude/new-session-Hd04V` (merged to `main` per-slice after verification).
Method: static + manual review of the public repo, with targeted live runtime probes (curl against a running primary) for the claims that static analysis couldn't settle. The reviewer could not compile/run the workspace in-sandbox; all `cargo build/test` ran in your CI. Each fix was verified by re-fetching the pushed commit and inspecting the diff before blessing the merge.

---

## 1. What was hardened (landed + verified)

**Comparator / divergence (CERT-006)**
- Posture-aware, speed-gated divergence escalation with leaky-bucket accumulator + audit sink, replacing an unsafe hard-stop-at-speed. (`e40d18c`)
- Two-axis (linear + angular) divergence detection + `ClampMotion` graceful degrade, fixing angular-blindness. (`85b8121`)

**RSS / kinematics math**
- RSS fail-safe on invalid/zero-divisor params (`NaN.max(0.0) = 0.0` in the unsafe direction). (`8619ac7`)
- `EnforcementAction::ClampMotion{linear, angular}` multi-axis variant + apply-site. (`3bf3b46`)

**Audit chain (tamper-evidence)**
- Routed all production posture writes through the chained (audit-linked) writer; gated the plain writer `#[cfg(test)]` so the bypass cannot be reintroduced (compiler-enforced). (`aa8462c`)
- Bound `event_type` + a per-entry `sequence` into the record hash via a **versioned, non-destructive** migration (v1 rows keep verifying; v2 catches relabeling/reorder with the cheap hash-only check). Also fixed a genesis-fork-on-read-error and removed a fragile re-query in verify. (`6f25437`)
- Wired the one-time `HASH_V2_MIGRATION` anchor into startup (active-only, post-signing-key, logged). (`55e3e9e`)

**Gateway posture gate**
- Mounted the posture-routing gate as the real outermost layer on the assembled app — it was classified, tested, and **unwired**. (`de06016`)

**HA / split-brain**
- Replaced the illusory in-memory `compare_exchange` "guard" with a durable **epoch fence**: conditional-CAS promotion, held-epoch check at the mutation gate with self-demote, heartbeat-aware startup claim. Invariant: at most one effective writer across partition/skew/restart. (`56e1fa7`)

**Posture / decision engine** (was effectively dead on a normal primary)
- Read path: actuator reads routed through `resolve_posture_with_reason` so stale/empty/poisoned cache fails closed to LockedOut (TTL now enforced). (`d21e3e8`)
- Write path: generation-monotonic cache replace + audit-committed gating of cache/broadcast (enforces "no broadcast without committed audit"). (`9622910`)
- Freshness: initial recalc before serve + serialized worker + periodic recompute-and-restamp liveness loop + fault handler routed through the worker. **Resurrected a primary that previously 503'd all functional traffic** because the cache was never populated. (`6d03c4f`)

**Protocol adapters**
- CanOpen NMT-offline now fires a recalc (honest partial — node-mapping gap tracked); all three industrial handlers' audit writes made loud; Modbus `encode_response` guarded against under-length frames (panic-free). (`7cb40b5`)

**Kinematics safety perimeter**
- Bounded the actuator-command body read (`to_bytes(usize::MAX)` → 16 KiB cap, `StatusCode::PAYLOAD_TOO_LARGE` / HTTP 413 on exceed) to prevent unbounded-allocation DoS; `DenyBreach` audit made loud. (`a9c4b54`)

**Fabric (distributed enforcement)**
- Fail-closed registration seed (Nominal → Degraded, per-profile MRC, `UNVERIFIED_PENDING_FIRST_POSTURE`) and automatic bounded cross-asset propagation on posture change (non-recursive: rules fire only on LockedOut sources, changes only set Degraded). (`255a512`)

---

## 2. The dominant theme

The recurring finding was **not** bad logic — the safety logic was consistently high quality and fail-closed where it counted. The recurring finding was **wiring**: correct, tested components that were never connected to the assembled binary, or connected only advisorily. Examples: the posture-routing gate (unwired), the entire posture-freshness subsystem (worker + watchdog + TTL resolver, all unwired — leaving a normal primary inoperable), the CanOpen recalc flag (computed, audited, never acted on), the fabric clamp (computed, reported `allowed:true`, never applied).

**Consequence:** the single highest-leverage open follow-up is **#72 — extract `build_app` so integration tests exercise the real assembled router.** Without it, every "wired it up" fix in this pass can silently regress, because the green test suite tests representative copies, not the binary that ships. That one piece of test infrastructure is the durable guard against the entire class of bug this review kept surfacing.

---

## 3. Open design decisions (need a human call, not a mechanical fix)

| # | Decision | Why it's a decision, not a patch |
|---|----------|----------------------------------|
| #82 | Does the **verifier ingest RSS** (endpoint + trigger link), or is RSS purely parko's in-vehicle job? Plus telemetry-watchdog wiring. | `apply_rss_state`/`rss_active_violation` exist but nothing sets them in the service today; wiring requires knowing the intended RSS data source. |
| #86 | Fabric command endpoint: **authoritative** (must enforce clamps server-side) or **advisory** (must then not return `allowed:true` for a clamp)? | Today it reports clamps as allowed without applying them — a fail-open unless the client honors the action field. |
| #87 | Causal log: **forensic artifact** (needs chaining + full-field signing + verification + persistence) or **best-effort buffer** (drop the write-only signing)? | Signed but never verified; signature omits the causality edges; flat Vec; in-memory/unbounded. |
| #88 | Build the **verifier→fabric posture feed**. | Confirmed absent. This is why the fabric registration seed is the *interim* Degraded, not the eventual LockedOut. The seed's correctness as a fail-closed default is blocked on this. |

---

## 4. Follow-up ledger

| # | Item | Severity | Status |
|---|------|----------|--------|
| #72 | Extract in-binary `build_app` for real-assembly integration tests | **High (leverage)** | Open |
| #73 | Attestation uses single shared `KIRRA_ADMIN_TOKEN` HMAC, no per-node/TPM | Med-High | Open |
| #74 | `PRAGMA synchronous=NORMAL` weakens power-loss durability | Medium | Open |
| #76 | Audit key-rotation verification broken (single-key verify; signing key not swapped) | Med-High | Open |
| #77 | Audit tail-truncation/deletion not detected (no signed external head) | Medium | Open |
| #78 | Ensure migration anchor on PassiveStandby→Active promotion | Medium | Open |
| #79 | Gate-level epoch TOCTOU — epoch-in-write-transaction for top-tier writes | Medium | Open |
| #80 | Clock-skew can defeat failover (liveness) | Medium | Open |
| #81 | Full leader election / startup re-arbitration (zero spurious failovers) | Low-Med | Open |
| #82 | RSS ingestion architecture + telemetry-watchdog wiring | **Decision** | Open |
| #83 | Standby→Active promotion freshness lifecycle | Medium | Open |
| #84 | CANopen→fleet-node mapping (so NMT-offline recalc is effectful) | Medium | Open |
| #85 | Legacy `map_industrial_event_to_claim` fabricates token velocities | Low | Open |
| #86 | Fabric command endpoint clamp enforcement | **Decision** | Filed |
| #87 | Causal log rigor (chain/sign/verify/persist vs. buffer) | **Decision** | Filed |
| #88 | Verifier→fabric posture feed integration | **Decision** | Filed |

---

## 5. Honest assessment & residual risk

**Strong now:**
- The kinematics safety perimeter (`policy_layer` + `gateway/kinematics_contract`) is genuinely safety-grade: comprehensive float-trap handling, zero/negative-`dt` rejection, a progressive-correction pipeline that won't let one clamp suppress a later violation, posture-gated envelopes, and clamps that actually rewrite the egress command.
- The trust/posture engine now functions (it didn't — a normal primary denied everything).
- The HA path has a real durable fence (it didn't — the prior guard was per-process and illusory).
- The main audit chain is meaningfully tamper-evident for mutation/insertion and (post-v2) relabeling/reorder.

**Residual risk, in priority order:**
1. **Test-vs-production gap (#72).** Until the assembled app is integration-tested, the wiring fixes are guarded only by manual/runtime checks. This is the top structural risk.
2. **Open integration decisions (#82, #88).** RSS escalation and the verifier→fabric posture feed are both absent; until resolved, RSS violations don't escalate posture in the service, and fabric enforcement runs against manually-set/stale postures.
3. **Audit completeness gaps (#76, #77, #87).** Key-rotation verification is broken; tail-truncation/deletion is undetectable; the causal log is not a real chain.
4. **Attestation trust model (#73).** A single shared admin token can attest any node Trusted — fine if documented as admin-asserted, a real gap if per-node attestation is intended.

**Scope limits of this review:** it was static + manual + targeted runtime probes, not a compiling/fuzzing/property-test pass and not a third-party security audit. Modules touched only glancingly or not at all: `scenario_runner`, `kinematics_sim`, `fabric/asset.rs`, `fabric/telemetry.rs`, `ffi.rs`, and the non-verifier bins. The `parko` subworkspace (the in-vehicle runtime) was reviewed only where it intersected the comparator/RSS work.

**Recommended next steps (in order):**
1. Land **#72** (assembled-app integration tests) — it protects everything else.
2. Resolve the two integration decisions **#82** and **#88** (they gate real behavior).
3. Close the audit gaps **#76 / #77**, then decide **#87**.
4. Consider an external security audit + a fuzzing/proptest pass over the protocol adapters and the kinematics contract before any safety-certification milestone.
