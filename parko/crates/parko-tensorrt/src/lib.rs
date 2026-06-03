// crates/parko-tensorrt/src/lib.rs
//
// PARK-021 — TensorRT backend (A1: distinct crate using ort's TensorRT execution
// provider). Runs Parko accelerated on the ROSOrin's Jetson. CI-BUILDABLE ONLY:
// it compiles with no GPU/CUDA/TRT libraries present (ort's `load-dynamic` pulls
// `ort-sys/disable-linking`), but real inference requires a TensorRT-enabled ORT
// runtime on NVIDIA silicon — see the PARK-021 Jetson-gated list at the bottom.
//
// REUSE: the load_model/run inference path is IDENTICAL to parko-onnx and is
// single-sourced via `parko_onnx::session_core::OrtRunCore`. This crate differs
// ONLY in how the session is built (the TRT execution provider + precision
// config) and which posture is logged.
//
// FAIL-CLOSED (the load-bearing safety property): the TRT EP is registered with
// `.error_on_failure()`. ort's default is to SILENTLY fall back to CPU when an EP
// fails to register (confirmed in ort rc.11 `apply_execution_providers`); that is
// exactly the silent-degradation hazard a safety path must reject. With
// `error_on_failure`, a TRT-unavailable runtime (e.g. CI's CPU-only ORT lib)
// makes `with_config` return `Err` — never a quiet CPU run.
//
// DETERMINISM HONESTY: GPU TensorRT is NOT bitwise-reproducible the way
// single-thread CPU is, so this backend does NOT read `InferenceThreads`
// (num_threads is a CPU concept). Its config anchor is `TrtPosture`. The safety
// posture is "fixed engine + fixed precision + decision-agreement bound
// (hardware-measured)", logged — not a bitwise-determinism claim.

use ort::ep::TensorRT;
use ort::session::Session;

use parko_core::backend::{
    BackendCapabilities, BackendDescriptor, BackendError, InferenceBackend, ModelHandle,
    TensorBatch,
};
use parko_onnx::session_core::OrtRunCore;

/// TF32 control state. Ampere+ GPUs (the Orin) may use TF32 for fp32 matmuls,
/// silently dropping mantissa bits. ort's TensorRT EP exposes NO TF32 knob
/// (confirmed in ort rc.11 source — `with_fp16`/`with_int8` exist, no TF32), so
/// this backend CANNOT enforce TF32-off from Rust. This is an honest,
/// not-yet-resolved precision gap, surfaced rather than hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tf32Control {
    /// TF32 is NOT enforced off here. Resolution is Jetson-gated (e.g. the
    /// `NVIDIA_TF32_OVERRIDE=0` env override, measured against tolerance) or, if
    /// it cannot be controlled, the A2 (native nvinfer) escalation trigger.
    /// MUST NOT be read as "TF32 off / full precision guaranteed".
    UnenforcedPendingJetsonResolution,
}

impl Tf32Control {
    /// Honest one-line status for the init log. Deliberately does NOT say "off".
    #[must_use]
    pub fn status_str(self) -> &'static str {
        match self {
            Tf32Control::UnenforcedPendingJetsonResolution => "UNENFORCED (pending Jetson resolution; no TF32 knob in ort TRT EP)",
        }
    }
}

/// The TensorRT backend's execution posture — its config anchor and the audit
/// record logged at init. Replaces the CPU/OpenVINO `bitwise_reproducible`
/// claim, which does not hold on GPU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrtPosture {
    /// FP16 inference. FIXED false — full precision for the safety path.
    pub fp16: bool,
    /// INT8 inference. FIXED false — no silent quantization.
    pub int8: bool,
    /// TF32 control — see [`Tf32Control`]. NOT enforceable from ort's TRT EP.
    pub tf32: Tf32Control,
    /// Where the serialized TRT engine is cached (per model/shape/version/GPU).
    pub engine_cache_path: String,
    /// SHA of the built engine. `None` until an engine is actually built on
    /// hardware (Jetson-gated); it cannot be known on a GPU-less CI build.
    pub engine_sha: Option<String>,
}

impl TrtPosture {
    /// The fixed safety defaults for a given engine-cache path: full precision
    /// (no fp16/int8), TF32 unenforced-pending, no engine SHA yet.
    #[must_use]
    pub fn full_precision(engine_cache_path: impl Into<String>) -> Self {
        Self {
            fp16: false,
            int8: false,
            tf32: Tf32Control::UnenforcedPendingJetsonResolution,
            engine_cache_path: engine_cache_path.into(),
            engine_sha: None,
        }
    }

