// src/federation.rs

use serde::{Deserialize, Serialize};
use crate::verifier::FleetPosture;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FederatedTrustReport {
    pub source_controller_id: String,
    pub asset_id: String,
    pub posture: FleetPosture,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub signature_b64: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReportEvaluation {
    pub accepted: bool,
    pub reason: String,
}

pub fn evaluate_federated_report(report: &FederatedTrustReport, current_time_ms: u64) -> ReportEvaluation {
    if report.source_controller_id.is_empty() {
        return ReportEvaluation {
            accepted: false,
            reason: "MISSING_SOURCE_CONTROLLER".to_string(),
        };
    }
    if report.asset_id.is_empty() {
        return ReportEvaluation {
            accepted: false,
            reason: "MISSING_ASSET_ID".to_string(),
        };
    }
    if report.issued_at_ms > current_time_ms {
        return ReportEvaluation {
            accepted: false,
            reason: "REPORT_TIMELINE_FUTURE_INVALID".to_string(),
        };
    }
    if current_time_ms >= report.expires_at_ms {
        return ReportEvaluation {
            accepted: false,
            reason: "REPORT_STALE_EXPIRED".to_string(),
        };
    }

    ReportEvaluation {
        accepted: true,
        reason: "FEDERATED_OBSERVATION_RECORDED".to_string(),
    }
}
