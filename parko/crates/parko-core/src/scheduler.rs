// crates/parko-core/src/scheduler.rs

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task;

use crate::backend::{BackendCapabilities, InferenceBackend, ModelHandle, PrecisionMode, TensorBatch};
use crate::commands::ControlCommand;
use crate::sensor::SensorFrame;
use crate::telemetry::{CumulativeJitterEvaluator, PostureSnapshot, RuntimeTelemetry, ThermalState};

/// Thresholds for degraded-mode detection.
///
/// TODO: expose externally or load from config.
#[derive(Debug, Clone)]
pub struct DegradationThresholds {
    pub max_inference_latency_ms: u64,
    pub max_jitter_ms: f64,
    pub max_frame_age_ms: u64,
    pub max_linear_velocity_mps: f64,
}

impl Default for DegradationThresholds {
    fn default() -> Self {
        Self {
            max_inference_latency_ms: 150,
            max_jitter_ms: 25.0,
            max_frame_age_ms: 100,
            max_linear_velocity_mps: 1.5,
        }
    }
}

/// Inference loop with one-tick-delayed actuator publication.
///
/// NOTE: This contains a placeholder degraded-mode policy.
/// Real physical envelope enforcement belongs in the Aegis safety kernel.
pub struct InferenceLoop<B: InferenceBackend> {
    backend: Arc<B>,
    model: Arc<ModelHandle>,
    actuator_tx: mpsc::Sender<ControlCommand>,
    last_validated_command: Option<ControlCommand>,
    last_frame_id: Option<u64>,
    dropped_frame_counter: u64,
    jitter_tracker: CumulativeJitterEvaluator,
    thresholds: DegradationThresholds,
    cached_capabilities: BackendCapabilities,
}

