// src/action_filter.rs

use crate::{SafetyGovernor, ActionResolution, AgentAction, SafetyContract};

pub struct FilterOutput {
    pub resolution: ActionResolution,
    pub sanitized_action: AgentAction,
    pub narrative: String,
}

pub struct ActionFilter<C: SafetyContract> {
    pub contract: C,
}

impl<C: SafetyContract> ActionFilter<C> {
    pub fn new(contract: C) -> Self {
        Self { contract }
    }

    pub fn process_agent_intent<G: SafetyGovernor>(
        &self,
        governor: &mut G,
        action: AgentAction,
        dt: f64,
    ) -> FilterOutput {
        match action {
            AgentAction::MoveLinear { velocity } => {
                let intercept = governor.evaluate(velocity, dt);
                let mutated = (intercept.sanitized_scalar - velocity).abs() > 0.001;

                let resolution = if intercept.was_unsafe_attempt && governor.trust_mode() == crate::TrustMode::LockedOut {
                    ActionResolution::Failsafe
                } else if mutated {
                    ActionResolution::Mutated
                } else {
                    ActionResolution::Approved
                };

                FilterOutput {
                    resolution,
                    sanitized_action: AgentAction::MoveLinear { velocity: intercept.sanitized_scalar },
                    narrative: intercept.mitigation_narrative,
                }
            }
            AgentAction::Rotate { angular_velocity } => {
                if angular_velocity.abs() > self.contract.max_angular_rate() {
                    return FilterOutput {
                        resolution: ActionResolution::Rejected,
                        sanitized_action: AgentAction::HoldPosition,
                        narrative: "REJECTED: Angular rate violates safety envelope.".to_string(),
                    };
                }
                FilterOutput {
                    resolution: ActionResolution::Approved,
                    sanitized_action: AgentAction::Rotate { angular_velocity },
                    narrative: "PASSTHROUGH_NORMAL".to_string(),
                }
            }
            _ => FilterOutput {
                resolution: ActionResolution::Approved,
                sanitized_action: AgentAction::HoldPosition,
                narrative: "PASSTHROUGH_NORMAL".to_string(),
            },
        }
    }
}

// --- Posture-aware action claim evaluation (post-v1 extension) ---------------

use crate::verifier::FleetPosture;
use crate::gateway::cmd_vel::{validate_cmd_vel, CmdVel, DEFAULT_CMD_VEL_LIMITS};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ActionClaim {
    pub action_type: String,
    pub target_node: String,
    pub risk_class: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ActionDecision {
    pub allowed: bool,
    pub reason: String,
}

pub fn evaluate_action_claim(claim: ActionClaim, posture: FleetPosture) -> ActionDecision {
    if claim.action_type.is_empty() {
        return ActionDecision { allowed: false, reason: "MISSING_ACTION_TYPE".to_string() };
    }
    if claim.target_node.is_empty() {
        return ActionDecision { allowed: false, reason: "MISSING_TARGET_NODE".to_string() };
    }

    match posture {
        FleetPosture::Nominal => match claim.action_type.as_str() {
            "cmd_vel" => match serde_json::from_value::<CmdVel>(claim.payload.clone()) {
                Ok(cmd) => {
                    if validate_cmd_vel(&cmd, DEFAULT_CMD_VEL_LIMITS) {
                        ActionDecision { allowed: true, reason: "NOMINAL_VALID_KINEMATICS".to_string() }
                    } else {
                        ActionDecision { allowed: false, reason: "KINEMATIC_ENVELOPE_BREACH".to_string() }
                    }
                }
                Err(_) => ActionDecision { allowed: false, reason: "MALFORMED_CMD_VEL_PAYLOAD".to_string() },
            },
            _ => ActionDecision { allowed: false, reason: "UNKNOWN_ACTION_TYPE".to_string() },
        },
        FleetPosture::Degraded => {
            if claim.risk_class == "kinetic_write" || claim.action_type == "cmd_vel" {
                ActionDecision { allowed: false, reason: "DEGRADED_POSTURE_KINETIC_DENIED".to_string() }
            } else if claim.action_type == "read_telemetry" {
                ActionDecision { allowed: true, reason: "DEGRADED_READ_ONLY_PERMITTED".to_string() }
            } else {
                ActionDecision { allowed: false, reason: "DEGRADED_UNSUPPORTED_CLAIM_TYPE".to_string() }
            }
        }
        FleetPosture::LockedOut => ActionDecision {
            allowed: false,
            reason: "LOCKEDOUT_POSTURE_ABSOLUTE_DENIAL".to_string(),
        },
    }
}
