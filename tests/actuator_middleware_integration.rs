//! Integration tests for `enforce_actuator_safety_envelope`.
//!
//! These tests require a fully constructed `ServiceState` with an in-memory
//! `VerifierStore` and a real axum `TestServer`. They verify end-to-end HTTP
//! behaviour: status codes, response body mutation on clamp, and fail-closed
//! semantics for every posture state.

use std::sync::Arc;
use axum::{
    http::StatusCode,
    routing::post,
    Json, Router,
};
use axum_test::TestServer;
use serde_json::json;
use tokio::sync::{broadcast, RwLock};

use aegis_runtime_sdk::{
    bin::aegis_verifier_service::ServiceState,
    gateway::{
        kinematics_contract::ProposedVehicleCommand,
        policy_layer::enforce_actuator_safety_envelope,
    },
    posture_cache::{CachedFleetPosture, SharedPostureCache},
    verifier::{AppState, FleetPosture},
    verifier_store::VerifierStore,
};

// ---------------------------------------------------------------------------
// State builders
// ---------------------------------------------------------------------------

async fn build_state(posture: FleetPosture) -> Arc<ServiceState> {
    let store = VerifierStore::new(":memory:").expect("in-memory store");
    let (posture_tx, _) = broadcast::channel(1024);
    let app = Arc::new(AppState::new(store, posture_tx));
    let posture_cache: SharedPostureCache =
        Arc::new(RwLock::new(Some(CachedFleetPosture::new(posture))));
    Arc::new(ServiceState { app, posture_cache })
}

async fn build_state_empty_cache() -> Arc<ServiceState> {
    let store = VerifierStore::new(":memory:").expect("in-memory store");
    let (posture_tx, _) = broadcast::channel(1024);
    let app = Arc::new(AppState::new(store, posture_tx));
    // Explicitly None — cold start scenario
    let posture_cache: SharedPostureCache = Arc::new(RwLock::new(None));
    Arc::new(ServiceState { app, posture_cache })
}

fn build_router(state: Arc<ServiceState>) -> Router {
    Router::new()
        .route(
            "/actuator/motion/command",
            post(|Json(cmd): Json<ProposedVehicleCommand>| async move {
                // Echo the (possibly clamped) command so tests can inspect it.
                (StatusCode::OK, Json(cmd))
            }),
        )
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            enforce_actuator_safety_envelope,
        ))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Nominal posture
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_nominal_safe_command_passes_through_unmodified() {
    let state = build_state(FleetPosture::Nominal).await;
    let server = TestServer::new(build_router(state)).unwrap();

    let response = server
        .post("/actuator/motion/command")
        .json(&json!({
            "linear_velocity_mps": 10.0,
            "current_velocity_mps": 9.8,
            "delta_time_s": 0.1,
            "steering_angle_deg": 2.0,
            "current_steering_angle_deg": 1.8
        }))
        .await;

    response.assert_status(StatusCode::OK);
    let returned: ProposedVehicleCommand = response.json();
    assert_eq!(returned.linear_velocity_mps, 10.0);
    assert_eq!(returned.steering_angle_deg, 2.0);
}

#[tokio::test]
async fn test_nominal_over_acceleration_is_clamped_not_rejected() {
    let state = build_state(FleetPosture::Nominal).await;
    let server = TestServer::new(build_router(state)).unwrap();

    // 10 → 40 m/s in 0.1 s = 300 m/s² >> 2.5 limit
    let response = server
        .post("/actuator/motion/command")
        .json(&json!({
            "linear_velocity_mps": 40.0,
            "current_velocity_mps": 10.0,
            "delta_time_s": 0.1,
            "steering_angle_deg": 0.0,
            "current_steering_angle_deg": 0.0
        }))
        .await;

    response.assert_status(StatusCode::OK);
    let returned: ProposedVehicleCommand = response.json();
    // Expected: 10.0 + (2.5 × 0.1) = 10.25
    assert!(
        (returned.linear_velocity_mps - 10.25).abs() < 1e-9,
        "expected 10.25, got {}",
        returned.linear_velocity_mps
    );
}

