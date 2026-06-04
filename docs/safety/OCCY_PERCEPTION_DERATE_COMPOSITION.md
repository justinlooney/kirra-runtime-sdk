# Occy / KIRRA — Perception-Derate Verdict-Path Composition (Enforcement Keystone)

**Doc ID (proposed):** KIRRA-OCCY-PMON-002.
**Status:** Decided design for review. Records the architecture that makes the
Track-C perception-monitor guards (KIRRA-OCCY-PMON-001) actually *enforce* — the
verdict-path-**changing** keystone. No code in this commit; this is the spec the
implementation prompt builds against. Slice-1 (PMON-001) was byte-stable and
non-enforcing by construction; this design changes the hot path, so it is
designed and reviewed first.
**Scope:** Track C only (the independent governor; ADR-0004). Composes the
guards' output into the existing kinematic verdict path. Does NOT add perception
inference, does NOT touch `DenyCode` or the deny path.
**Base:** verified against `main` @ `a67161d`.
**References:** KIRRA-OCCY-PMON-001 (the two guards + `DerateDecision`/`DerateCode`),
ADR-0002 (cap composition: most-conservative-wins `min`), ADR-0004 (independent
safety channel / doer–checker), KIRRA-OCCY-SG2-MARGIN-001 (containment, the
separate-WCET-budget precedent).

---

## 0. The boundary (why this is still Track C)

```
[sensors] → Parko (Track B) → Occy world model + plan (Track A)
         → KIRRA derate guards (Track C) → cap → verdict path → actuators
```

PMON-001 established the two guards as bounded, deterministic, **stateless**
analytic functions over Track-A *output* that emit a homogeneous
`DerateDecision { cap_mps, DerateCode }`. This doc specifies **how that cap
reaches the actuator** without violating the verdict-path invariants. The guards
remain derate-only: they reduce a permitted-speed ceiling and compose into the
existing `EnforceAction::ClampLinear`; they never author a `DenyBreach` and never
sit on the byte-stable deny path.

**Constraints carried from PMON-001 §0 (restated as gates for this design):**

| # | Constraint | How this design holds it |
|---|------------|--------------------------|
| C1 | No perception DNN on the verdict path | The verdict path gains only an O(1) cap read; all analytic guard work runs off-path at perception-tick rate. |
| C2 | Verdict path stays byte-stable / bounded-WCET / fail-closed | The per-command path adds one read + one `min` (≈ `resolve_posture`). The `wcet_gate.rs` boundedness argument — "verdict path evaluates ONE command, O(1)" — is preserved. `validate_vehicle_command`'s structure is unchanged except the single ceiling bind composes one more `min` input. |
| C3 | Agnostic to Track-A model choice | The cap is a scalar published by the monitor; the verdict path never sees objects, ranges, or models. |

---

## 1. Current composition map (verified against `a67161d`)

**The cap function** — `VehicleKinematicsContract::effective_max_speed_mps()`
(`src/gateway/kinematics_contract.rs:159`):

```rust
match self.odd_speed_cap_mps {
    Some(cap) if cap < self.max_speed_mps => cap,
    _ => self.max_speed_mps,
}
```

i.e. `min(max_speed_mps, odd_speed_cap_mps)`. **This 2-input `min` is the entire
ADR-0002 "composition" implemented today** — there is no `weather_derate` or
`range_supported` term yet; PMON-001 §5.2 noted this gap, and this design fills
the `range_supported` slot (plus the kinematic cap) via composition rather than
by rewriting the function.

**The single consumption point** — `validate_vehicle_command`
(`src/gateway/kinematics_contract.rs:467`) binds the ceiling exactly once:

```rust
let effective_max_speed = contract.effective_max_speed_mps();   // :467
```

and then uses it at:
- **P2 hard ceiling** (`:468`): `if |linear_velocity_mps| > effective_max_speed → ClampLinear(effective_max_speed·sign)`;
- **P3/P4** accel/brake corrections re-clamp into `[-effective_max_speed, +effective_max_speed]` (`:506`, `:515`).

