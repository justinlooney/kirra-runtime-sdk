// src/bin/aegis_verifier_service.rs
// Aegis Verifier Service — distributed legitimacy fabric entry point.

use axum::{
    extract::{Path, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis_runtime_sdk::verifier::{
    AppState, BackupExport, FlapStatus, FleetNodePosture, HealthResponse,
    NodeTrustState, RegisteredNode, VerifierOperationMode,
};
use aegis_runtime_sdk::verifier_store::VerifierStore;
use aegis_runtime_sdk::posture_cache::{now_ms, CachedFleetPosture, SharedPostureCache};
use aegis_runtime_sdk::security::constant_time_compare;
use aegis_runtime_sdk::action_filter::{evaluate_action_claim, ActionClaim, ActionDecision};
use aegis_runtime_sdk::protocol_adapter::{evaluate_industrial_event, IndustrialEvent};
use aegis_runtime_sdk::federation::{evaluate_federated_report, FederatedTrustReport, ReportEvaluation};

// --- Auth middleware ---------------------------------------------------------

/// Reads the expected admin token from AEGIS_ADMIN_TOKEN.
/// Fail-closed: if the env var is absent or empty, ALL mutation requests are denied
/// (503 Service Unavailable — the service is misconfigured, not the caller).
/// Timing-safe comparison via constant_time_compare prevents oracle attacks on the token.
async fn require_admin_token(request: Request, next: Next) -> Result<Response, StatusCode> {
    let expected = std::env::var("AEGIS_ADMIN_TOKEN")
        .unwrap_or_default();

    if expected.is_empty() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let provided = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !constant_time_compare(provided.as_bytes(), expected.as_bytes()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}

// --- Shared service state ---------------------------------------------------

struct ServiceState {
    app: Arc<AppState>,
    posture_cache: SharedPostureCache,
}

// --- Request / response types -----------------------------------------------

#[derive(Deserialize)]
struct RegisterNodeRequest {
    node_id: String,
    #[serde(default)]
    ak_public_pem: Option<String>,
    #[serde(default)]
    expected_pcr16_digest_hex: Option<String>,
}

#[derive(Deserialize)]
struct RegisterDependenciesRequest {
    node_id: String,
    depends_on: Vec<String>,
}

#[derive(Deserialize)]
struct VerifyAttestationRequest {
    node_id: String,
    nonce: u64,
    /// HMAC-SHA256(AEGIS_ADMIN_TOKEN, nonce_as_le_bytes) encoded as hex.
    /// In a full PKI deployment replace with a node-specific certificate signature.
    proof_hex: String,
}

#[derive(Serialize)]
struct AttestationStatusResponse {
    node_id: String,
    status: String,
    registered_at_ms: u64,
}

// --- Handlers ----------------------------------------------------------------

/// Unconditional liveness probe — returns 200 immediately with no I/O.
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok".to_string() })
}

/// Readiness probe — verifies the SQLite connection is alive before returning 200.
async fn ready(State(svc): State<Arc<ServiceState>>) -> impl IntoResponse {
    match svc.app.store.lock() {
        Ok(store) => match store.health_check() {
            Ok(()) => (StatusCode::OK, Json(HealthResponse { status: "ready".to_string() }))
                .into_response(),
            Err(_) => (StatusCode::SERVICE_UNAVAILABLE,
                       Json(HealthResponse { status: "db_unavailable".to_string() }))
                .into_response(),
        },
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE,
                   Json(HealthResponse { status: "store_lock_poisoned".to_string() }))
            .into_response(),
    }
}

/// Full state snapshot — nodes, dependency graph, and posture event log.
/// Protected by require_admin_token; must never be exposed unauthenticated.
async fn export_backup(State(svc): State<Arc<ServiceState>>) -> impl IntoResponse {
    match svc.app.store.lock() {
        Ok(store) => {
            let nodes = match store.load_nodes() {
                Ok(n) => n,
                Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR,
                                  Json(json!({ "error": "failed to load nodes" }))).into_response(),
            };
            let dependencies = match store.load_dependencies() {
                Ok(d) => d,
                Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR,
                                  Json(json!({ "error": "failed to load dependencies" }))).into_response(),
            };
            let posture_events = match store.load_all_posture_events() {
                Ok(e) => e,
                Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR,
                                  Json(json!({ "error": "failed to load posture events" }))).into_response(),
            };
            Json(BackupExport {
                exported_at_ms: now_ms(),
                nodes,
                dependencies,
                posture_events,
            }).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
                   Json(json!({ "error": "store lock poisoned" }))).into_response(),
    }
}

