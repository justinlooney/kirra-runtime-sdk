// src/posture_cache.rs

use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::verifier::{FleetNodePosture, FleetPosture, NodeTrustState};
use serde::{Deserialize, Serialize};

/// Maximum age of a cached posture entry before it is considered stale.
///
/// Rationale: the verifier polling loop refreshes the cache at 1 Hz (1000 ms period).
/// A 2000 ms TTL provides one full polling interval of tolerance for scheduling jitter
/// or transient CPU starvation before the gateway begins denying commands.
///
/// For environments where the actuated hardware has sub-second response requirements
/// (e.g., servo loops, hydraulic valves), reduce this to match the worst-case latency
/// budget — the TTL must be less than the time it takes for a physical state change
/// to become hazardous if undetected.
pub const CACHE_TTL_MS: u64 = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFleetPosture {
    pub node_id: String,
    pub local_status: NodeTrustState,
    pub propagated_status: FleetPosture,
    pub blocked_by: Vec<String>,
    pub updated_at_epoch_ms: u64,
}

impl CachedFleetPosture {
    pub fn from_posture(posture: &FleetNodePosture, now_ms: u64) -> Self {
        Self {
            node_id: posture.node_id.clone(),
            local_status: posture.local_status.clone(),
            propagated_status: posture.propagated_status.clone(),
            blocked_by: posture.blocked_by.clone(),
            updated_at_epoch_ms: now_ms,
        }
    }
}

pub type SharedPostureCache = Arc<RwLock<Option<CachedFleetPosture>>>;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Returns true only if the cached posture is fresh AND Nominal.
/// All other conditions — stale, missing, poisoned lock — deny routing.
pub fn should_route_sensitive_command(cache: &CachedFleetPosture, now_ms: u64) -> bool {
    let age_ms = now_ms.saturating_sub(cache.updated_at_epoch_ms);
    if age_ms > CACHE_TTL_MS { return false; }
    matches!(cache.propagated_status, FleetPosture::Nominal)
}

/// Fail-closed gateway check. Returns false on any form of uncertainty:
///   - RwLock poisoned (writer panicked)           → deny
///   - Cache not yet populated                     → deny
///   - Cache stale (> CACHE_TTL_MS)                → deny
///   - Posture is Degraded or LockedOut            → deny
pub fn should_route_from_cache(cache: &SharedPostureCache) -> bool {
    let now = now_ms();
    let guard = match cache.read() {
        Ok(g) => g,
        Err(_) => return false,
    };
    match guard.as_ref() {
        Some(posture) => should_route_sensitive_command(posture, now),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// HTTP command classification and posture-aware routing
// ---------------------------------------------------------------------------

/// Broad classification of an HTTP request by its operational impact.
///
/// The classification is derived from method + path only. Headers must never
/// be the primary signal because they are caller-supplied and trivially forged.
/// A header override (e.g., X-Aegis-Command-Class) may be consulted as a
/// secondary signal for known-internal services, but it can only *downgrade*
/// the class, never upgrade it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationalCommand {
    ReadTelemetry,
    WriteState,
    SystemMutation,
    /// Any HTTP method or path pattern that cannot be positively identified.
    /// Denied in all posture states, including Nominal, to prevent implicit
    /// fallback paths from becoming exploitable bypass vectors.
    Unknown,
}

/// Classify an HTTP request into an `OperationalCommand` using method + path.
///
/// Path matching rules (first match wins):
///
/// | Method       | Path prefix / exact       | Class           |
/// |--------------|---------------------------|-----------------|
/// | GET          | /metrics/*, /telemetry/*, /health/* | ReadTelemetry |
/// | GET          | (any other)               | ReadTelemetry   |
/// | POST         | /actuator/*, /cmd_vel, /control/* | WriteState |
/// | PUT          | /actuator/*               | WriteState      |
/// | POST         | /firmware/*, /reboot      | SystemMutation  |
/// | PUT          | /config/*                 | SystemMutation  |
/// | DELETE       | (any)                     | SystemMutation  |
/// | unknown      | (any)                     | SystemMutation  |
///
/// Query strings are stripped before matching. Method comparison is
/// case-insensitive.
pub fn classify_http_command(method: &str, path: &str) -> OperationalCommand {
    // Strip query string — classification is path-structure only.
    let path = path.split('?').next().unwrap_or(path);
    let method = method.to_ascii_uppercase();

    match method.as_str() {
        "GET" => {
            // All GETs are reads; HTTP semantics prohibit side effects.
            // The explicit prefixes (/metrics, /telemetry, /health) are the
            // canonical read paths. Any other GET is still a read — the
            // catch-all keeps classification simple and safe.
            OperationalCommand::ReadTelemetry
        }
        "POST" => {
            if path.starts_with("/actuator")
                || path == "/cmd_vel"
                || path.starts_with("/control")
            {
                OperationalCommand::WriteState
            } else if path.starts_with("/firmware") || path == "/reboot" {
                OperationalCommand::SystemMutation
            } else {
                // Unknown POST paths can mutate state — treat conservatively.
                OperationalCommand::WriteState
            }
        }
        "PUT" => {
            if path.starts_with("/actuator") {
                OperationalCommand::WriteState
            } else if path.starts_with("/config") {
                OperationalCommand::SystemMutation
            } else {
                OperationalCommand::WriteState
            }
        }
        "DELETE" => OperationalCommand::SystemMutation,
        _ => {
            // Unknown HTTP methods have undefined semantics — classified as Unknown
            // so the routing matrix denies them in ALL postures, including Nominal.
            OperationalCommand::Unknown
        }
    }
}

/// Decide whether to forward a command based on fleet posture and cache freshness.
///
/// Policy (ordered by severity, first match wins):
///
/// | Condition                   | ReadTelemetry | WriteState | SystemMutation | Unknown |
/// |-----------------------------|---------------|------------|----------------|---------|
/// | Stale (> CACHE_TTL_MS)      | deny          | deny       | deny           | deny    |
/// | LockedOut                   | deny          | deny       | deny           | deny    |
/// | Degraded                    | allow         | deny       | deny           | deny    |
/// | Nominal                     | allow         | allow      | allow          | deny    |
///
/// `Unknown` is denied in all posture states, including Nominal, to close the
/// implicit fallback path identified in the v1 gateway policy specification.
///
/// "Missing" (cache is `None`) must be handled by the caller before invoking
/// this function. `should_route_from_cache` handles that case.
pub fn should_route_command(
    cache: &CachedFleetPosture,
    now_ms: u64,
    command: OperationalCommand,
) -> bool {
    // Unknown commands are unconditionally denied regardless of posture or freshness.
    if command == OperationalCommand::Unknown {
        return false;
    }

    let age_ms = now_ms.saturating_sub(cache.updated_at_epoch_ms);
    if age_ms > CACHE_TTL_MS {
        return false;
    }

    match cache.propagated_status {
        FleetPosture::Nominal => true,
        FleetPosture::Degraded => matches!(command, OperationalCommand::ReadTelemetry),
        FleetPosture::LockedOut => false,
    }
}
