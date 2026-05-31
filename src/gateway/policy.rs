// src/gateway/policy.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationalCommand {
    /// Safe reads: telemetry, metrics, health probes. Allowed in all postures.
    ReadTelemetry,
    /// Actuator writes and velocity commands. Denied when LockedOut.
    WriteState,
    /// Firmware, reboot, config mutations. Denied unless Nominal.
    SystemMutation,
    /// Unrecognised HTTP method. Denied in ALL postures (fail-closed).
    Unknown,
}

/// Classifies an HTTP request into an OperationalCommand based solely on method
/// and path prefix. No state access — pure function, always total.
// SAFETY: SG7 SG9 | REQ: doer-agnostic-classification | TEST: sg7_doer_agnostic_verdict_byte_identical_across_ingress_paths,test_safety_goal_sg_006_unknown_command_denial
// (No `source` field in the signature: the classifier is path/method-only,
//  so teleop vs planner ingress produces identical OperationalCommands —
//  SG7. Unknown HTTP method maps to OperationalCommand::Unknown which
//  feeds SG9 fail-closed at should_route_command.)
pub fn classify_http_command(method: &str, path: &str) -> OperationalCommand {
    match method {
        "GET" | "HEAD" => OperationalCommand::ReadTelemetry,

        "DELETE" => OperationalCommand::SystemMutation,

        "POST" | "PUT" => {
            if path.starts_with("/actuator") || path == "/cmd_vel" || path.starts_with("/cmd_vel/") {
                OperationalCommand::WriteState
            } else if path.starts_with("/firmware")
                || path == "/reboot"
                || path.starts_with("/reboot/")
                || path.starts_with("/config")
            {
                OperationalCommand::SystemMutation
            } else {
                // All other POST/PUT: treat as WriteState (state mutation, not
                // infrastructure mutation). Attestation, federation, action-filter
                // endpoints all fall here and are further gated by auth middleware.
                OperationalCommand::WriteState
            }
        }

        _ => OperationalCommand::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classifies_read_telemetry() {
        assert_eq!(classify_http_command("GET", "/telemetry/status"), OperationalCommand::ReadTelemetry);
        assert_eq!(classify_http_command("GET", "/metrics"),          OperationalCommand::ReadTelemetry);
        assert_eq!(classify_http_command("GET", "/health/live"),      OperationalCommand::ReadTelemetry);
    }

    #[test]
    fn test_classifies_cmd_vel_as_write_state() {
        assert_eq!(classify_http_command("POST", "/cmd_vel"), OperationalCommand::WriteState);
    }

    #[test]
    fn test_classifies_actuator_as_write_state() {
        assert_eq!(classify_http_command("POST", "/actuator/servo"), OperationalCommand::WriteState);
        assert_eq!(classify_http_command("PUT",  "/actuator/valve"), OperationalCommand::WriteState);
    }

    #[test]
    fn test_classifies_system_mutations() {
        assert_eq!(classify_http_command("POST",   "/firmware/update"), OperationalCommand::SystemMutation);
        assert_eq!(classify_http_command("POST",   "/reboot"),          OperationalCommand::SystemMutation);
        assert_eq!(classify_http_command("PUT",    "/config/network"),  OperationalCommand::SystemMutation);
        assert_eq!(classify_http_command("DELETE", "/anything"),        OperationalCommand::SystemMutation);
    }

    #[test]
    fn test_unknown_method_classifies_as_unknown() {
        // Unknown HTTP methods map to OperationalCommand::Unknown, which is
        // denied in ALL posture states including Nominal — closing the implicit
        // fallback bypass identified in the v1 gateway policy specification.
        assert_eq!(classify_http_command("PATCH",  "/unknown"), OperationalCommand::Unknown);
        assert_eq!(classify_http_command("FROBNI", "/x"),       OperationalCommand::Unknown);
    }

    // -------------------------------------------------------------------------
    // MC/DC pair-completion tests (S3 / #115 — KIRRA-OCCY-MCDC-001).
    //
    // The POST/PUT OR-chain at l.29 has three alternates
    //   (a) path.starts_with("/actuator")
    //   (b) path == "/cmd_vel"
    //   (c) path.starts_with("/cmd_vel/")
    // and the SystemMutation OR-chain at l.31–34 has four alternates with the
    // same shape. The existing tests cover (a), (b), and "/firmware",
    // "/reboot" exact, "/config" prefix. The two undemonstrated independent
    // effects are (c) — a sub-path of /cmd_vel/ — and the "/reboot/" prefix.
    // -------------------------------------------------------------------------

    /// MC/DC: independent-effect of `path.starts_with("/cmd_vel/")`
    /// (l.29 third OR clause). All prior clauses are false; this one
    /// decides the verdict.
    #[test]
    fn test_cmd_vel_sub_path_classifies_as_write_state() {
        assert_eq!(
            classify_http_command("POST", "/cmd_vel/replay"),
            OperationalCommand::WriteState,
            "/cmd_vel/* sub-path must classify as WriteState (third OR clause)"
        );
        assert_eq!(
            classify_http_command("PUT", "/cmd_vel/buffer"),
            OperationalCommand::WriteState
        );
    }

    /// MC/DC: independent-effect of `path.starts_with("/reboot/")`
    /// (l.33 third OR clause in the SystemMutation chain). /firmware
    /// prefix false, exact /reboot false, prefix /reboot/ decides.
    #[test]
    fn test_reboot_sub_path_classifies_as_system_mutation() {
        assert_eq!(
            classify_http_command("POST", "/reboot/now"),
            OperationalCommand::SystemMutation,
            "/reboot/* sub-path must classify as SystemMutation (third OR clause)"
        );
        assert_eq!(
            classify_http_command("POST", "/reboot/scheduled/15s"),
            OperationalCommand::SystemMutation
        );
    }
}
