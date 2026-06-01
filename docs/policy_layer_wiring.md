# Wiring `enforce_actuator_safety_envelope` into `kirra_verifier_service.rs`

This document records the exact diff to apply to `src/bin/kirra_verifier_service.rs`
to integrate the actuator safety envelope middleware introduced in v2.1.0.

---

## 1. Add imports

```rust
use axum::middleware::from_fn_with_state;
use crate::gateway::policy_layer::enforce_actuator_safety_envelope;
```

## 2. Update `src/gateway/mod.rs`

```rust
pub mod cmd_vel;
pub mod interceptor;
pub mod kinematics_contract;   // added v2.0.0
pub mod policy;
pub mod policy_layer;          // added v2.1.0
```

## 3. Router wiring

Add the actuator motion route under the identity-gated + physics enforcement stack.
Layer application order (axum layers apply bottom-up at call time):

```
Layer 3 (outermost, runs first): require_admin_token
Layer 2:                          require_client_identity
Layer 1 (innermost, runs last):   enforce_actuator_safety_envelope
```

Auth first — unauthenticated probes must not observe fleet posture via timing or
response-code differences from the physics layer.

```rust
let actuator_motion_routes = Router::new()
    .route("/actuator/motion/command", post(handle_actuator_motion_command))
    .layer(from_fn_with_state(
        Arc::clone(&state),
        enforce_actuator_safety_envelope,
    ));

// Merge into the identity-gated router.
// DO NOT re-add /industrial/evaluate — it is already registered as Tier 1.
let identity_gated_routes = Router::new()
    .merge(actuator_motion_routes)                         // ← v2.1.0
    .route("/system/posture/stream",      get(system_posture_stream))
    .route("/federation/reports/submit",  post(submit_federated_report))
    .route("/action_filter/evaluate",     post(evaluate_action_filter))
    .route("/industrial/evaluate",        post(evaluate_industrial_adapter))
    .layer(from_fn_with_state(Arc::clone(&state), require_client_identity))
    .layer(from_fn(require_admin_token));
```

## 4. Handler stub

```rust
/// Receives a proposed vehicle motion command after it has been validated or
/// clamped by `enforce_actuator_safety_envelope`. Any clamping is already
/// reflected in the payload by the time this handler runs.
async fn handle_actuator_motion_command(
    State(svc): State<Arc<ServiceState>>,
    Json(cmd): Json<ProposedVehicleCommand>,
) -> impl IntoResponse {
    tracing::info!(
        linear_velocity_mps = %cmd.linear_velocity_mps,
        steering_angle_deg  = %cmd.steering_angle_deg,
        "Actuator motion command admitted through safety envelope"
    );
    // TODO v2.2.0: forward to ros2_adapter / dds_bridge.
    // NaN/Inf rejection happens in ros2_adapter.rs before DDS publish.
    StatusCode::ACCEPTED
}
```

---

## Bugs fixed vs. original milestone doc proposal

| # | Doc error | Correct form |
|---|-----------|-------------|
| 1 | `State<Arc<AppState>>` in middleware | `State<Arc<ServiceState>>` (invariant #11) |
| 2 | `FleetPosture` from `crate::gateway::posture_cache` | `crate::verifier::FleetPosture` |
| 3 | `PostureCache::new()` | `Arc<RwLock<Option<CachedFleetPosture>>>` |
| 4 | `state.store.lock()` (assumed Mutex) | `svc.app.store.*` direct call |
| 5 | `now_ms()` undefined | Defined locally in `policy_layer.rs` |
| 6 | `/industrial/evaluate` re-mounted | Already Tier 1; not added again |