    /// True only if precision is *fully* guaranteed end to end. It is NOT, while
    /// TF32 is unenforceable from the EP — so this is honestly `false`. The init
    /// log surfaces this rather than implying full-precision determinism.
    #[must_use]
    pub fn full_precision_guaranteed(&self) -> bool {
        // Even with fp16/int8 off, full precision is NOT guaranteed while TF32 is
        // unenforceable from ort's TRT EP. Honestly `false` until TF32 is
        // resolved on the Jetson (env override measured vs tolerance, or A2).
        if self.fp16 || self.int8 {
            return false;
        }
        match self.tf32 {
            Tf32Control::UnenforcedPendingJetsonResolution => false,
        }
    }
}

/// Configuration for [`TrtBackend::with_config`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrtConfig {
    /// Engine-cache directory. See [`resolve_engine_cache_path`].
    pub engine_cache_path: String,
}

impl Default for TrtConfig {
    fn default() -> Self {
        Self { engine_cache_path: resolve_engine_cache_path(None) }
    }
}

/// Default engine-cache directory when none is configured. The fixed input
/// shapes Parko's sensor mappings emit mean one engine per model and clean cache
/// reuse, so a stable on-disk path is the intended setup.
pub const DEFAULT_ENGINE_CACHE_PATH: &str = "./parko_trt_engine_cache";

/// Resolve the engine-cache path: explicit wins, else `PARKO_TRT_ENGINE_CACHE`
/// env, else the default. Pure + GPU-free (unit-tested on CI).
#[must_use]
pub fn resolve_engine_cache_path(explicit: Option<&str>) -> String {
    if let Some(p) = explicit {
        return p.to_string();
    }
    std::env::var("PARKO_TRT_ENGINE_CACHE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_ENGINE_CACHE_PATH.to_string())
}

/// TensorRT inference backend. Construct with [`TrtBackend::with_config`] (or
/// [`TrtBackend::new`] for defaults). Real inference is Jetson-gated.
pub struct TrtBackend {
    core: OrtRunCore,
    posture: TrtPosture,
}

impl TrtBackend {
    /// Construct with the default config (resolved engine-cache path, full
    /// precision). Jetson-gated at runtime: needs a TensorRT-enabled ORT lib.
    pub fn new(model_path: &str) -> Result<Self, BackendError> {
        Self::with_config(model_path, &TrtConfig::default())
    }

    /// Build a session that runs on the TensorRT EP ONLY (no CUDA/CPU EP entries
    /// — fail-closed is the posture), with full precision (fp16/int8 off) and
    /// engine caching enabled. Returns `Err` if the TRT EP cannot register
    /// against the dlopened ORT runtime (`error_on_failure`) — never a silent
    /// CPU run.
    pub fn with_config(model_path: &str, cfg: &TrtConfig) -> Result<Self, BackendError> {
        let posture = TrtPosture::full_precision(cfg.engine_cache_path.clone());

        // TRT EP only. fp16=false, int8=false (full precision); engine cache on.
        // `.error_on_failure()` makes a failed registration fatal (fail-closed).
        let trt_ep = TensorRT::default()
            .with_fp16(posture.fp16)
            .with_int8(posture.int8)
            .with_engine_cache(true)
            .with_engine_cache_path(&posture.engine_cache_path)
            .build()
            .error_on_failure();

        let session: Session = Session::builder()
            .map_err(|e| BackendError::InitializationError(format!("ort builder error: {e:?}")))?
            .with_execution_providers([trt_ep])
            .map_err(|e| BackendError::InitializationError(format!(
                "TensorRT EP registration failed — refusing to run (fail-closed; no CPU fallback). \
                 The dlopened ONNX Runtime lacks a usable TensorRT provider \
                 (expected on a CPU-only ORT build / CI): {e:?}"
            )))?
            .commit_from_file(model_path)
            .map_err(|e| BackendError::InitializationError(format!("ort session init error: {e:?}")))?;

        // Audit-relevant posture log. HONEST: full_precision_guaranteed is false
        // while TF32 is unenforceable, and GPU TRT is not bitwise-reproducible.
        tracing::info!(
            backend = "tensorrt",
            fp16 = posture.fp16,
            int8 = posture.int8,
            tf32 = %posture.tf32.status_str(),
            engine_cache_path = %posture.engine_cache_path,
            engine_sha = ?posture.engine_sha,
            full_precision_guaranteed = posture.full_precision_guaranteed(),
            "TrtBackend execution posture (TensorRT EP; not bitwise-reproducible — \
             fixed-engine + fixed-precision + hardware-measured decision-agreement posture)"
        );

        Ok(Self {
            core: OrtRunCore::new(session, "ort_trt"),
            posture,
        })
    }

    /// The logged execution posture (audit / introspection).
    #[must_use]
    pub fn posture(&self) -> &TrtPosture {
        &self.posture
    }
}

impl InferenceBackend for TrtBackend {
    fn load_model(&self, path: &str) -> Result<ModelHandle, BackendError> {
        self.core.load_model(path)
    }

    fn run(&self, model: &ModelHandle, inputs: &TensorBatch)
        -> Result<TensorBatch<'static>, BackendError>
    {
        self.core.run(model, inputs)
    }

    fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor::TensorRT
    }

    fn capabilities(&self) -> BackendCapabilities {
        // Jetson-gated: the real fp16/int8/batch capabilities are measured on
        // hardware (PARK-021). The CI-buildable skeleton reports the conservative
        // full-precision posture it actually configures.
        BackendCapabilities {
            supports_int8: false,
            supports_fp16: false,
            max_batch_size: None,
        }
    }
}

