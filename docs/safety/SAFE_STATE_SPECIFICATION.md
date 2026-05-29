# Kirra Safe State Specification

Document ID: KIRRA-SSS-001
Version: 1.0
Status: Active
Standard: ISO 26262 ASIL-D
Date: 2026-05-29

## 1. Overview

A safe state is a system state in which no unreasonable risk exists.
When Kirra detects a fault condition, it transitions to the appropriate
safe state based on fault severity. This document specifies each safe
state, its trigger conditions, behavior, and recovery path.

This document covers all 16 safety goals (SG-001 through SG-016) as
defined in `docs/safety/SAFETY_GOALS.md`. For the 11 goals without test
coverage, see `docs/safety/RTM_GAP_REPORT.md` and
`tests/cert_003_rtm_gap_stubs.rs`.

## 2. Safe States

### SS-001: Normal Operation (PostureState::Nominal)

Behavior:
  Full kinematic envelope. 35.0 m/s velocity ceiling. Stricter
  acceleration rate-limit applied by `KirraGovernor` nominal profile.
  All commands forwarded to actuators subject to kinematic limits.

Entry conditions:
  - All nodes trusted
  - No RSS violation (gap ≥ `longitudinal_safe_distance`)
  - Governor reachable
  - All startup invariants satisfied (`startup_sentinel` passed)
  - No active telemetry timeout

Exit conditions: Any fault trigger in SS-002, SS-003, or SS-004.

Safety goals covered: SG-001, SG-002, SG-004, SG-005, SG-011

---

### SS-002: Minimum Risk Condition (PostureState::Degraded)

Behavior:
  `MRC_VELOCITY_CEILING_MPS` (5.0 m/s) cap applied by `KirraGovernor`
  `apply_mrc_profile()`. System continues operating in reduced-capability
  mode. Commands forwarded but velocity capped at 5.0 m/s.

Entry conditions (any of):
  - SG-003: Sensor telemetry timeout (`AV_TELEMETRY_TIMEOUT_MS` exceeded)
  - SG-005: RSS violation (gap < `longitudinal_safe_distance`)
  - Governor unreachable (timeout or network partition)
  - Node trust state `Untrusted` with non-critical dependency impact
  - `Degraded` posture propagated from dependency graph

Recovery:
  `AV_RECOVERY_STREAK_THRESHOLD` (5) consecutive clean ticks within
  `AV_RECOVERY_WINDOW_MS` (10,000 ms) → transitions to SS-001 Nominal.
  A single unhealthy report or gap in the window resets streak to 0.
  (SG-013)

Implements: ISO 26262 safe state for recoverable faults.

Safety goals covered: SG-003, SG-005, SG-013

---

### SS-003: Lockout / Hard Stop (PostureState::LockedOut)

Behavior:
  0.0 m/s hard stop. No commands forwarded to actuators under any
  circumstance. Human intervention required to clear.
  `LockedOut` dominates all other posture states.

Entry conditions (any of):
  - DAG cycle detected in dependency graph
  - Multiple critical nodes `Untrusted` simultaneously (DAG propagation)
  - `MAX_DEPENDENCY_DEPTH` (10) exceeded in recursive DAG traversal
  - `GovernorComparator` divergence detected (CERT-006)
  - Leader `LockedOut` → followers `Degraded` within one fabric tick
    ≤ 500 ms; propagation recorded in fabric causal log (SG-007)

Recovery:
  Explicit human-initiated reset via `KIRRA_SUPERVISOR_RESET_KEY`
  endpoint. Automatic recovery from `LockedOut` is NOT permitted
  under any circumstances. Human must verify system state before
  issuing reset.

Implements: ISO 26262 safe state for non-recoverable faults.

Safety goals covered: SG-007, SG-011 (partial)

---

### SS-004: Process Fail-Closed (`startup_sentinel` abort)

Behavior:
  Process does not start. TCP listener never binds. No commands
  accepted. System remains completely offline until invariant
  violation is corrected and process restarted.

