// src/protocol_adapter.rs

use serde::{Deserialize, Serialize};
use crate::action_filter::{evaluate_action_claim, ActionClaim, ActionDecision};
use crate::verifier::FleetPosture;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndustrialProtocol {
    Modbus,
    OpcUa,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndustrialEvent {
    pub protocol: IndustrialProtocol,
    pub asset_id: String,
    pub operation: String,
    pub address: String,
    pub value: i64,
    pub risk_class: String,
}

pub fn map_industrial_event_to_claim(event: &IndustrialEvent) -> Result<ActionClaim, &'static str> {
    if event.asset_id.is_empty() {
        return Err("MISSING_ASSET_ID");
    }

    let mapped_action_type = match (event.protocol.clone(), event.operation.as_str()) {
        (IndustrialProtocol::Modbus, "write_register")
        | (IndustrialProtocol::Modbus, "coil_write") => "cmd_vel",
        (IndustrialProtocol::Modbus, "read_register") => "read_telemetry",
        (IndustrialProtocol::OpcUa, "write_node")
        | (IndustrialProtocol::OpcUa, "call_method") => "cmd_vel",
        (IndustrialProtocol::OpcUa, "read_node") => "read_telemetry",
        _ => return Err("UNSUPPORTED_INDUSTRIAL_OPERATION"),
    };

    let payload = serde_json::json!({
        "linear_x": if mapped_action_type == "cmd_vel" && event.value != 0 { 0.25 } else { 0.0 },
        "linear_y": 0.0,
        "linear_z": 0.0,
        "angular_x": 0.0,
        "angular_y": 0.0,
        "angular_z": if mapped_action_type == "cmd_vel" && event.value > 1 { 0.4 } else { 0.0 },
        "industrial_context": {
            "address": event.address,
            "raw_value": event.value,
        }
    });

    Ok(ActionClaim {
        action_type: mapped_action_type.to_string(),
        target_node: event.asset_id.clone(),
        risk_class: event.risk_class.clone(),
        payload,
    })
}

pub fn evaluate_industrial_event(event: IndustrialEvent, posture: FleetPosture) -> ActionDecision {
    match map_industrial_event_to_claim(&event) {
        Ok(claim) => evaluate_action_claim(claim, posture),
        Err(err) => ActionDecision {
            allowed: false,
            reason: format!("ADAPTER_TRANSLATION_FAILURE: {}", err),
        },
    }
}
