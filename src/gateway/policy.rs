// src/gateway/policy.rs
//
// Re-exports the canonical OperationalCommand type and classify_http_command
// from posture_cache. All classification logic lives there; this module is
// the gateway-facing import surface.

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
    fn test_unknown_method_classifies_as_unknown() {
        // Unknown HTTP methods map to OperationalCommand::Unknown, which is
        // denied in ALL posture states including Nominal — closing the implicit
        // fallback bypass identified in the v1 gateway policy specification.
        assert_eq!(classify_http_command("PATCH",  "/unknown"), OperationalCommand::Unknown);
        assert_eq!(classify_http_command("FROBNI", "/x"),       OperationalCommand::Unknown);
    }
}