impl<B: InferenceBackend + 'static> InferenceLoop<B> {
    pub fn new(
        backend: Arc<B>,
        model: ModelHandle,
        actuator_tx: mpsc::Sender<ControlCommand>,
    ) -> Self {
        let cached_capabilities = backend.capabilities();
        Self {
            backend,
            model: Arc::new(model),
            actuator_tx,
            last_validated_command: None,
            last_frame_id: None,
            dropped_frame_counter: 0,
            jitter_tracker: CumulativeJitterEvaluator::new(),
            thresholds: DegradationThresholds::default(),
            cached_capabilities,
        }
    }

    pub async fn tick(&mut self, current_frame: SensorFrame) -> Result<PostureSnapshot, String> {
        let loop_start_ms = crate::sensor::current_time_ms();

        // Flush previously validated command (frame N-1).
        if let Some(ref cmd) = self.last_validated_command {
            self.actuator_tx
                .send(cmd.clone())
                .await
                .map_err(|e| format!("actuator channel closed: {}", e))?;
        }

        // Track dropped frames.
        if let Some(prev) = self.last_frame_id {
            if current_frame.frame_id > prev + 1 {
                let gap = current_frame.frame_id - prev - 1;
                self.dropped_frame_counter =
                    self.dropped_frame_counter.saturating_add(gap);
            }
        }
        self.last_frame_id = Some(current_frame.frame_id);

        // Frame age + payload size.
        let frame_age_ms = loop_start_ms.saturating_sub(current_frame.timestamp_ms);
        let tensor_payload_bytes = current_frame
            .payload
            .named_tensors
            .values()
            .map(|s| s.as_slice().len() * std::mem::size_of::<f32>())
            .sum();

        let backend_ref = Arc::clone(&self.backend);
        let model_handle = Arc::clone(&self.model);
        let payload = current_frame.payload;

        // Inference on blocking thread.
        let (output_tensors, inference_latency_ms) = task::spawn_blocking(move || {
            let start = std::time::Instant::now();
            let out = backend_ref.run(&model_handle, &payload);
            let elapsed = start.elapsed().as_millis() as u64;
            (out, elapsed)
        })
        .await
        .map_err(|e| format!("inference worker join failure: {}", e))?;

        let processed_outputs =
            output_tensors.map_err(|e| format!("backend inference error: {}", e))?;

        // Jitter update.
        self.jitter_tracker.update(inference_latency_ms);
        let rolling_jitter_ms = self.jitter_tracker.std_dev_ms();

        // Thermal probe.
        let thermal_state_opt = self.probe_platform_thermals();

        // Parse inference outputs.
        let proposed_cmd = self
            .parse_inference_to_command(&processed_outputs, loop_start_ms)
            .map_err(|e| format!("invalid inference outputs: {}", e))?;

        // Degraded-mode detection.
        let mut degraded = false;
        let t = &self.thresholds;

        if inference_latency_ms > t.max_inference_latency_ms {
            degraded = true;
        }
        if rolling_jitter_ms > t.max_jitter_ms {
            degraded = true;
        }
        if frame_age_ms > t.max_frame_age_ms {
            degraded = true;
        }
        if matches!(thermal_state_opt, Some(ThermalState::Critical)) {
            degraded = true;
        }

        // Clamp-only degraded mode.
        let sanitized_command = if degraded {
            let clamped_linear = proposed_cmd
                .linear_velocity
                .min(t.max_linear_velocity_mps);
            ControlCommand {
                linear_velocity: clamped_linear,
                angular_velocity: proposed_cmd.angular_velocity,
                timestamp_ms: loop_start_ms,
            }
        } else {
            proposed_cmd
        };

        self.last_validated_command = Some(sanitized_command.clone());

        let telemetry = RuntimeTelemetry {
            inference_latency_ms,
            rolling_jitter_ms,
            dropped_frames: self.dropped_frame_counter,
            thermal_state: thermal_state_opt.unwrap_or(ThermalState::Normal),
            frame_age_ms,
            tensor_payload_bytes,
            backend_precision: self
                .cached_capabilities
                .precision_modes
                .first()
                .copied()
                .unwrap_or(PrecisionMode::FP32),
            backend_vendor: std::borrow::Cow::Borrowed(self.cached_capabilities.vendor_name),
        };

        Ok(PostureSnapshot {
            frame_id: current_frame.frame_id,
            active_command: sanitized_command,
            telemetry,
            active_state_degraded: degraded,
        })
    }

    fn parse_inference_to_command(
        &self,
        outputs: &TensorBatch,
        ts: u64,
    ) -> Result<ControlCommand, String> {
        let linear = outputs
            .named_tensors
            .get("cmd_vel_linear")
            .and_then(|v| v.as_slice().first())
            .copied()
            .unwrap_or(0.0) as f64;

        let angular = outputs
            .named_tensors
            .get("cmd_vel_angular")
            .and_then(|v| v.as_slice().first())
            .copied()
            .unwrap_or(0.0) as f64;

        if !linear.is_finite() || !angular.is_finite() {
            return Err(format!(
                "non-finite command values: linear={}, angular={}",
                linear, angular
            ));
        }

        Ok(ControlCommand {
            linear_velocity: linear,
            angular_velocity: angular,
            timestamp_ms: ts,
        })
    }

    fn probe_platform_thermals(&self) -> Option<ThermalState> {
        let content = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok()?;
        let temp_raw = content.trim().parse::<i32>().ok()?;
        let temp_c = temp_raw / 1000;

        Some(if temp_c >= 80 {
            ThermalState::Critical
        } else if temp_c >= 65 {
            ThermalState::Warning
        } else {
            ThermalState::Normal
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::backend::{BackendCapabilities, BackendError, InferenceBackend, PrecisionMode, TensorStorage};
    use crate::sensor::SensorStream;

    struct TestBackend;

    impl InferenceBackend for TestBackend {
        fn load_model(&self, _: &str) -> Result<ModelHandle, BackendError> {
            let mut inputs = HashMap::new();
            inputs.insert("image_input".to_string(), vec![1, 3, 224, 224]);
            Ok(ModelHandle {
                model_id: "test".to_string(),
                input_shapes: inputs,
                output_shapes: HashMap::new(),
                expected_precision: PrecisionMode::FP32,
            })
        }

        fn run(
            &self,
            _: &ModelHandle,
            _: &TensorBatch,
        ) -> Result<TensorBatch<'static>, BackendError> {
            // Force degraded mode by exceeding latency threshold.
            std::thread::sleep(std::time::Duration::from_millis(200));

            let mut map = HashMap::new();
            map.insert(
                "cmd_vel_linear".to_string(),
                TensorStorage::Owned(vec![65.0]),
            );
            map.insert(
                "cmd_vel_angular".to_string(),
                TensorStorage::Owned(vec![0.0]),
            );
            Ok(TensorBatch {
                named_tensors: map,
                metadata: HashMap::new(),
            })
        }

        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities {
                precision_modes: vec![PrecisionMode::FP32],
                supports_zero_copy_inputs: true,
                max_batch_size: 1,
                vendor_name: "TestBackend",
            }
        }
    }

    struct SimpleStream {
        next_id: u64,
    }

    impl SensorStream for SimpleStream {
        fn next_frame(&mut self) -> Option<SensorFrame> {
            self.next_id += 1;
            Some(SensorFrame::new(
                self.next_id,
                TensorBatch {
                    named_tensors: HashMap::new(),
                    metadata: HashMap::new(),
                },
            ))
        }
    }

    #[tokio::test]
    async fn degraded_mode_clamps_overspeed_commands() {
        let backend = Arc::new(TestBackend);
        let model = backend.load_model("").unwrap();
        let (tx, mut rx) = mpsc::channel(4);

        let mut loop_engine = InferenceLoop::new(backend, model, tx);
        let mut stream = SimpleStream { next_id: 0 };

        // First tick: fills last_validated_command, sends nothing yet.
        let _ = loop_engine.tick(stream.next_frame().unwrap()).await.unwrap();

        // Second tick: sends previous command, computes a new clamped one.
        let snapshot = loop_engine.tick(stream.next_frame().unwrap()).await.unwrap();

        assert!(snapshot.active_state_degraded, "expected degraded mode");
        assert!(snapshot.active_command.linear_velocity <= 1.5);
        assert_eq!(snapshot.active_command.linear_velocity, 1.5);

        let flushed = rx.recv().await.unwrap();
        assert!(flushed.linear_velocity <= 1.5);
    }
}