async fn register_node(
    State(svc): State<Arc<ServiceState>>,
    Json(req): Json<RegisterNodeRequest>,
) -> impl IntoResponse {
    if !svc.app.mode.allows_mutation() {
        return (StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "instance is in passive standby mode" }))).into_response();
    }
    let now = now_ms();
    let node = RegisteredNode {
        node_id: req.node_id.clone(),
        status: NodeTrustState::Unknown,
        registered_at_ms: now,
        last_trust_update_ms: 0,
        ak_public_pem: req.ak_public_pem,
        expected_pcr16_digest_hex: req.expected_pcr16_digest_hex,
    };

    // Fail-closed: disk must accept the write before memory is updated.
    if svc.app.persist_and_insert_node(node).is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to persist node" }))).into_response();
    }

    (StatusCode::CREATED, Json(json!({ "node_id": req.node_id, "status": "registered" }))).into_response()
}

async fn issue_challenge(
    State(svc): State<Arc<ServiceState>>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    if !svc.app.mode.allows_mutation() {
        return (StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "instance is in passive standby mode" }))).into_response();
    }
    if !svc.app.nodes.contains_key(&node_id) {
        return (StatusCode::NOT_FOUND,
                Json(json!({ "error": "node not registered" }))).into_response();
    }
    let nonce: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    svc.app.issue_challenge(&node_id, nonce, now_ms());
    (StatusCode::OK, Json(json!({ "node_id": node_id, "nonce": nonce }))).into_response()
}

async fn verify_attestation(
    State(svc): State<Arc<ServiceState>>,
    Json(req): Json<VerifyAttestationRequest>,
) -> impl IntoResponse {
    if !svc.app.mode.allows_mutation() {
        return (StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "instance is in passive standby mode" }))).into_response();
    }
    let now = now_ms();

    // Verify the proof: HMAC-SHA256(admin_token, nonce_le_bytes) == proof_hex.
    let admin_token = match std::env::var("AEGIS_ADMIN_TOKEN").ok().filter(|s| !s.is_empty()) {
        Some(t) => t,
        None => return (StatusCode::SERVICE_UNAVAILABLE,
                        Json(json!({ "error": "attestation key not configured" }))).into_response(),
    };
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(admin_token.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(&req.nonce.to_le_bytes());
    let expected_hex = hex::encode(mac.finalize().into_bytes());

    if !constant_time_compare(req.proof_hex.as_bytes(), expected_hex.as_bytes()) {
        return (StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "attestation proof invalid" }))).into_response();
    }

    // Consume the nonce (replay protection — second use is rejected).
    if !svc.app.consume_challenge(&req.node_id, req.nonce, now) {
        return (StatusCode::CONFLICT,
                Json(json!({ "error": "nonce absent, expired, or already consumed" }))).into_response();
    }

    // Promote node to Trusted, persist before updating memory.
    let updated = match svc.app.nodes.get(&req.node_id) {
        Some(existing) => RegisteredNode {
            node_id: existing.node_id.clone(),
            status: NodeTrustState::Trusted,
            registered_at_ms: existing.registered_at_ms,
            last_trust_update_ms: now,
            ak_public_pem: existing.ak_public_pem.clone(),
            expected_pcr16_digest_hex: existing.expected_pcr16_digest_hex.clone(),
        },
        None => return (StatusCode::NOT_FOUND,
                        Json(json!({ "error": "node not registered" }))).into_response(),
    };

    if svc.app.persist_and_insert_node(updated).is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to persist trust state" }))).into_response();
    }

    // Emit posture event after successful attestation (best-effort; does not
    // roll back the trust promotion if the log write fails).
    let posture = svc.app.calculate_posture(&req.node_id);
    if let Ok(posture_json) = serde_json::to_string(&posture) {
        if let Ok(store) = svc.app.store.lock() {
            let _ = store.save_posture_event(
                &req.node_id, "ATTESTATION_TRUSTED", &posture_json, None, now,
            );
        }
    }

    (StatusCode::OK, Json(json!({ "node_id": req.node_id, "attested": true }))).into_response()
}

