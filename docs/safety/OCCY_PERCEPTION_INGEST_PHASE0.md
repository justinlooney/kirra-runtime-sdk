# Occy / KIRRA — Perception Ingest, Phase-0 (Adapter-Surface Enforcement)

**Doc ID (proposed):** KIRRA-OCCY-PMON-003.
**Status:** Decided design for review. Takes Track-C from **dormant** (the merged
kinematic guard + cap-composition machinery, with no feed and `enabled = false`)
to **enforcing at the ROS2-adapter surface**. No code in this commit; this is the
spec the slice-1 implementation builds against (it also names the one source
comment-correction as an implement task, not done here).
**Scope:** the perception **ingest** — the `TrackedObject` source that feeds the
`PerceptionCapPublisher`, wired in the `kirra-ros2-adapter` process where Autoware
perception already arrives and the verdict already runs.
**Base:** verified against `main @ e5a7666` ("Merge pull request #179 …
KIRRA-OCCY-PMON-002"); evidence from the #126 scope report (cited file:line).
**References:** KIRRA-OCCY-PMON-001 (the guards + `TrackedObject`), KIRRA-OCCY-PMON-002
(the cap-composition machinery), ADR-0002 (most-conservative-wins `min`),
ADR-0004 (independent safety channel / doer–checker).

---

## 0. The one principle (why this is still Track C)

```
[sensors] → Autoware tracking/prediction (Track A/B) → PredictedObjects (~/input/objects)
         → kirra-ros2-adapter: field-remap → TrackedObject → kinematic guard (Track C)
         → published cap → apply_perception_cap → the adapter's validate_vehicle_command → actuators
```

PMON-001 built the guards; PMON-002 built the cap lifecycle + composition; both are
merged and `pub`, but **dormant** — there is no `on_tick` caller and
`perception_monitor_enabled` defaults false. This doc supplies the missing feed.

**The ingest is a pure field-remap of upstream-tracked objects — Kirra checks
perception output, builds none.** No tracker, associator, or detector is added.
The guards remain derate-only (they never author a `DenyBreach`).

**Constraints carried from PMON-001/002 (restated as gates):**

| # | Constraint | How this design holds it |
|---|------------|--------------------------|
| C1 | No perception DNN / no perception-building in Kirra | The shim only remaps fields of upstream-tracked objects (IDs + velocity from Autoware tracking). D2b (teleport, prior-frame retention) is the boundary's edge — **deferred**. D1 raw detections = a violation to reject. |
| C2 | Verdict fn byte-stable | Compose via the `pub` `apply_perception_cap` (a `min` into `odd_speed_cap_mps`), exactly as the PMON-002 merge did — `validate_vehicle_command` / `effective_max_speed_mps` / `DenyCode` **untouched**. |
| C3 | Agnostic to Track-A model choice | The guard consumes `TrackedObject` (object list); any Autoware tracker/predictor swap is adoptable without touching Kirra. |

---

## 1. The process-split this doc resolves (verified at `e5a7666`)

PMON-002 wired cap composition into the **verifier-service process** — the HTTP
middleware (`src/gateway/policy_layer.rs`, Nominal arm) and the fabric governor
(`src/fabric/governor.rs`), both reading `ServiceState.perception_cap` +
`ServiceState.perception_monitor_enabled` (`src/posture_cache.rs`), default false
(`src/bin/kirra_verifier_service.rs`).

But perception **does not arrive in the verifier-service process.** The #126 scope
found it already ingested in the **`kirra-ros2-adapter`** process:

- subscribes to Autoware **`~/input/objects` = `autoware_perception_msgs/PredictedObjects`**
  (`crates/kirra-ros2-adapter/src/node.rs:192`);
- **parses** to `PerceivedObject { id, pos, velocity_mps, heading_rad }`
  (`crates/kirra-ros2-adapter/src/state.rs:186-191` via
  `crates/kirra-ros2-adapter/src/parsing.rs:95-122`);
- **stamps freshness** on each batch (`touch_objects`,
  `crates/kirra-ros2-adapter/src/state.rs:437`; staleness const
  `SUBSCRIPTION_STALENESS_TIMEOUT_MS = 500`, `state.rs:106`);
- runs **`validate_vehicle_command` per-pose** in its slow loop
  (`crates/kirra-ros2-adapter/src/validation.rs:187`), beside `snapshot_objects()`
  (`crates/kirra-ros2-adapter/src/node.rs:333-343`);
- and **depends on `kirra-runtime-sdk`** (`crates/kirra-ros2-adapter/Cargo.toml:5-9`;
  imports at `validation.rs:19-26`, `state.rs:23`), so every PMON primitive
  (`SharedPerceptionCap`, `PerceptionCapPublisher`, `resolve_perception_cap`,
  `apply_perception_cap`, `kinematic_plausibility_derate`) is **linkable as-is**.

Posture, by contrast, flows **verifier → adapter** over HTTP/SSE
(`crates/kirra-ros2-adapter/src/posture_source.rs:1-33`) — the adapter is a posture
*consumer* but a perception *producer-adjacent* surface. **Perception is already
local to the adapter; it does not need to be transported to the verifier.** That
asymmetry is what makes the adapter the natural enforcement surface.

---

## 2. Architecture — D3a: compose in the adapter (DECIDED), D3b deferred

**DECIDED (D3a):** add an **adapter-local `SharedPerceptionCap` + `PerceptionCapPublisher`**,
ticked from the slow loop (`node.rs:333`) right where `snapshot_objects()` already
runs, and compose the resolved cap into the adapter's own `validate_vehicle_command`
path (`validation.rs:187`) via the **same `pub` `apply_perception_cap`** — not a
reimplementation. The verifier-service composition (PMON-002, HTTP/fabric) stays
**unchanged**.

**Acknowledged explicitly:** the adapter thereby becomes a **perception-ENFORCING
surface that PMON-002 did not enumerate** (PMON-002 listed HTTP + fabric, with
parko-kirra staged). This doc **adds the adapter as a fourth surface.** It is the
right one because perception + verdict are already in-process there.

**Rationale (D3a over D3b):** no new transport, no serialization, no freshness-hop,
no perception-rate latency added to a verifier round-trip. **D3b** (transport
perception to the verifier so its in-process publisher feeds
`ServiceState.perception_cap`) reuses the already-wired HTTP/fabric composition but
is heavier and adds a hop on a perception-rate path — **deferred** (§8).

The async update-then-read shape is identical to PMON-002 §2 (the posture cache /
RSS-state pattern): the publisher writes the cap out-of-band on each tick; the
verdict path reads it O(1) and `min`s it in. Here both sides live in the adapter
process.

---

## 3. The shim — `PerceivedObject` → `TrackedObject` (the gap)

The kinematic guard consumes `TrackedObject`
(`src/gateway/perception_monitor.rs:198-207`):

| `TrackedObject` field | Source in adapter | Mapping |
|---|---|---|
| `id: u64` (`:199`) | `PerceivedObject.id` (`state.rs:187`) — folded from `object_id.uuid` low 8 bytes (`parsing.rs:99-103`) | direct |
| `pos_m: Vec2` (`:200-201`, map frame) | `PerceivedObject.pos.{x_m,y_m}` (`state.rs:188`, from `initial_pose…pose.position`, `parsing.rs:104,117`) | direct |
| `vel_mps: Vec2` (`:202-203`, map-frame ground velocity) | upstream `twist.linear.{x,y}` (`parsing.rs:112-113`) | see §4 sub-decision (preserve vs reconstruct) |
| `prev_pos_m: Vec2` (`:204-205`) | — | **D2a:** set `prev_pos_m = pos_m` |
| `dt_s: f64` (`:207`) | — | **D2a:** a positive constant → implied speed 0 → teleport check no-op |

Per ADR-0004, this is a remap of upstream-tracked objects — nothing is associated
or estimated inside Kirra.

---

## 4. Decisions — locked

### D1 — tracked-objects-only (precondition)
Consume Autoware **tracking/prediction** output (`PredictedObjects` carries
`object_id.uuid` + `twist`, `parsing.rs:80-114`); Kirra checks, builds nothing.
**Precondition:** the integrator wires Autoware **tracking/prediction** (not a raw
detector) to `~/input/objects`. Raw detections would force association inside Kirra
= ADR-0004 boundary violation → **reject, never scope in.**

### D2a — reported-velocity ceiling only (first slice)
Populate `vel_mps` from the upstream twist; set `prev_pos_m = pos_m` and `dt_s` to a
positive constant so the **teleport (implied-speed) check is a no-op**
(`|pos − prev_pos|/dt = 0`). Only the velocity-ceiling check
(`vel_mps.magnitude() > V_OBJECT_MAX_MPS`) is active in slice 1.
**D2b (teleport) DEFERRED:** it needs prior-frame retention keyed by the
upstream-provided ID — memoizing *upstream* IDs is not building a tracker, but it is
the boundary's edge, and the folded `id` is **intra-cycle-stable only / reusable**
(`parsing.rs:99-102`), so it stays out of slice 1 (§8).

### D3a — compose in the adapter
Per §2. Adapter-local cap + publisher + `apply_perception_cap`; verifier-service
composition unchanged; adapter acknowledged as a new enforcing surface.

### D4 — map-frame pre-enable gate
The ceiling `V_OBJECT_MAX_MPS = 60.0` is **map-frame absolute ground speed**
(`src/gateway/perception_monitor.rs:42-50`), but the adapter applies **no frame
transform** to object twist (`parsing.rs:112-114`) — unlike the **ego** odom, whose
twist is explicitly noted body-frame (`parsing.rs:124-126`). So the map-frame
assumption **rests entirely on the upstream Autoware message contract.**

**Named pre-enable gate:** before flipping the enable flag on any vehicle, confirm
the target Autoware version emits object twist as **map/world-frame ABSOLUTE
velocity** — not ego-relative/closing, not body/sensor frame. If it does not, the
shim must transform to map frame **before** the ceiling check.

**Comment correction (slice-1 implement TASK — not done in this doc PR):** the
PMON-001 comment at `src/gateway/perception_monitor.rs:42-50` asserting map-frame
"confirmed against the Autoware adapter" is **overstated** — the adapter does no
transform. Corrected wording to apply in slice 1:

> *"map-frame absolute ground speed. This rests on the upstream `PredictedObjects`
> twist being published in map/world frame; confirm per deployment (see
> KIRRA-OCCY-PMON-003 §4 D4 pre-enable gate)."*

### D5 — adapter env-var enable gate, default OFF
A new adapter env var (mirroring `KIRRA_POSTURE_STREAM_URL` and
`KIRRA_SUBSCRIPTION_STALENESS_MS`, `node.rs:50-55,323`) gates the ingest, **default
off** → Track-C stays a no-op until a deployment opts in.
**Freshness flows for state 3:** `touch_objects` (`state.rs:437`) stamps each
objects batch; the publisher carries that as `generated_at_ms` with a `ttl_ms`; on a
silent objects stream the **staleness sweep publishes the MRC-floor (0.0) cap** —
the adapter already detects "objects subscription stream closed — staleness will
fire fleet-wide" (`node.rs:293`). The verifier-service `perception_monitor_enabled`
stays the **separate** gate for the HTTP/fabric surface (different process).

---

## 5. Sub-decisions to flag (options + lean; not hard-decided)

1. **`vel_mps` source — PRESERVE vs RECONSTRUCT.**
   - *Preserve:* stop discarding the upstream twist vector at `parsing.rs:114`; carry
     `vx,vy` into `PerceivedObject` and the shim. Faithful; future-proofs
     direction-aware checks.
   - *Reconstruct:* in the shim, `vel_mps = velocity_mps · (cos,sin)(heading_rad)`
     from the existing scalar + heading.
   - The slice-1 ceiling check is **magnitude-only**, so reconstruction yields an
     **exact magnitude** (direction approximate but unused) — acceptable for slice 1.
   - **Lean: PRESERVE** (faithfulness; avoids re-introducing a vector approximation
     the moment D2b/direction checks land). **Flagged for the implementer.**

2. **`dt_s` positive-constant value (teleport no-op).** Any `dt_s > 0` makes the
   implied speed 0. **Lean:** a documented sentinel such as `dt_s = 1.0` s with a
   code comment "D2a teleport no-op — see KIRRA-OCCY-PMON-003 §4." (Value is inert in
   slice 1; D2b will replace it with the real inter-frame Δt.)

3. **Adapter env-var name.** **Lean:** `KIRRA_PERCEPTION_DERATE_ENABLED`
   (parallels the existing `KIRRA_*` adapter vars). Final name at implement time.

4. **`ttl_ms` / staleness budget.** Reuse `SUBSCRIPTION_STALENESS_TIMEOUT_MS = 500`
   (`state.rs:106`) vs a separate perception budget. **Lean:** reuse 500 ms for
   slice 1 (one fewer tunable; matches the adapter's existing subscription-staleness
   semantics and the `KIRRA_SUBSCRIPTION_STALENESS_MS` override), and split to a
   dedicated perception budget only if the perception tick rate diverges from the
   other subscriptions. **Flagged.**

---

## 6. Invariants (this design preserves)

- **`kinematics_contract.rs` stays byte-identical** — compose via the `pub`
  `apply_perception_cap` (a `min` into `odd_speed_cap_mps`), never by touching the
  verdict fn; same property the PMON-002 merge held.
- **`DenyCode` + the deny path untouched**; the guards/composition are **derate-only**.
- **Fail-closed, no new code path:** an MRC-floor cap `0.0` → `apply_perception_cap`
  tightens `effective_max_speed` to `0.0` → the adapter's existing
  `validate_vehicle_command` P2 emits `ClampLinear(0.0)` → controlled stop. State 3
  (stale/silent) yields the same via the sweep.

---

## 7. Recommended first slice (restated)

Smallest ingest that flips the **kinematic guard live for the ROS2/Autoware
transport**, gated off by default:

1. **Preserve** the upstream twist vector into `PerceivedObject` (sub-decision 1
   lean) — or reconstruct in the shim if the implementer chooses.
2. **Shim** `PerceivedObject → TrackedObject`: `id`, `pos_m`, `vel_mps` (vector);
   `prev_pos_m = pos_m`, `dt_s = 1.0` (D2a — teleport no-op).
3. **Adapter-local `SharedPerceptionCap` + `PerceptionCapPublisher`**, ticked from
   the slow loop (`node.rs:333`); `generated_at_ms` from `touch_objects` freshness;
   `sweep_staleness` on the objects-stale path (`node.rs:293`).
4. **Compose** the resolved cap into the adapter's `validate_vehicle_command`
   (`validation.rs:187`) via `apply_perception_cap` — derate-only, MRC-floor-on-stale.
5. **Enable gate** = adapter env var `KIRRA_PERCEPTION_DERATE_ENABLED`, **default
   off**; the verifier-service `perception_monitor_enabled` stays the separate
   HTTP/fabric gate.
6. **Comment correction** (D4) applied as part of slice 1.
7. **Pre-enable gate (D4):** confirm map-frame absolute object twist for the target
   Autoware version before enabling on a vehicle; plus a sim/bench validation pass.

---

## 8. Staged (named, not designed here)

- **D2b — teleport check:** prior-frame retention keyed by upstream object ID
  (boundary's edge; intra-cycle ID stability + reuse to handle).
- **Range guard** (PMON-001 Guard 2): no `R_obs` producer exists (#120 Item B);
  stays PENDING-WIRING.
- **parko-kirra matched-pair** wiring (primary + diverse shadow in lockstep).
- **D3b — verifier-service HTTP/fabric perception path:** transport perception to
  the verifier to feed its in-process `ServiceState.perception_cap`.

---

## 9. Decisions — resolved

| ID | Decision | Rationale / evidence |
|----|----------|----------------------|
| **D3a** | Compose in the **adapter** (adapter-local cap + publisher + `apply_perception_cap`); verifier-service composition unchanged; adapter is a new enforcing surface. | Perception + verdict already in-process (`node.rs:192,333-343`, `validation.rs:187`); no new transport/serialization/latency. D3b deferred. |
| **D1** | Tracked-objects-only; integrator must wire Autoware tracking/prediction. | `PredictedObjects` carries IDs + twist (`parsing.rs:80-114`); raw detections → ADR-0004 violation. |
| **D2a** | Reported-velocity ceiling only; teleport a no-op (`prev_pos_m = pos_m`, `dt_s > 0`). | Slice-1 magnitude check needs only `vel_mps`; D2b needs prior-frame state (deferred). |
| **D4** | Map-frame is a **named pre-enable gate**; comment correction is a slice-1 task. | Ceiling is map-frame absolute (`perception_monitor.rs:42-50`); adapter does no transform (`parsing.rs:112-114`); ego twist noted body-frame (`parsing.rs:124-126`). |
| **D5** | Adapter env-var enable gate, default OFF; freshness via `touch_objects` → sweep → state-3 MRC. | Mirrors `KIRRA_*` adapter vars (`node.rs:50-55,323`); silent-stream detection (`node.rs:293`); separate from verifier-service gate. |

**Governor boundary:** the ingest is a pure field-remap of upstream-tracked objects
— no tracker, associator, or detector in Kirra. D2b is the deferred edge; D1
raw-detections is a violation to reject.