**→ The single insertion point for a perception cap is the bind at `:467`:**
make `effective_max_speed = min(contract.effective_max_speed_mps(), perception_cap)`.
Everything downstream (P2/P3/P4 and the existing `ClampLinear`) then enforces it
for free — **no new branch, no new `EnforceAction`, no `DenyCode`.**

**Live verdict surfaces that reach `:467` (verified — three, all Nominal-arm):**

| # | Surface | File:line | Nominal call |
|---|---------|-----------|--------------|
| 1 | HTTP actuator middleware | `src/gateway/policy_layer.rs:222` | `validate_vehicle_command(cmd, nominal_reference_profile())` |
| 2 | Fabric asset governor | `src/fabric/governor.rs:120` | `validate_vehicle_command(cmd, profile.nominal_contract())` |
| 3 | parko-kirra diverse governor | `parko/crates/parko-kirra/src/diverse.rs:225` | inline `effective_ceiling()` — does **not** call the shared fn |

Surfaces 1 and 2 both route through the shared `validate_vehicle_command`, so a
single composition at `:467` covers both. Surface 3 re-derives its ceiling inline
(`diverse.rs:225–230`) and is handled in the staged surface-coverage plan (§6).

---

## 2. The async update-then-read pattern this design reuses (verified)

Two existing, proven patterns establish exactly the shape Option B uses — an
out-of-band writer plus an O(1) hot-path read that fails closed on ambiguity:

1. **Posture cache** — `resolve_posture` (`src/gateway/policy_layer.rs:50`):
   ```rust
   match svc.posture_cache.read() {
       Ok(guard) => match guard.as_ref() {
           Some(cached) => cached.posture.clone(),
           None => FleetPosture::LockedOut,     // fail-closed
       },
       Err(_) => FleetPosture::LockedOut,        // poisoned → fail-closed
   }
   ```
   The posture-engine worker writes the cache out-of-band; the verdict path reads
   it O(1) and maps `None`/poison → `LockedOut`. `SharedPostureCache` is
   `Arc<RwLock<Option<CachedFleetPosture>>>` with `generated_at_ms`/`ttl_ms`/`generation`.

2. **RSS state** — `diverse.rs`: `update_rss_state` writes `self.rss_state = state`
   (`:204`), out of band; the verdict path reads `!self.rss_state.safe` in
   `classify()` (`:216`) to route to the minimum-risk envelope. Write-then-read,
   hot-path O(1).

**The perception cap mirrors pattern 1 precisely** — a sibling cache and a sibling
fail-closed read.

---

## 3. Architecture — Option B (DECIDED), with Option A rejected

### 3.1 Option B (decided): evaluate at perception-tick rate, publish a cap

A **perception-monitor worker** evaluates the PMON-001 guards when new perception
arrives, composes `min(kinematic_cap, range_cap)`, and **publishes** the result to
a new `SharedPerceptionCap` — mirroring the posture cache:

```
SharedPerceptionCap = Arc<RwLock<Option<CachedPerceptionCap>>>
CachedPerceptionCap { cap_mps: f64, generated_at_ms: u64, ttl_ms: u64, reason: DerateCode }
```

The verdict path adds exactly **one O(1) read + one `min`** at the `:467`
insertion point — the same cost and shape as `resolve_posture`, which is already
on the path and within budget.

**Rationale (decided):**
- **Decouples the O(`MAX_TRACKED_OBJECTS`) guard cost from command rate.** The
  kinematic guard is `O(256)`; running it per command would couple verdict WCET to
  object count. At tick rate it runs once per perception frame regardless of how
  many commands arrive.
- **Keeps the verdict path bounded regardless of object count** — the C2 / `wcet_gate`
  boundedness argument ("verdict path evaluates ONE command, O(1)") stays intact.
- **Reuses two proven patterns** (posture cache + RSS state) — minimal novel
  machinery, well-understood fail-closed semantics.