async fn get_node_status(
    State(svc): State<Arc<ServiceState>>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    match svc.app.nodes.get(&node_id) {
        Some(node) => {
            let status = match &node.status {
                NodeTrustState::Trusted => "Trusted",
                NodeTrustState::Untrusted(_) => "Untrusted",
                NodeTrustState::Unknown => "Unknown",
            };
            (StatusCode::OK, Json(AttestationStatusResponse {
                node_id: node_id.clone(),
                status: status.to_string(),
                registered_at_ms: node.registered_at_ms,
            })).into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response(),
    }
}

async fn get_fleet_posture(State(svc): State<Arc<ServiceState>>) -> impl IntoResponse {
    let postures: Vec<FleetNodePosture> = svc.app.nodes
        .iter()
        .map(|entry| svc.app.calculate_posture(entry.key()))
        .collect();
    Json(json!({ "fleet": postures }))
}

async fn get_node_posture(
    State(svc): State<Arc<ServiceState>>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    let posture = svc.app.calculate_posture(&node_id);
    let now = now_ms();
    let cached = CachedFleetPosture::from_posture(&posture, now);
    // Refresh the cache on read so the gateway interceptor has a fresh entry.
    if let Ok(mut guard) = svc.posture_cache.write() {
        *guard = Some(cached);
    }
    Json(posture)
}

async fn register_dependencies(
    State(svc): State<Arc<ServiceState>>,
    Json(req): Json<RegisterDependenciesRequest>,
) -> impl IntoResponse {
    if !svc.app.mode.allows_mutation() {
        return (StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "instance is in passive standby mode" }))).into_response();
    }
    if svc.app.persist_and_insert_deps(&req.node_id, req.depends_on).is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to persist dependencies" }))).into_response();
    }

    // Snapshot posture after topology change (best-effort event log).
    let posture = svc.app.calculate_posture(&req.node_id);
    let now = now_ms();
    if let Ok(posture_json) = serde_json::to_string(&posture) {
        if let Ok(store) = svc.app.store.lock() {
            let _ = store.save_posture_event(
                &req.node_id, "DEPENDENCY_UPDATED", &posture_json, None, now,
            );
        }
    }

    (StatusCode::OK, Json(json!({ "node_id": req.node_id, "dependencies_registered": true }))).into_response()
}

async fn get_node_history(
    State(svc): State<Arc<ServiceState>>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    match svc.app.store.lock() {
        Ok(store) => match store.load_node_history(&node_id) {
            Ok(history) => Json(json!({ "node_id": node_id, "history": history })).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
                       Json(json!({ "error": "failed to load history" }))).into_response(),
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
                   Json(json!({ "error": "store lock poisoned" }))).into_response(),
    }
}

async fn get_node_flap_status(
    State(svc): State<Arc<ServiceState>>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    let five_minutes_ago = now_ms().saturating_sub(300_000);
    match svc.app.store.lock() {
        Ok(store) => match store.count_recent_posture_events(&node_id, five_minutes_ago) {
            Ok(count) => {
                let status = FlapStatus {
                    node_id: node_id.clone(),
                    flapping: count >= 3,
                    event_count_5m: count,
                };
                Json(status).into_response()
            }
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
                       Json(json!({ "error": "failed to query events" }))).into_response(),
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
                   Json(json!({ "error": "store lock poisoned" }))).into_response(),
    }
}