Entry conditions (checked by `startup_sentinel` before bind):
  - `KIRRA_ADMIN_TOKEN` absent or empty (SG-008, SG-015)
  - `KIRRA_SUPERVISOR_RESET_KEY` absent, empty, or > 64 bytes
  - Watchdog thread fails to start
  - Posture engine fails to initialize
  - SQLite WAL mode fails to activate
  - DDS actuator topic configured with `TransientLocal` (SG-016)
  - Any startup invariant listed in CRITICAL SECURITY INVARIANTS

Recovery:
  Fix the invariant violation. Restart the process.
  `startup_sentinel` re-runs all checks on every startup.

Implements: ISO 26262 fail-safe startup behavior. Ensures the system
never enters a partially-initialized state that could accept commands.

Safety goals covered: SG-008, SG-015, SG-016

---

## 3. Fault to Safe State Mapping

| Fault Mode | Safety Goal | ASIL | Safe State | Recovery | Test Status |
|------------|------------|------|-----------|---------|-------------|
| Linear velocity command exceeds `max_speed_mps` of active kinematic contract | SG-001 | D | SS-001 (clamp via `validate_vehicle_command` Priority 2; command continues post-clamp) | Automatic — clamp applied per command, no state transition | ✓ `test_speed_above_ceiling_triggers_clamp_linear` |
| Vehicle command implies lateral acceleration above `max_lateral_accel_mps2` (bicycle model) | SG-002 | D | SS-001 (clamp steering via `validate_vehicle_command` Priority 6) | Automatic — steering clamped per command | ✓ `test_nominal_highway_speed_high_steering_clamps_steering` |
| AV sensor node silent ≥ `AV_TELEMETRY_TIMEOUT_MS` (2,000 ms) | SG-003 | D | SS-002 — node marked `Untrusted`, posture recalculated within `AV_WATCHDOG_SWEEP_MS` (100 ms) | SS-002 → SS-001 via SG-013 recovery hysteresis | PENDING — stub: `test_safety_goal_sg_003_sensor_timeout_fault_detection` |
| Non-finite (NaN / Inf) value in any f64 field of vehicle command | SG-004 | C | SS-001 (command rejected at Priority 0; arithmetic never executed; posture unaffected) | Automatic — single-command rejection, system continues | ✓ `test_inf_linear_velocity_is_denied` |
| Posture cache age ≥ `POSTURE_CACHE_TTL_MS` (5,000 ms) at command-evaluation time | SG-005 | D | SS-003 — `resolve_posture_with_reason` returns `LockedOut(PostureCacheStale)`; all commands fail-closed | Cache refresh by next successful recalculation cycle returns posture to SS-001 / SS-002 | ✓ `test_stale_cache_fails_closed_after_virtual_clock_advance` |
| `OperationalCommand::Unknown` received (unrecognized path + method) | SG-006 | D | SS-001 (single request denied unconditionally before posture eval; fleet posture unchanged) | Automatic — per-request denial, no state transition | PENDING — stub: `test_safety_goal_sg_006_unknown_command_denial` |
| Leader asset enters `LockedOut` in multi-asset fabric | SG-007 | D | SS-003 (leader) + SS-002 (all followers, within one fabric tick ≤ 500 ms); propagation logged in fabric causal log | Leader recovery via human reset → fabric tick restores followers to SS-001 | PENDING — stub: `test_safety_goal_sg_007_cross_asset_lockout_propagation` |
| `startup_sentinel` invariant failure at boot (token, watchdog, posture engine, WAL, DDS durability) | SG-008 | D | SS-004 — process aborts before TCP listener binds; no command surface exposed | Fix invariant violation; restart process; `startup_sentinel` re-runs | PENDING — stub: `test_safety_goal_sg_008_process_fail_closed_on_crash` |
| Primary heartbeat silent ≥ `PROMOTION_TIMEOUT_MS` (10,000 ms) in HA deployment | SG-009 | B | SS-001 maintained via standby promotion (`mode_active.compare_exchange`); enforcement coverage gap bounded by promotion timeout | Promoted standby begins heartbeat + posture recalculation immediately on promotion | PENDING — stub: `test_safety_goal_sg_009_ha_standby_promotion_within_timeout` |
| Audit chain entry `prev_hash` mismatch detected during verification | SG-010 | B | SS-001 maintained, integrity-failure flag raised for operator review; chain logged; service continues | Operator-driven forensic review; no automatic recovery of tampered entries | PENDING — stub: `test_safety_goal_sg_010_audit_chain_tamper_detection` |
| CANOpen NMT command with `data[0]` in `{0x02, 0x80, 0x81, 0x82}` | SG-011 | C | Posture engine triggered to recalculate; result places system in SS-001 / SS-002 / SS-003 based on resulting fleet posture | Per recovery rules of resulting safe state | ✓ `test_canopen_nmt_stop_triggers_posture_recalculation` (1 of 3 — partial coverage) |
| DNP3 message to `DNP3_BROADCAST_ADDRESS` received | SG-012 | B | SS-001 normally — audit chain entry written before control output; if audit write fails, control blocked (SS-001 with denied command) | Automatic — audit-before-action ordering enforced per request | PENDING — stub: `test_safety_goal_sg_012_dnp3_broadcast_mandatory_audit` |
| Recovery hysteresis evaluation for recently-faulted node | SG-013 | B | SS-002 → SS-001 transition only when 5 consecutive healthy reports arrive inside a 10 s window; otherwise streak resets | Per the streak / window rule itself; no other recovery path | PENDING — stub: `test_safety_goal_sg_013_recovery_hysteresis_streak_and_window` |
| `FederatedTrustReportV2` with `generation ≤ last_accepted_generation` from same peer, or replayed nonce | SG-014 | B | SS-001 maintained — report rejected; rejection logged to audit chain; posture unchanged | Automatic — replay rejection per request, no state transition | PENDING — stub: `test_safety_goal_sg_014_federation_report_replay_prevention` |
| `KIRRA_ADMIN_TOKEN` absent or empty at mutation route invocation | SG-015 | B | SS-004 if absent at startup (process never binds); HTTP 503 if absent at request time (SS-001 with denied request) | Provide token via environment; restart if previously missing at startup | PENDING — stub: `test_safety_goal_sg_015_admin_token_absent_fail_closed` |
| DDS actuator topic detected with `DurabilityPolicy::TransientLocal` at startup | SG-016 | C | SS-004 — `startup_sentinel` aborts; process never binds | Reconfigure topic to `DurabilityPolicy::Volatile`; restart | PENDING — stub: `test_safety_goal_sg_016_dds_actuator_volatile_durability` |