### 3.2 Option A (rejected): guards on the per-command verdict path

`validate_vehicle_command` would call both guards on every command.

**Why rejected:** the kinematic guard is `O(MAX_TRACKED_OBJECTS) = O(256)` (finite
+ velocity-ceiling + teleport checks per object). Adding ~256 object iterations to
**every command** is orders of magnitude above the current O(1) handful of scalar
ops and **blows the per-command WCET budget** (`GOVERNOR_VERDICT_WCET_TARGET_MICROS
= 100 µs`; see §4). It also forces perception data to be plumbed *into* the
per-command call signature. The range guard alone is O(1) (one `sqrt`) and would be
fine, but the kinematic guard makes A untenable without a per-command budget
revision and a tighter per-object bound. Option B confines the only new
verdict-path cost to an O(1) read needing no budget revision.

---

## 4. The 3-state cap lifecycle + staleness watchdog (DECIDED refinement)

The cap has **three** states, not two. This enabled-gate is a first-class part of
the cap lifecycle — it is what lets the mechanism land safely as a **no-op** before
any perception ingest exists, and it separates "layer not deployed" from "deployed
layer faulted."

| State | Condition | Verdict-path effect |
|-------|-----------|---------------------|
| **1 — NOT CONFIGURED / not enabled** | The perception monitor is not deployed/enabled | **No cap, no derate.** The monitor is an *optional* safety layer; its absence is **NOT a fault** and must not derate. The composition is a pure no-op (`effective_max_speed` unchanged). |
| **2 — ENABLED + FRESH** | Enabled; a cap published within `ttl_ms` | Derate to the published `cap_mps` (the `min` applies). |
| **3 — ENABLED + STALE / None / poisoned** | Enabled but TTL expired, cache `None`, or RwLock poisoned | **Fail closed to the MRC floor** (`cap_mps = 0.0` → controlled stop). A configured monitor going silent **IS a fault.** |

**Why the gate is load-bearing:** without state 1, a naive "no cap → MRC" rule
would brake *every* vehicle that simply hasn't deployed perception monitoring. The
gate distinguishes **"layer not deployed"** (no-op, state 1) from **"deployed layer
faulted"** (MRC, state 3).

**Staleness watchdog:** state 3 is produced by a sweep that mirrors
`resolve_posture → LockedOut` and `telemetry_watchdog` — when an enabled monitor
publishes no fresh cap within `ttl_ms`, the watchdog writes an MRC-floor cap (or the
verdict-path read maps expired/`None`/poison → MRC inline). A stale perception
snapshot can never read as "all clear."

**Enabled flag (implementation note):** the enabled/not-enabled distinction is a
deployment configuration (e.g. an `Option<SharedPerceptionCap>` on `ServiceState`
that is `None` when the layer is not wired, vs. a configured-but-empty cache that
the watchdog drives to MRC). Exact representation is confirmed at implement time;
the semantic contract is the three states above.

---

## 5. WCET treatment (verified budgets; constant names TBD at implement time)

Budgets in `src/wcet_gate.rs`:
- `GOVERNOR_VERDICT_WCET_TARGET_MICROS = 100` (`:94`) — target-SoC per-command budget.
- `GOVERNOR_VERDICT_WCET_CI_THRESHOLD_MICROS = 1000` (`:105`) — CI gate, 10× headroom.
- `GOVERNOR_CONTAINMENT_WCET_CI_THRESHOLD_MICROS = 10_000` (`:123`) — **precedent:
  a structurally-heavier-than-O(1) guard gets its OWN separate budget, not folded
  into the per-command budget.**

**Under Option B:**
- **Verdict path:** adds one `RwLock` read + one `min` ≈ the cost of
  `resolve_posture` (already budgeted, already passing). **No per-command budget
  revision needed.** A small O(1) cap-read assertion should be added to the
  per-command WCET coverage.
