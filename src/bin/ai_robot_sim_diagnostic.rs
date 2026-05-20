// src/bin/ai_robot_sim_diagnostic.rs
// Simulation diagnostic tool for Aegis safety governor validation.

use aegis_runtime_sdk::aegis_core::AegisKernelGovernor;
use aegis_runtime_sdk::kinematics_contract::KinematicContract;
use aegis_runtime_sdk::action_filter::ActionFilter;
use aegis_runtime_sdk::action_policy::UnstructuredTextParser;

fn main() {
    println!("=== Aegis Sim Diagnostic v1.0-rc20 ===\n");

    let contract = KinematicContract {
        max_linear_velocity: 2.0,
        max_angular_velocity: 1.5,
        max_linear_acceleration: 0.5,
        fallback_linear_speed: 0.0,
    };

    let mut governor = AegisKernelGovernor::new(contract, 0.0, -2.0, 2.0);
    let filter = ActionFilter::new(contract);
    let parser = UnstructuredTextParser;

    let test_vectors = [
        r#"{"action": "MOVE", "velocity": 1.5}"#,
        r#"{"action": "ROTATE", "angular_velocity": 2.0}"#,
        r#"{"action": "MOVE", "velocity": -3.0}"#,
        r#"{"action": "STOP"}"#,
    ];

    for json in &test_vectors {
        print!("Input: {} => ", json);
        match parser.parse_llm_json_intent(json) {
            Ok(action) => {
                let output = filter.process_agent_intent(&mut governor, action, 0.1);
                println!("{:?} | {}", output.resolution, output.narrative);
            }
            Err(e) => println!("ParseError({})", e),
        }
    }

    println!("\nFinal trust score: {}", governor.trust_engine.current_score);
}
