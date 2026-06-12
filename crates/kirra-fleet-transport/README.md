# kirra-fleet-transport — fleet-lane (QM) Zenoh transport spike (#296)

The **fleet lane**: vehicle ↔ ops/cloud transport for cellular / distributed
fleets. The decision lives in [`docs/adr/0007-fleet-transport-zenoh.md`](../../docs/adr/0007-fleet-transport-zenoh.md);
this crate is the spike.

## The three clauses (ADR-0007), in one paragraph

Zenoh is an **untrusted carrier** — trust derives from **Ed25519 payload
signatures** (federation reports via the SDK's
`verify_federated_report_signature_v2`; grants via this crate's
`verify_clearance_grant`), **never** from transport identity, topic name, or
Zenoh's own auth, and every ingest **verifies before use** with unsigned /
bad-signature / malformed payloads rejected and **counted** (`RejectionCounter`)
[Clause 1]. It is **strictly QM**: this crate is a **leaf** consumer that depends
on the SDK, and **nothing under `src/gateway/` or any safety path may depend on
it** — ADR-0006 Clause 2's boundary asymmetry is the parent rule [Clause 2]. The
down-lane grant **terminates at the vehicle's verifier store**: a verified grant
is written through the **existing** Phase-A path (`save_clearance_grant_chained`,
a `PENDING` row) and Phase-B's one-shot pickup + two-checkpoint delivery proceed
**unchanged** — remote transport composes with the clearance design, it never
creates a second release path [Clause 3].

## Layout

- **`lib.rs`** — the transport-free **trust + codec core**: the namespace key
  expressions, `accept_report` (decode → **verify-first**), the signed-grant
  envelope (`sign_clearance_grant` / `verify_clearance_grant`), `ingest_clearance_grant`
  (verify → existing store path), and `RejectionCounter`. Unit-tested with no
  Zenoh session.
- **`transport.rs`** — the thin Zenoh edge: `FleetPublisher`, `FleetSubscriber`
  (`recv_report` verifies before surfacing), `GrantIngest` (`recv_and_ingest`).
  Tested with two **in-process peer sessions** (no router, localhost TCP, multicast
  off).

## Build / sandbox note (the bench is the build authority)

This crate **builds and tests in the authoring sandbox** (rustc 1.94.1) — but only
with Zenoh's **TLS/QUIC features disabled** (`default-features = false`, just
`transport_tcp` + `unstable`). Zenoh's *default* TLS stack pulls an
`rcgen` / `time` dependency chain that **does not compile on rustc 1.94.1**
(`E0119`, a conflicting-impl break) — the anticipated MSRV/toolchain wall. We
disable TLS deliberately: **TLS is confidentiality defense-in-depth, not the trust
root** (the trust root is the Ed25519 payload signature), so dropping it costs no
trust for the spike. The **bench is the authority** for the TLS-enabled production
config; if a future transitive dep ever breaks the resolve, the
`zenoh-pinned-deps-1-75` lockdown is the escape hatch.

The Zenoh tests require a **multi-threaded** tokio runtime
(`#[tokio::test(flavor = "multi_thread", …)]`) — Zenoh's runtime rejects the
current-thread scheduler.

## What this spike is NOT

Router / cellular / NAT deployment (ops/router territory), QoS / `AdvancedPublisher`
delivery guarantees (named future — `zenoh-ext`), and the multi-tenant fleet side
with a per-controller key registry (#314). See the ADR's *Honest limits*.