- **Tick-rate guard evaluation:** gets its **own registered budget**, following the
  containment separate-budget precedent — a structurally-heavier guard
  (`O(MAX_TRACKED_OBJECTS)`) does not fold into the per-command budget. A new
  constant (e.g. `GOVERNOR_PERCEPTION_GUARD_WCET_CI_THRESHOLD_MICROS`) is added and
  measured at tick rate, decoupled from command rate.

> **Flag:** the exact `wcet_gate` constant **names and values** are to be confirmed
> at implement time. This doc fixes the *treatment* (separate budget for the
> tick-rate eval; O(1) read on the per-command path), not the numbers.

---

## 6. Fail-closed composition + surface coverage

### 6.1 Fail-closed composition (D4 — DECIDED)

- **Composition order:** the perception cap is `min`'d into the **Nominal-arm**
  `effective_max_speed` (ADR-0002 most-conservative-wins = a pure `min`). Applied
  **uniformly**; it is a harmless **no-op on Degraded/LockedOut** (those arms are
  already at/below the MRC envelope, so the `min` changes nothing).
- **MRC-floor cap → controlled stop, no new code path:** a published `cap_mps = 0.0`
  flows into `effective_max_speed = min(…, 0.0) = 0.0`; `validate_vehicle_command`
  P2 then clamps any non-zero command to `0.0` via the **existing**
  `ClampLinear(0.0)` → actuator commanded to stop.
- **Staleness → MRC** via state 3 (§4).
- **Structurally-invalid perception already fails closed inside the slice-1 guards**
  (PMON-001: non-finite / `dt_s ≤ 0` / over-`MAX_TRACKED_OBJECTS` →
  `PerceptionSnapshotUnhealthy` MRC floor; untrusted range → controlled stop). The
  monitor publishes that MRC-floor cap; the composition enforces it.
- **Determinism + bounded-WCET preserved; `DenyCode` untouched.** The guards are
  derate-only and never touch the deny path. The verdict path remains the O(1)
  lock-free pipeline the `wcet_gate.rs` argument asserts, plus one bounded read.

### 6.2 Surface coverage (D1 — DECIDED)

- **Enforce at the HTTP (`policy_layer`) + fabric surfaces FIRST**, via the single
  `:467` insertion point — both call the shared `validate_vehicle_command`, so one
  composition covers both.
- **parko-kirra is STAGED.** Requirement for that stage: the cap must reach **both
  the parko primary AND the diverse shadow** (`diverse.rs` inline
  `effective_ceiling`) **consistently**. parko-kirra is a **matched comparator
  pair**, not just another surface — if the cap reaches one governor and not the
  other, the comparator sees them diverge and **false-alarms**. The parko-kirra
  stage must publish the cap into both governors in lockstep (the same way
  `comparator.update_rss_state` fans out to primary + shadow at
  `parko/crates/parko-kirra/src/comparator.rs:495–497`).

---

## 7. Prerequisites — this design ENFORCES NOTHING until ingest lands (verified)

Two upstream producers are required, and **neither exists anywhere in the repo
today** (verified by repo-wide search on `a67161d`):