#[tokio::test]
async fn test_nominal_invalid_time_delta_returns_400() {
    let state = build_state(FleetPosture::Nominal).await;
    let server = TestServer::new(build_router(state)).unwrap();

    let response = server
        .post("/actuator/motion/command")
        .json(&json!({
            "linear_velocity_mps": 10.0,
            "current_velocity_mps": 10.0,
            "delta_time_s": 0.0,
            "steering_angle_deg": 0.0,
            "current_steering_angle_deg": 0.0
        }))
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_nominal_highway_speed_high_steering_clamps_steering() {
    let state = build_state(FleetPosture::Nominal).await;
    let server = TestServer::new(build_router(state)).unwrap();

    // 30 m/s + 20° — bicycle model must clamp steering
    let response = server
        .post("/actuator/motion/command")
        .json(&json!({
            "linear_velocity_mps": 30.0,
            "current_velocity_mps": 30.0,
            "delta_time_s": 1.0,
            "steering_angle_deg": 20.0,
            "current_steering_angle_deg": 0.0
        }))
        .await;

    response.assert_status(StatusCode::OK);
    let returned: ProposedVehicleCommand = response.json();
    assert!(returned.steering_angle_deg < 20.0, "must have been clamped");
    assert!(returned.steering_angle_deg > 0.0, "sign must be preserved");
}

// ---------------------------------------------------------------------------
// Degraded posture
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_degraded_posture_clamps_high_speed_to_mrc_limit() {
    let state = build_state(FleetPosture::Degraded).await;
    let server = TestServer::new(build_router(state)).unwrap();

    let response = server
        .post("/actuator/motion/command")
        .json(&json!({
            "linear_velocity_mps": 15.0,
            "current_velocity_mps": 14.5,
            "delta_time_s": 0.5,
            "steering_angle_deg": 0.0,
            "current_steering_angle_deg": 0.0
        }))
        .await;

    response.assert_status(StatusCode::OK);
    let returned: ProposedVehicleCommand = response.json();
    assert_eq!(returned.linear_velocity_mps, 5.0, "MRC max speed is 5.0 m/s");
}

// ---------------------------------------------------------------------------
// LockedOut posture
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_locked_out_posture_returns_403_for_any_command() {
    let state = build_state(FleetPosture::LockedOut).await;
    let server = TestServer::new(build_router(state)).unwrap();

    let response = server
        .post("/actuator/motion/command")
        .json(&json!({
            "linear_velocity_mps": 1.0,
            "current_velocity_mps": 1.0,
            "delta_time_s": 0.1,
            "steering_angle_deg": 0.0,
            "current_steering_angle_deg": 0.0
        }))
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_locked_out_rejects_zero_motion_command() {
    // LockedOut must reject ALL commands — even a zero-velocity command.
    // The planner cannot submit a "do nothing" to bypass the lock.
    let state = build_state(FleetPosture::LockedOut).await;
    let server = TestServer::new(build_router(state)).unwrap();

    let response = server
        .post("/actuator/motion/command")
        .json(&json!({
            "linear_velocity_mps": 0.0,
            "current_velocity_mps": 0.0,
            "delta_time_s": 0.1,
            "steering_angle_deg": 0.0,
            "current_steering_angle_deg": 0.0
        }))
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Cache edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_empty_posture_cache_fails_closed_as_locked_out() {
    // None cache (cold start) must be treated as LockedOut — fail-closed.
    let state = build_state_empty_cache().await;
    let server = TestServer::new(build_router(state)).unwrap();

    let response = server
        .post("/actuator/motion/command")
        .json(&json!({
            "linear_velocity_mps": 5.0,
            "current_velocity_mps": 4.9,
            "delta_time_s": 0.1,
            "steering_angle_deg": 0.0,
            "current_steering_angle_deg": 0.0
        }))
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
}
