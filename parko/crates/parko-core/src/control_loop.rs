// crates/parko-core/src/control_loop.rs

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::backend::{InferenceBackend, ModelHandle};
use crate::clock::{Clock, WallClock};
use crate::commands::ControlCommand;
use crate::runtime::RuntimeState;
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
    /// Wall-clock abstraction. All timing reads use `clock.now_ms()` so
    /// tests can inject MockClock and advance time without sleeping (ADL-004).
    clock: Arc<dyn Clock>,
    tick_interval_ms: u64,
    /// `None` = never fired; `Some(t)` = last tick fired at wall-clock `t`.
    /// Stored as Option so the first tick always fires regardless of
    /// the clock's current value (including t=0 with MockClock).
    last_tick_ms: Option<u64>,
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
        assert!(
            hz.is_finite() && hz > 0.0,
            "ControlLoop hz must be positive finite, got {}",
            hz
        );
        let inner = InferenceLoop::new(backend, model, actuator_tx);
        Self {
            state: RuntimeState::Warmup,
            inner,
            sensor,
            clock: Arc::new(WallClock),
            tick_interval_ms: (1000.0 / hz).round() as u64,
            last_tick_ms: None,
        }
    }

    /// Override the clock. Primarily used in tests to inject a `MockClock`
    /// so tick-timing can be exercised without sleeping (ADL-004).
    pub fn with_clock(mut self, c: Arc<dyn Clock>) -> Self {
        self.clock = c;
        self
    }

    /// Override the tick interval in milliseconds.
    /// `#[cfg(test)]` — not compiled into release builds.
    #[cfg(test)]
    pub fn with_tick_interval_ms(mut self, ms: u64) -> Self {
        self.tick_interval_ms = ms;
        self
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

    /// Drive one control tick.
    ///
    /// Returns `Ok(None)` when the tick interval has not yet elapsed since the
    /// last fired tick — callers should poll again later. Returns
    /// `Ok(Some(snapshot))` when a tick fires. Returns `Err` only on
    /// unrecoverable failures (e.g. sensor stream exhausted).
    ///
    /// The first call always fires (`last_tick_ms` starts at 0). All
    /// subsequent calls use `clock.now_ms()` for interval gating (ADL-004).
    pub async fn tick(&mut self) -> Result<Option<PostureSnapshot>, String> {
        let now = self.clock.now_ms();
        if let Some(last) = self.last_tick_ms {
            if now.saturating_sub(last) < self.tick_interval_ms {
                return Ok(None);
            }
        }
        self.last_tick_ms = Some(now);

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

        Ok(Some(snapshot))
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
            PrecisionMode, TensorBatch,
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

    // ── PARK-005 clock tests ─────────────────────────────────────────────────

    /// MockClock controls tick firing without any wall-clock sleep (ADL-004).
    ///
    /// Verifies that at 50ms intervals:
    ///  - first tick fires at t=0
    ///  - tick does NOT fire at 40ms (interval not elapsed)
    ///  - tick DOES fire at 50ms (exactly one interval elapsed)
    ///  - four advances of 50ms each produce exactly four fired ticks
    #[tokio::test]
    async fn test_mock_clock_tick_count() {
        use std::collections::HashMap;
        use crate::backend::{
            BackendCapabilities, BackendError, InferenceBackend, ModelHandle,
            PrecisionMode, TensorBatch,
        };
        use crate::clock::MockClock;
        use crate::sensor::{SensorFrame, SensorStream};

        struct FastBackend2;
        impl InferenceBackend for FastBackend2 {
            fn load_model(&self, _: &str) -> Result<ModelHandle, BackendError> {
                Ok(ModelHandle {
                    model_id: "fast2".into(),
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
                    vendor_name: "fast2",
                }
            }
        }

        struct InfiniteStream { next_id: u64 }
        impl SensorStream for InfiniteStream {
            fn next_frame(&mut self) -> Option<SensorFrame> {
                self.next_id += 1;
                Some(SensorFrame::new(
                    self.next_id,
                    TensorBatch { named_tensors: HashMap::new(), metadata: HashMap::new() },
                ))
            }
        }

        let mock = MockClock::new(0);
        let backend = Arc::new(FastBackend2);
        let model = backend.load_model("").unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(32);

        let mut control = ControlLoop::new(backend, model, InfiniteStream { next_id: 0 }, tx, 20.0)
            .with_clock(Arc::new(mock.clone()))
            .with_tick_interval_ms(50);

        // t=0: first tick always fires (last_tick_ms starts at 0).
        let r = control.tick().await.unwrap();
        assert!(r.is_some(), "first tick must fire at t=0");

        // t=40ms: interval not yet elapsed (40 < 50).
        mock.advance(40);
        let r = control.tick().await.unwrap();
        assert!(r.is_none(), "tick at 40ms must not fire (interval not elapsed)");

        // t=50ms: exactly one interval; must fire.
        mock.advance(10);
        let r = control.tick().await.unwrap();
        assert!(r.is_some(), "tick at 50ms must fire (interval elapsed)");

        // Four more advances of 50ms each → exactly 4 fired ticks.
        let mut fired = 0usize;
        for _ in 0..4 {
            mock.advance(50);
            let r = control.tick().await.unwrap();
            if r.is_some() {
                fired += 1;
            }
        }
        assert_eq!(fired, 4, "four 50ms advances must produce exactly 4 fired ticks");
    }

    /// WallClock is the default when no with_clock() call is made.
    /// Verifies no panic and that the first tick fires (returns Some).
    #[tokio::test]
    async fn test_runtime_clock_default_smoke() {
        use std::collections::HashMap;
        use crate::backend::{
            BackendCapabilities, BackendError, InferenceBackend, ModelHandle,
            PrecisionMode, TensorBatch,
        };
        use crate::sensor::{SensorFrame, SensorStream};

        struct FastBackend3;
        impl InferenceBackend for FastBackend3 {
            fn load_model(&self, _: &str) -> Result<ModelHandle, BackendError> {
                Ok(ModelHandle {
                    model_id: "fast3".into(),
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
                    vendor_name: "fast3",
                }
            }
        }

        struct OneFrameStream { done: bool }
        impl SensorStream for OneFrameStream {
            fn next_frame(&mut self) -> Option<SensorFrame> {
                if self.done { return None; }
                self.done = true;
                Some(SensorFrame::new(
                    1,
                    TensorBatch { named_tensors: HashMap::new(), metadata: HashMap::new() },
                ))
            }
        }

        let backend = Arc::new(FastBackend3);
        let model = backend.load_model("").unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);

        // No with_clock() call — defaults to WallClock.
        let mut control = ControlLoop::new(backend, model, OneFrameStream { done: false }, tx, 20.0);

        // First tick always fires; WallClock returns a non-zero unix timestamp.
        let result = control.tick().await;
        assert!(result.is_ok(), "tick must not error: {:?}", result);
        let snapshot = result.unwrap();
        assert!(snapshot.is_some(), "first tick must fire with WallClock default");
    }
}