| Prerequisite | For | Status |
|--------------|-----|--------|
| A `TrackedObject` perception-**ingest** source (the #126 perception-input contract endpoint/tick) | kinematic guard | **Does not exist.** No producer of `PerceptionOutput`/`TrackedObject` lists. |
| An `R_obs` **observed-detection-range producer** (#120 Item B / `OCCY_SPEED_CAP_VALIDATION`) | range guard | **Does not exist.** No producer of an observed range. |

**Make explicit:** this composition is **step 1 of the enforcement path and
enforces nothing on its own.** Its value is that it **isolates and banks the
hot-path change** — the verdict-path-touching part — ahead of the ingest plumbing,
and it is **testable now with synthetic published caps** (a unit/integration test
can write a `CachedPerceptionCap` directly and assert the `min`/`ClampLinear`
behavior, the 3-state lifecycle, and the staleness→MRC transition). The risky
keystone lands, reviewed and WCET-gated, decoupled from the (larger, separate)
ingest work.

---

## 8. Which guard goes live (D3 — DECIDED)

- **Kinematic-plausibility guard FIRST** — it is `R_obs`-independent (consumes only
  `TrackedObject` lists). It enforces as soon as the `TrackedObject` ingest exists.
- **Range-based guard STAGED / PENDING-WIRING** — gated on the `R_obs` producer that
  does not exist. The `range_supported` slot in the published `min` is simply absent
  (or a no-op `+∞` cap) until the producer lands.
- Both are *additionally* gated on a perception ingest source (§7).

---

## 9. Implementation plan (staged, Option B, gated-off, kinematic-first)

A future, separate change (this doc is design-only):

1. **`SharedPerceptionCap` on `ServiceState`** — `Arc<RwLock<Option<CachedPerceptionCap>>>`
   beside `posture_cache`, with the 3-state enabled-gate representation (§4).
2. **Perception-monitor worker** — mirror `start_posture_engine_worker` +
   `telemetry_watchdog`: on a perception tick, run `kinematic_plausibility_derate`
   (and `range_supported_derate` once `R_obs` exists), publish `min` to the cache;
   a staleness sweep drives an enabled-but-silent monitor to the MRC-floor cap.
3. **Verdict-path O(1) read + compose at `:467`**, behind the enabled gate: read the
   cap (fail-closed on stale/`None`/poison per §4), `min` into `effective_max_speed`.
   Cleanest forms: build the Nominal contract with
   `odd_speed_cap_mps = min(existing, perception_cap)`, **or** add an explicit `min`
   at `:467` behind an optional passed-in cap. (Choice confirmed at implement time;
   either keeps the downstream P2/P3/P4 + `ClampLinear` enforcement unchanged.)
4. **Separate guard-eval WCET budget** — add the new constant (§5) and a tick-rate
   measurement; add an O(1) cap-read assertion to the per-command coverage.
5. **First cut = kinematic only**; range stays PENDING-WIRING (§8).
6. **parko-kirra staged** as a matched-pair fan-out (§6.2), after surfaces 1–2.

---

## 10. Decisions — resolved

| ID | Decision | Rationale |
|----|----------|-----------|
| **D1** | Enforce at HTTP (`policy_layer`) + fabric **first**, via the single `:467` insertion point; parko-kirra **staged**. | Surfaces 1–2 share `validate_vehicle_command`; one composition covers both. parko-kirra is a matched comparator pair — the cap must reach primary **and** diverse shadow in lockstep or the comparator false-alarms. |
| **D2** | **Option B** — evaluate guards at perception-tick rate, publish a cap to `SharedPerceptionCap`; verdict path does one O(1) read + `min`. Option A rejected. | Decouples `O(MAX_TRACKED_OBJECTS)` cost from command rate; keeps the verdict path O(1) and bounded; reuses posture-cache + RSS-state patterns. A would blow the 100 µs per-command budget. |
| **D3** | **Kinematic guard first** (R_obs-independent); range guard staged. | No `R_obs` producer exists; the kinematic guard needs only the object ingest. |
| **D4** | Perception cap is `min`'d into the **Nominal-arm** `effective_max_speed` (pure ADR-0002 `min`); applied uniformly, no-op on Degraded/LockedOut. | Most-conservative-wins; reuses the existing `ClampLinear` enforcement with no new code path. |
| **Refinement** | **3-state cap lifecycle** (not-configured = no-op; enabled+fresh = derate; enabled+stale = MRC). | Lets the keystone land as a safe no-op before ingest exists; separates "layer not deployed" from "deployed layer faulted." |

**Prerequisites (neither exists yet):** a `TrackedObject` perception-ingest source
(#126) and an `R_obs` producer (#120 Item B). This composition enforces nothing
until the ingest lands — it banks the hot-path change ahead of the plumbing.
