// crates/parko-onnx/src/lib.rs

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;

use parko_core::backend::{
    BackendCapabilities, BackendError, InferenceBackend, InferenceThreads, ModelHandle,
    TensorBatch,
};

pub mod session_core;
use session_core::OrtRunCore;

pub struct OrtBackend {
    core: OrtRunCore,
}

impl OrtBackend {
    /// Construct with the default execution posture (single-threaded,
    /// bitwise-reproducible). The thread count is the only configurable knob;
    /// see [`OrtBackend::with_threads`].
    pub fn new(model_path: &str) -> Result<Self, BackendError> {
        Self::with_threads(model_path, InferenceThreads::default())
    }

    /// Construct with an explicit [`InferenceThreads`]. `num_threads` is the
    /// sole configurable setting; the optimization level (`Disable`, the
    /// determinism posture mirrored by parko-openvino's ACCURACY mode) is
    /// fixed. The thread count MUST come from the same `InferenceThreads` the
    /// OpenVINO backend reads — see `parko_core::InferenceThreads`.
    pub fn with_threads(
        model_path: &str,
        threads: InferenceThreads,
    ) -> Result<Self, BackendError> {
        let session = Session::builder()
            .map_err(|e| BackendError::InitializationError(format!("ort builder error: {:?}", e)))?
            .with_intra_threads(threads.num_threads)
            .map_err(|e| BackendError::InitializationError(format!("ort intra_threads error: {:?}", e)))?
            .with_optimization_level(GraphOptimizationLevel::Disable)
            .map_err(|e| BackendError::InitializationError(format!("ort opt_level error: {:?}", e)))?
            .commit_from_file(model_path)
            .map_err(|e| BackendError::InitializationError(format!("ort session init error: {:?}", e)))?;

        // Record the execution posture (determinism status is audit-relevant).
        tracing::info!(
            backend = "ort",
            num_threads = threads.num_threads,
            optimization = "disabled",
            bitwise_reproducible = threads.bitwise_reproducible(),
            "OrtBackend execution posture"
        );

        // The CPU backend keeps its model_id identity ("ort_native_cpu"); the
        // shared core single-sources the load_model/run logic.
        Ok(Self {
            core: OrtRunCore::new(session, "ort_native_cpu"),
        })
    }
}

impl InferenceBackend for OrtBackend {
    fn load_model(&self, path: &str) -> Result<ModelHandle, BackendError> {
        self.core.load_model(path)
    }

    fn run(&self, model: &ModelHandle, inputs: &TensorBatch)
        -> Result<TensorBatch<'static>, BackendError>
    {
        self.core.run(model, inputs)
    }

    fn capabilities(&self) -> BackendCapabilities {
        // CPU ONNX Runtime baseline — update when quantized models are tested (PARK-009, ADL-007)
        BackendCapabilities {
            supports_int8: false,
            supports_fp16: false,
            max_batch_size: None,
        }
    }
}
