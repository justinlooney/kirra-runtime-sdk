// crates/parko-core/src/control_loop.rs

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::backend::{InferenceBackend, ModelHandle};
use crate::commands::ControlCommand;
use crate::runtime::{RuntimeClock, RuntimeState};
use crate::scheduler::InferenceLoop;
use crate::sensor::SensorStream;
use crate::telemetry::PostureSnapshot;

/// Clock-driven control loop wrapping an InferenceLoop with a lifecycle
/// state machine.
///
/// This is one orchestration pattern over the primitives in parko-core.
/// Other consumers may prefer event-driven or externally-clocked patterns;
/// this is the canonical "pull a frame each tick, run inference, transition
/// state" loop suitable for demos and real-time control.
pub struct ControlLoop<B: InferenceBackend, S: SensorStream> {
    state: RuntimeState,
    inner: InferenceLoop<B>,
    sensor: S,
    clock: RuntimeClock,
}

impl<B, S> ControlLoop<B, S>
where
    B: InferenceBackend + 'static,
    S: SensorStream + 'static,
{
    pub fn new(
        backend: Arc<B>,
        model: ModelHandle,
        sensor: S,
        actuator_tx: mpsc::Sender<ControlCommand>,
        hz: f64,
    ) -> Self {
        let inner = InferenceLoop::new(backend, model, actuator_tx);
        Self {
            state: RuntimeState::Warmup,
            inner,
            sensor,
            clock: RuntimeClock::new(hz),
        }
    }

    pub fn state(&self) -> RuntimeState {
        self.state
    }

    /// Force the state machine to a specific state; bypasses transition logic.
    /// Available under `cfg(test)` (unit tests) or the `test-helpers` feature
    /// (integration tests). Never compiled into release builds.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_state_for_test(&mut self, state: RuntimeState) {
        self.state = state;
    }

    /// Attach a KirraGovernor (or any `SafetyGovernor`) to this loop.
    /// The governor's decision takes precedence over the built-in degraded-mode
    /// clamp; both paths must not fire on the same tick (ADL-002).
    pub fn with_governor(mut self, governor: impl crate::safety::SafetyGovernor + 'static) -> Self {
        self.inner = self.inner.with_governor(governor);
        self
    }

    pub async fn tick(&mut self) -> Result<PostureSnapshot, String> {
        let _tick_status = self.clock.wait_for_next_tick().await;

        let Some(current_frame) = self.sensor.next_frame() else {
            self.state = RuntimeState::EmergencyStop;
            return Err("sensor stream exhausted".to_string());
        };

        let safety_posture = match self.state {
            RuntimeState::Nominal => crate::safety::SafetyPosture::Nominal,
            RuntimeState::EmergencyStop => crate::safety::SafetyPosture::LockedOut,
            _ => crate::safety::SafetyPosture::Degraded,
        };
        let snapshot = self.inner.tick(current_frame, safety_posture).await?;

        self.state = next_state(self.state, snapshot.active_state_degraded);

        Ok(snapshot)
    }
}