---

## 4. Safe State Transition Invariants

The following invariants are enforced in code and must never be
violated regardless of what upstream AI systems instruct:

1.  `LockedOut` can only be cleared by human reset — never automatic
2.  `Degraded` recovery requires N consecutive clean ticks — not immediate
3.  Governor unreachable → `Degraded` semantics (NOT `LockedOut`)
4.  RSS unsafe → `Degraded` semantics (NOT `LockedOut`)
5.  NaN / Inf model output → safe floor applied before governor runs
6.  DAG `LockedOut` propagates upward — never downgraded by RSS recovery
7.  `LockedOut` dominates `Degraded` — if both conditions present,
    `LockedOut` wins
8.  `startup_sentinel` abort → no commands accepted under any circumstance
9.  `OperationalCommand::Unknown` denied in ALL posture states (SG-006)
    — before posture check, before governor, unconditionally
10. DDS actuator topics must use `DurabilityPolicy::Volatile` only —
    `TransientLocal` triggers `startup_sentinel` abort (SG-016)
11. SQLite writes go to disk before memory — `persist_and_insert_node`
    calls `save_node` then `nodes.insert`, never reversed
12. `KIRRA_ADMIN_TOKEN` compared with `constant_time_compare` only —
    standard `==` forbidden on security-critical byte sequences

---

