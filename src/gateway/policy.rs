// src/gateway/policy.rs
//
// Re-exports the canonical OperationalCommand type and classify_http_command
// from posture_cache. All classification logic lives there; this module is
// the gateway-facing import surface.
//
// NOTE: there is no Unknown variant. Unrecognised methods return SystemMutation
// (the most conservative class) so unknown routes fail closed without needing
// a separate enum variant and an extra match arm.

pub use crate::posture_cache::{classify_http_command, OperationalCommand};

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
    fn test_unknown_method_fails_closed_as_system_mutation() {
        // Unknown HTTP methods map to SystemMutation — the most conservative
        // class — so they are blocked in every posture except Nominal.
        assert_eq!(classify_http_command("PATCH",  "/unknown"), OperationalCommand::SystemMutation);
        assert_eq!(classify_http_command("FROBNI", "/x"),       OperationalCommand::SystemMutation);
    }
}