/// Pure state-transition function — extracted for testability.
///
/// Note: Recovery is a single-tick hysteresis state. A real safety
/// integration would likely require N consecutive non-degraded ticks
/// before fully transitioning to Nominal.
fn next_state(current: RuntimeState, degraded: bool) -> RuntimeState {
    match current {
        RuntimeState::Initializing => RuntimeState::Warmup,
        RuntimeState::Warmup => {
            if degraded {
                RuntimeState::Warmup
            } else {
                RuntimeState::Nominal
            }
        }
        RuntimeState::Nominal => {
            if degraded {
                RuntimeState::Degraded
            } else {
                RuntimeState::Nominal
            }
        }
        RuntimeState::Degraded => {
            if degraded {
                RuntimeState::Degraded
            } else {
                RuntimeState::Recovery
            }
        }
        RuntimeState::Recovery => {
            if degraded {
                RuntimeState::Degraded
            } else {
                RuntimeState::Nominal
            }
        }
        // EmergencyStop is terminal; no transitions out.
        RuntimeState::EmergencyStop => RuntimeState::EmergencyStop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmup_stays_warmup_while_degraded() {
        assert_eq!(next_state(RuntimeState::Warmup, true), RuntimeState::Warmup);
    }

    #[test]
    fn warmup_transitions_to_nominal_when_healthy() {
        assert_eq!(
            next_state(RuntimeState::Warmup, false),
            RuntimeState::Nominal
        );
    }

    #[test]
    fn nominal_transitions_to_degraded_when_degraded() {
        assert_eq!(
            next_state(RuntimeState::Nominal, true),
            RuntimeState::Degraded
        );
    }

    #[test]
    fn nominal_stays_nominal_when_healthy() {
        assert_eq!(
            next_state(RuntimeState::Nominal, false),
            RuntimeState::Nominal
        );
    }

    #[test]
    fn degraded_transitions_to_recovery_when_healthy() {
        assert_eq!(
            next_state(RuntimeState::Degraded, false),
            RuntimeState::Recovery
        );
    }

    #[test]
    fn degraded_stays_degraded_when_still_degraded() {
        assert_eq!(
            next_state(RuntimeState::Degraded, true),
            RuntimeState::Degraded
        );
    }

    #[test]
    fn recovery_transitions_to_nominal_when_confirmed_healthy() {
        assert_eq!(
            next_state(RuntimeState::Recovery, false),
            RuntimeState::Nominal
        );
    }

    #[test]
    fn recovery_returns_to_degraded_when_flapping() {
        assert_eq!(
            next_state(RuntimeState::Recovery, true),
            RuntimeState::Degraded
        );
    }

    #[test]
    fn emergency_stop_is_sticky() {
        assert_eq!(
            next_state(RuntimeState::EmergencyStop, false),
            RuntimeState::EmergencyStop
        );
        assert_eq!(
            next_state(RuntimeState::EmergencyStop, true),
            RuntimeState::EmergencyStop
        );
    }

    #[test]
    fn initializing_transitions_unconditionally_to_warmup() {
        assert_eq!(
            next_state(RuntimeState::Initializing, false),
            RuntimeState::Warmup
        );
        assert_eq!(
            next_state(RuntimeState::Initializing, true),
            RuntimeState::Warmup
        );
    }

    #[test]
    fn set_state_for_test_overrides_initial_warmup_state() {
        use std::collections::HashMap;
        use std::sync::Arc;
        use crate::backend::{
            BackendCapabilities, BackendError, InferenceBackend, ModelHandle,
            PrecisionMode, TensorBatch, TensorStorage,
        };
        use crate::sensor::{SensorFrame, SensorStream};

        struct FastBackend;
        impl InferenceBackend for FastBackend {
            fn load_model(&self, _: &str) -> Result<ModelHandle, BackendError> {
                Ok(ModelHandle {
                    model_id: "fast".into(),
                    input_shapes: HashMap::new(),
                    output_shapes: HashMap::new(),
                    expected_precision: PrecisionMode::FP32,
                })
            }
            fn run(&self, _: &ModelHandle, _: &TensorBatch) -> Result<TensorBatch<'static>, BackendError> {
                Ok(TensorBatch { named_tensors: HashMap::new(), metadata: HashMap::new() })
            }
            fn capabilities(&self) -> BackendCapabilities {
                BackendCapabilities {
                    precision_modes: vec![PrecisionMode::FP32],
                    supports_zero_copy_inputs: false,
                    max_batch_size: 1,
                    vendor_name: "fast",
                }
            }
        }

        struct EmptyStream;
        impl SensorStream for EmptyStream {
            fn next_frame(&mut self) -> Option<SensorFrame> { None }
        }

        let backend = Arc::new(FastBackend);
        let model = backend.load_model("").unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let mut control = ControlLoop::new(backend, model, EmptyStream, tx, 10.0);

        assert_eq!(control.state(), RuntimeState::Warmup, "initial state should be Warmup");
        control.set_state_for_test(RuntimeState::Degraded);
        assert_eq!(control.state(), RuntimeState::Degraded, "set_state_for_test must override Warmup");
    }
}