// --- v1.1 handlers ----------------------------------------------------------

async fn verify_audit_chain(
    State(svc): State<Arc<ServiceState>>,
) -> impl IntoResponse {
    match svc.app.store.lock() {
        Ok(mut store) => match store.verify_audit_chain_integrity() {
            Ok(valid) => Json(json!({ "valid": valid })).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
                       Json(json!({ "error": "audit chain query failed" }))).into_response(),
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
                   Json(json!({ "error": "store lock poisoned" }))).into_response(),
    }
}

async fn evaluate_action_filter(
    State(svc): State<Arc<ServiceState>>,
    Json(claim): Json<ActionClaim>,
) -> impl IntoResponse {
    let posture = svc.posture_cache
        .read()
        .ok()
        .and_then(|g| g.as_ref().map(|c| c.propagated_status.clone()))
        .unwrap_or(aegis_runtime_sdk::verifier::FleetPosture::LockedOut);

    let decision = evaluate_action_claim(claim.clone(), posture);

    if !decision.allowed {
        let event = json!({
            "target_node": claim.target_node,
            "action_type": claim.action_type,
            "risk_class": claim.risk_class,
            "reason": decision.reason,
        });
        if let Ok(mut store) = svc.app.store.lock() {
            let _ = store.save_posture_event_chained(
                "action_filter", "ACTION_FILTER_DENIED",
                &event.to_string(), Some("action denied"), now_ms(),
            );
        }
    }
    Json(decision).into_response()
}

async fn evaluate_industrial_adapter(
    State(svc): State<Arc<ServiceState>>,
    Json(event): Json<IndustrialEvent>,
) -> impl IntoResponse {
    let posture = svc.posture_cache
        .read()
        .ok()
        .and_then(|g| g.as_ref().map(|c| c.propagated_status.clone()))
        .unwrap_or(aegis_runtime_sdk::verifier::FleetPosture::LockedOut);

    let asset_id = event.asset_id.clone();
    let protocol = format!("{:?}", event.protocol);
    let operation = event.operation.clone();
    let address = event.address.clone();
    let risk_class = event.risk_class.clone();

    let decision = evaluate_industrial_event(event, posture);

    if !decision.allowed {
        let audit = json!({
            "asset_id": asset_id,
            "protocol": protocol,
            "operation": operation,
            "address": address,
            "risk_class": risk_class,
            "reason": decision.reason,
        });
        if let Ok(mut store) = svc.app.store.lock() {
            let _ = store.save_posture_event_chained(
                "industrial_adapter", "INDUSTRIAL_ACTION_DENIED",
                &audit.to_string(), Some("industrial action denied"), now_ms(),
            );
        }
    }
    Json(decision).into_response()
}

async fn submit_federated_report(
    State(svc): State<Arc<ServiceState>>,
    Json(report): Json<FederatedTrustReport>,
) -> impl IntoResponse {
    let received_at_ms = now_ms();
    let evaluation = evaluate_federated_report(&report, received_at_ms);

    if evaluation.accepted {
        let posture_json = match serde_json::to_string(&report.posture) {
            Ok(s) => s,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR,
                              Json(json!({ "error": "posture serialization failed" }))).into_response(),
        };
        match svc.app.store.lock() {
            Ok(mut store) => {
                if store.save_federated_report_chained(
                    &report.source_controller_id,
                    &report.asset_id,
                    &posture_json,
                    report.issued_at_ms,
                    report.expires_at_ms,
                    received_at_ms,
                ).is_err() {
                    return (StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": "failed to persist federated report" }))).into_response();
                }
            }
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR,
                              Json(json!({ "error": "store lock poisoned" }))).into_response(),
        }
    }
    Json(evaluation).into_response()
}

