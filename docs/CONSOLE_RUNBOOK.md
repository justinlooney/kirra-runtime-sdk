# Operator Console — Demo Runbook

> ## ⚠️ DEMO DATA — NOT EVIDENCE
> Everything this runbook seeds is a **demonstration**, not a safety artifact.
> Node ids are **`KIRRA-DEMO-*`** so they're unmistakable in any screenshot. The
> seeded audit chain is genuinely signed (so the console's verification flags are
> real), but **nothing here enters any safety case** — it is a desk demo of the
> real machinery, with synthetic inputs.

Five minutes, copy-paste, **zero hardware**. You'll seed a demo fleet, serve the
console, walk a real SG6 escalation → operator clearance → Phase-B delivery, and
reset.

---

## 1. One-time: generate an audit signing key

The seeded chain is **signed** so its verification is real (not decoration). Make
a base64 32-byte Ed25519 seed (the form `KIRRA_LOG_SIGNING_KEY` expects):

```sh
export KIRRA_LOG_SIGNING_KEY="$(openssl rand -base64 32)"
export KIRRA_DB_PATH="kirra_demo.sqlite"
export KIRRA_ADMIN_TOKEN="demo-admin"
export KIRRA_SUPERVISOR_RESET_KEY="demo-supervisor-key"
```

Keep this shell open (the seeder, the service, and the delivery example all need
the **same** `KIRRA_LOG_SIGNING_KEY` so the chain verifies end-to-end).

## 2. Seed the demo store

```sh
cargo run --bin kirra_console_demo_seed
```

This refuses to run against a non-empty store (it only initializes fresh ones) and
requires `KIRRA_LOG_SIGNING_KEY`. It writes 6 `KIRRA-DEMO-*` nodes and the
`KIRRA-DEMO-03` SG6 sequence (`RSS_VIOLATION → TRAJECTORY_MRC_FALLBACK →
ImpactDetected → ImpactEscalationRaised`) as **real, signed** chain events, then
prints the exact next command.

## 3. Serve the console

```sh
cargo run --bin kirra_verifier_service
```

Open **http://127.0.0.1:8090/console**.

## 4. Walk the demo

1. **See the fleet** — 6 tiles. `KIRRA-DEMO-03` is `Untrusted` (post-collision
   latch); `KIRRA-DEMO-05` carries the `flood_condition_active` note.
2. **Open the escalation docket** — `KIRRA-DEMO-03`'s `ImpactDetected` (vanished-
   object trigger) + `ImpactEscalationRaised`, signed, with no later clear → open.
3. **Record a clearance grant** — in the grant card enter node `KIRRA-DEMO-03`, an
   operator id, and the supervisor key `demo-supervisor-key` (the value of
   `KIRRA_SUPERVISOR_RESET_KEY`). Submit → the card shows **`GRANT RECORDED —
   DELIVERY PENDING`** and a signed `OperatorClearanceGrantIssued` row appears in
   the chain feed. **The vehicle is NOT released yet** — the grant is recorded and
   signed; delivery is the node's job.
4. **Deliver it** — in a second terminal (same env vars):

   ```sh
   cargo run -p parko-kirra --example deliver_clearance -- \
     --db "$KIRRA_DB_PATH" --node KIRRA-DEMO-03
   ```

   This is the **Phase-B loop, live** — it stands in for the parko-ros2 node tick;
   `poll_and_deliver` is the exact call the node will make. It takes the pending
   grant (one-shot), re-validates it at the `ClearanceLoop`, and prints
   **`DELIVERED · CLEARED`**.
5. **Refresh the console** — the grant card flips to **`DELIVERED · CLEARED`** and
   a signed `ClearanceDelivered` row is in the feed.

## 5. Reset

```sh
rm -f "$KIRRA_DB_PATH" "$KIRRA_DB_PATH"-wal "$KIRRA_DB_PATH"-shm
```

A fresh `KIRRA_DB_PATH` re-seeds cleanly (step 2).

---

## What you're looking at (one pane at a time)

For a first-time viewer — each pane is a window onto a **real** mechanism, not a
mock.

- **Fleet posture (tiles).** The per-node trust state straight from the verifier's
  durable store (`load_nodes`) — `Trusted` / `Untrusted(reason)`. The
  `flood_condition_active` and post-collision notes are the actual trust-state
  reasons the **posture engine** records, not labels painted on for the demo.

- **Escalation docket.** Open SG6 escalations **derived from the signed audit
  chain** — `ImpactDetected` / `ImpactEscalationRaised` events with no later
  `ImpactCleared`. These are the production event types (parko-kirra's impact
  sink); the docket is the same fail-closed view the operator sees in the field.

- **Clearance grant form.** The console's **one** authenticated affordance. The
  supervisor key is verified by `constant_time_compare` against
  `KIRRA_SUPERVISOR_RESET_KEY` (the #255 mechanism) — a bad key is a real 401.
  Well-formedness (non-empty operator, **registered** node) is checked server-side,
  and the timestamp is the **server's** clock (no client-supplied time → no
  future-dating). The grant is recorded **and cryptographically signed**; it does
  not release anything.

- **Audit chain feed.** Every row is **Ed25519-signed and hash-linked**; the
  `valid` flag is real signature verification under the configured key, and the
  masthead `chain: verified` is the hash-chain integrity check (`chain_intact`).
  The clearance lifecycle you just drove — `OperatorClearanceGrantIssued` →
  `ClearanceDelivered` — is in this same tamper-evident ledger.

- **Delivery (the example / the node tick).** The grant is taken **exactly once**
  (an atomic `UPDATE … RETURNING` — double-pickup is impossible) and re-validated
  at the `ClearanceLoop` at **delivery** time: a grant that sat too long is
  rejected at the loop even though the verifier accepted it at record time (the
  **two-checkpoint** design). A rejected/stale grant is consumed, never retried —
  the operator re-issues.

> The console is **QM** and **posture-exempt** (reachable during LockedOut — the
> posture it exists to recover from). The governor judges; the console does not.