/// PARK-021 JETSON-GATED FOLLOW-UPS — cannot be implemented/validated on CI (no
/// GPU). Each is a tracked next step with its resolution path; this crate is the
/// CI-buildable skeleton only.
///
/// 1. **Real `load_model`/`run` output** — needs a TensorRT-enabled ORT runtime
///    on NVIDIA silicon. The inference path itself is already shared
///    (`OrtRunCore`); only on-hardware validation remains.
/// 2. **Engine build / cache + warm-up** — TRT builds a per-model/shape engine
///    (slow first run), cached at `engine_cache_path`. Parko's sensor mappings
///    emit FIXED shapes → one engine per model, clean cache reuse. Needs a
///    startup warm-up so the multi-second build never lands on the first real
///    command. Populate `TrtPosture.engine_sha` once an engine exists.
/// 3. **Precision validation** — confirm fp32 is actually used, and resolve TF32:
///    test `NVIDIA_TF32_OVERRIDE=0` and measure its impact vs the decision
///    tolerance. If TF32 can't be controlled out-of-band, this is the **A2
///    (native nvinfer FFI) escalation trigger** (the ort TRT EP has no TF32 knob).
/// 4. **Cross-backend equivalence** — extend the #152 ORT-vs-OV harness to
///    TRT-vs-ORT-CPU. TRT-vs-CPU drift exceeds CPU-vs-CPU drift, so the bound is a
///    SEPARATE, hardware-measured **decision-agreement** tolerance on the governed
///    command (not bitwise logits). Anchor the harness on `TrtPosture`, NOT
///    `InferenceThreads` (a CPU concept).
/// 5. **Perf / latency** — engine-build time, warm vs cold, throughput.
/// 6. **Runtime confirmation** — verify the JetPack ORT build on the ROSOrin
///    image actually carries the TensorRT EP (so `with_config` succeeds there,
///    rather than fail-closing as it correctly does on CI's CPU-only build).
pub mod park021_jetson_gated {}

#[cfg(test)]
mod tests {
    use super::*;

    // GPU-FREE — these RUN on CI (no ORT runtime, no Session construction).

    #[test]
    fn trt_posture_defaults_are_full_precision() {
        let p = TrtPosture::full_precision("/tmp/cache");
        assert!(!p.fp16, "fp16 must default off (full precision)");
        assert!(!p.int8, "int8 must default off (no silent quantization)");
        assert_eq!(p.tf32, Tf32Control::UnenforcedPendingJetsonResolution);
        assert_eq!(p.engine_sha, None, "no engine SHA until built on hardware");
    }

    #[test]
    fn full_precision_is_not_guaranteed_while_tf32_unenforced() {
        // Honesty guard: even with fp16/int8 off, precision is NOT fully
        // guaranteed because TF32 is unenforceable from the EP. The flag — and
        // therefore the init log — must say false.
        let p = TrtPosture::full_precision("/tmp/cache");
        assert!(!p.full_precision_guaranteed(),
            "must not claim full precision while TF32 is unenforced");
    }

    #[test]
    fn tf32_status_is_honest_not_off() {
        let s = Tf32Control::UnenforcedPendingJetsonResolution.status_str();
        assert!(s.contains("UNENFORCED"), "status must surface that TF32 is unenforced");
        assert!(!s.to_lowercase().contains("off"),
            "status must NOT read as 'TF32 off'");
    }

    #[test]
    fn engine_cache_path_explicit_wins() {
        assert_eq!(resolve_engine_cache_path(Some("/var/parko/trt")), "/var/parko/trt");
    }

    #[test]
    fn engine_cache_path_falls_back_to_default() {
        // With no explicit path and (assuming) no env, the default is used.
        // Not asserting under a set env to keep this hermetic.
        if std::env::var("PARKO_TRT_ENGINE_CACHE").is_err() {
            assert_eq!(resolve_engine_cache_path(None), DEFAULT_ENGINE_CACHE_PATH);
        }
    }

    #[test]
    fn config_default_uses_resolved_path() {
        let c = TrtConfig::default();
        assert!(!c.engine_cache_path.is_empty());
    }
}