async fn get_federated_reports(
    State(svc): State<Arc<ServiceState>>,
    Path(asset_id): Path<String>,
) -> impl IntoResponse {
    match svc.app.store.lock() {
        Ok(store) => match store.load_federated_reports_for_asset(&asset_id) {
            Ok(reports) => Json(json!({ "asset_id": asset_id, "reports": reports })).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
                       Json(json!({ "error": "failed to load reports" }))).into_response(),
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
                   Json(json!({ "error": "store lock poisoned" }))).into_response(),
    }
}

// --- Entry point ------------------------------------------------------------

#[tokio::main]
async fn main() {
    let db_path = std::env::var("AEGIS_DB_PATH")
        .unwrap_or_else(|_| "aegis_verifier.sqlite".to_string());
    let listen_addr = std::env::var("AEGIS_VERIFIER_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8090".to_string());

    let store = VerifierStore::new(&db_path)
        .expect("failed to initialize verifier store");

    let mode = VerifierOperationMode::from_env();
    println!("Aegis Verifier starting in {mode:?} mode (db: {db_path})");

    let app_state = Arc::new(AppState::new(store, mode));

    // Boot hydration — load persisted nodes and dependency graph into memory.
    // Mutex is released before the server starts; the lock window is startup-only.
    {
        let guard = app_state.store.lock()
            .expect("verifier store lock poisoned during boot hydration");

        for node in guard.load_nodes().expect("failed to load persisted nodes") {
            app_state.nodes.insert(node.node_id.clone(), node);
        }
        for (node_id, deps) in guard.load_dependencies()
            .expect("failed to load persisted dependencies")
        {
            app_state.dependency_graph.insert(node_id, deps);
        }
    }

    let svc_state = Arc::new(ServiceState {
        app: app_state,
        posture_cache: Arc::new(std::sync::RwLock::new(None)),
    });

    // Mutation routes require Bearer token auth (AEGIS_ADMIN_TOKEN env var).
    // export_backup is read-only but returns the full trust-fabric state dump —
    // it is admin-protected so an unauthenticated caller cannot exfiltrate it.
    let admin_routes = Router::new()
        .route("/attestation/register", post(register_node))
        .route("/fleet/dependencies", post(register_dependencies))
        .route("/system/backup/export", post(export_backup))
        .route("/system/audit/verify", get(verify_audit_chain))
        .route("/action_filter/evaluate", post(evaluate_action_filter))
        .route("/industrial/evaluate", post(evaluate_industrial_adapter))
        .route("/federation/reports/submit", post(submit_federated_report))
        .layer(middleware::from_fn(require_admin_token));

    // Challenge and verify are unauthenticated — the challenge-response protocol
    // itself provides the attestation guarantee.
    let attestation_routes = Router::new()
        .route("/attestation/challenge/:node_id", post(issue_challenge))
        .route("/attestation/verify", post(verify_attestation));

    // Liveness/readiness probes — always public, no auth, minimal I/O.
    let probe_routes = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready));

    // Read-only routes need no auth.
    let read_routes = Router::new()
        .route("/attestation/status/:node_id", get(get_node_status))
        .route("/fleet/posture", get(get_fleet_posture))
        .route("/fleet/posture/:node_id", get(get_node_posture))
        .route("/fleet/history/:node_id", get(get_node_history))
        .route("/fleet/flapping/:node_id", get(get_node_flap_status))
        .route("/federation/reports/:asset_id", get(get_federated_reports));

    let app = Router::new()
        .merge(probe_routes)
        .merge(admin_routes)
        .merge(attestation_routes)
        .merge(read_routes)
        .with_state(svc_state);

    println!("Aegis Verifier Service listening on {listen_addr} (db: {db_path})");
    let listener = tokio::net::TcpListener::bind(&listen_addr).await
        .expect("failed to bind listener");
    axum::serve(listener, app).await
        .expect("server error");
}
