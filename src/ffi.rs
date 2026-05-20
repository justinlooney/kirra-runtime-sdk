// src/ffi.rs

use std::sync::{LazyLock, Mutex};
use crate::aegis_core::AegisKernelGovernor;
use crate::kinematics_contract::KinematicContract;
use crate::SafetyGovernor;

static GLOBAL_GOVERNOR: LazyLock<Mutex<AegisKernelGovernor<KinematicContract>>> = LazyLock::new(|| {
    let contract = KinematicContract {
        max_linear_velocity: 2.0,
        max_angular_velocity: 1.5,
        max_linear_acceleration: 0.5,
        fallback_linear_speed: 0.0,
    };
    Mutex::new(AegisKernelGovernor::new(contract, 0.0, -2.0, 2.0))
});

#[no_mangle]
pub extern "C" fn aegis_filter_move_velocity(demand: f64, dt: f64) -> f64 {
    let mut gov = GLOBAL_GOVERNOR.lock().unwrap();
    gov.evaluate(demand, dt).sanitized_scalar
}

#[no_mangle]
pub extern "C" fn aegis_filter_rotate_velocity(angular_demand: f64, _dt: f64) -> f64 {
    let gov = GLOBAL_GOVERNOR.lock().unwrap();
    let limit = gov.contract.max_angular_velocity;
    angular_demand.clamp(-limit, limit)
}

#[no_mangle]
pub extern "C" fn aegis_get_trust_score() -> u32 {
    GLOBAL_GOVERNOR.lock().unwrap().trust_engine.current_score
}

#[no_mangle]
pub extern "C" fn aegis_reset_state(token_ptr: *const u8, token_len: usize) -> i32 {
    if token_ptr.is_null() || token_len == 0 || token_len > 64 { return 0; }
    let runtime_auth_key = match std::env::var("AEGIS_SUPERVISOR_RESET_KEY") {
        Ok(val) if !val.is_empty() => val.into_bytes(),
        _ => return 0,
    };
    let token = unsafe { std::slice::from_raw_parts(token_ptr, token_len) };
    let mut gov = GLOBAL_GOVERNOR.lock().unwrap();
    match gov.trust_engine.authenticated_manual_reset(token, &runtime_auth_key, 0) {
        Ok(()) => 1,
        Err(_) => 0,
    }
}