## 5. Open Items (CERT-003 Gaps)

The following safety goals have documented stubs but no implemented
test coverage. Each is tracked in `tests/cert_003_rtm_gap_stubs.rs`.
This section shrinks as CERT-004 implements each test.

| Goal | ASIL | Property to verify | Stub function | Mapped safe state |
|---|---|---|---|---|
| SG-003 | D | Watchdog marks node `Untrusted` within `AV_TELEMETRY_TIMEOUT_MS + AV_WATCHDOG_SWEEP_MS` of last telemetry; `PostureRecalcTrigger` fired same cycle | `test_safety_goal_sg_003_sensor_timeout_fault_detection` | SS-002 |
| SG-006 | D | `should_route_command` denies `Unknown` unconditionally before posture eval in all three posture states | `test_safety_goal_sg_006_unknown_command_denial` | SS-001 (per-request denial) |
| SG-007 | D | Leader `LockedOut` → followers `Degraded` within one fabric tick ≤ 500 ms; event logged in fabric causal log | `test_safety_goal_sg_007_cross_asset_lockout_propagation` | SS-003 (leader) + SS-002 (followers) |
| SG-008 | D | `startup_sentinel` aborts before listener bind on any invariant failure (token, watchdog, posture engine, WAL, DDS durability) | `test_safety_goal_sg_008_process_fail_closed_on_crash` | SS-004 |
| SG-009 | B | Standby promotes (`mode_active.compare_exchange`) within `PROMOTION_TIMEOUT_MS` (10 s) of last primary heartbeat; promoted instance resumes heartbeat + recalculation | `test_safety_goal_sg_009_ha_standby_promotion_within_timeout` | SS-001 (maintained via standby) |
| SG-010 | B | `AuditChainLinker` detects any tampered entry via `prev_hash` mismatch; `/system/audit/verify` returns first bad index; verification runs at startup | `test_safety_goal_sg_010_audit_chain_tamper_detection` | SS-001 with integrity flag |
| SG-012 | B | DNP3 broadcast → audit chain entry written before control output; audit write failure on broadcast blocks control (fail-closed ordering) | `test_safety_goal_sg_012_dnp3_broadcast_mandatory_audit` | SS-001 (per-request) |
| SG-013 | B | `evaluate_recovery_report` requires exactly 5 healthy reports inside a 10 s window; gap or unhealthy report resets streak to 0 | `test_safety_goal_sg_013_recovery_hysteresis_streak_and_window` | SS-002 → SS-001 transition |
| SG-014 | B | `reconcile_reports` rejects replayed `FederatedTrustReportV2`; Ed25519 signature verified; nonces burned in `federation_report_nonces` | `test_safety_goal_sg_014_federation_report_replay_prevention` | SS-001 (per-request rejection) |
| SG-015 | B | `require_admin_token` returns HTTP 503 when `KIRRA_ADMIN_TOKEN` absent / empty; comparison uses `constant_time_compare`; reaches every mutation handler | `test_safety_goal_sg_015_admin_token_absent_fail_closed` | SS-004 at startup / SS-001 at request time |
| SG-016 | C | Every DDS actuator topic uses `DurabilityPolicy::Volatile`; `startup_sentinel` aborts on `TransientLocal` | `test_safety_goal_sg_016_dds_actuator_volatile_durability` | SS-004 |

---

## 6. Implementation References

- `PostureState` enum: `src/posture_engine.rs` (or `src/verifier.rs`)
- `KirraGovernor` authority model: `parko/parko-kirra/src/lib.rs`
- `startup_sentinel`: `src/bin/kirra_verifier_service.rs`
- `should_route_command`: `src/posture_cache.rs`
- DDS bridge: `src/dds_bridge.rs`
- ADL-001 (governor authority model): `work/decisions.md`
- Safety goals: `docs/safety/SAFETY_GOALS.md`
- RTM gap report: `docs/safety/RTM_GAP_REPORT.md`
- Test stubs: `tests/cert_003_rtm_gap_stubs.rs`
