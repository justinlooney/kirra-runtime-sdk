// crates/parko-openvino/tests/test_openvino_backend.rs
//
// Integration tests for the OpenVINO backend. The same MNIST-12 ONNX
// fixture parko-onnx uses; OpenVINO ingests ONNX directly so no
// separate IR conversion is needed.
//
// What these tests verify:
//   - Smoke: OvBackend constructs, introspects shapes, and runs one
//     inference against the MNIST input without panicking, with all
//     output scores finite.
//   - Cross-backend numerical equivalence: the same input through
//     OrtBackend and OvBackend produces outputs within `EQUIV_TOL`.
//   - Fail-closed: a malformed / missing model path returns
//     `BackendError::InitializationError`, not a panic.
//   - Descriptor + capabilities: the trait surface returns
//     `BackendDescriptor::IntelOpenVino` and the documented CPU
//     baseline `BackendCapabilities`.
//
// All four tests require the OpenVINO C++ runtime to be discoverable
// at process start — libopenvino_c.so. On a clean dev box install via
// the Intel apt repo (or the archive) and set `OPENVINO_LIB_PATH` if
// the lib is not on the default search path. See parko/README.md
// §"Building and testing parko-openvino" (to be added; this PR also
// updates the README table).
//
// CI: the `parko-openvino` job in .github/workflows/ci.yml installs
// the runtime + runs `cargo test -p parko-openvino`. The
// `parko-safety` job is unaffected — it `--exclude`s this crate.

use std::collections::HashMap;

use parko_core::backend::{
    BackendCapabilities, BackendDescriptor, BackendError,
    InferenceBackend, TensorBatch, TensorStorage,
};
use parko_onnx::OrtBackend;
use parko_openvino::OvBackend;

const MNIST_PATH: &str = "tests/data/mnist-12.onnx";
const MNIST_INPUT_NAME:  &str = "Input3";
const MNIST_OUTPUT_NAME: &str = "Plus214_Output_0";

/// Absolute-value tolerance for the cross-backend equivalence check.
/// 1e-3 picks up genuine numerical drift between the two runtimes
/// while absorbing the unavoidable last-bit differences from
/// kernel-selection / fusion choices. If equivalence ever fails for
/// the MNIST fixture below this bound, the divergence is real —
/// don't loosen the bound without investigating.
const EQUIV_TOL: f32 = 1e-3;

fn make_mnist_input() -> [f32; 28 * 28] {
    // Same all-zeros input parko-onnx uses; the MNIST model is
    // deterministic so the equivalence check is a pure-numerics
    // comparison of how the two runtimes resolve the graph.
    [0.0_f32; 28 * 28]
}

fn batch_with<'a>(name: &str, data: &'a [f32]) -> TensorBatch<'a> {
    let mut named = HashMap::new();
    named.insert(name.to_string(), TensorStorage::Borrowed(data));
    TensorBatch { named_tensors: named, metadata: HashMap::new() }
}

#[test]
fn openvino_smoke_mnist_inference_runs_and_outputs_finite() {
    let backend = OvBackend::new(MNIST_PATH).unwrap_or_else(|e|
        panic!("OvBackend::new failed: {e:?}. Is libopenvino_c.so installed? \
                Set OPENVINO_LIB_PATH or apt-install openvino-2024."));

    let model = backend.load_model(MNIST_PATH).expect("load_model");
    let input_shape  = model.input_shapes.get(MNIST_INPUT_NAME)
        .expect("MNIST input node 'Input3' not found in introspection");
    let output_shape = model.output_shapes.get(MNIST_OUTPUT_NAME)
        .expect("MNIST output node 'Plus214_Output_0' not found");
    assert_eq!(input_shape,  &vec![1, 1, 28, 28], "input shape");
    assert_eq!(output_shape, &vec![1, 10],         "output shape");

    let input = make_mnist_input();
    let batch = batch_with(MNIST_INPUT_NAME, &input);
    let out = backend.run(&model, &batch).expect("run");
    let scores = out.named_tensors.get(MNIST_OUTPUT_NAME)
        .expect("missing output tensor").as_slice();
    assert_eq!(scores.len(), 10, "10-class MNIST output");
    for (i, s) in scores.iter().enumerate() {
        assert!(s.is_finite(), "non-finite score at index {i}: {s}");
    }
}

#[test]
fn ort_ov_output_equivalence_on_mnist() {
    // The first cross-backend validation check. Loads the SAME ONNX
    // model in both runtimes, runs the SAME input, compares element-
    // wise within `EQUIV_TOL`. Seeds the model-validation tooling
    // (a parko follow-up: a generic harness that swaps any two
    // InferenceBackend impls and runs this comparison).
    let ort = OrtBackend::new(MNIST_PATH).unwrap_or_else(|e|
        panic!("OrtBackend::new failed: {e:?}. Is libonnxruntime.so installed? \
                Set ORT_DYLIB_PATH or run via the parko-onnx README."));
    let ov  = OvBackend::new(MNIST_PATH).unwrap_or_else(|e|
        panic!("OvBackend::new failed: {e:?}. Is libopenvino_c.so installed?"));

    let ort_model = ort.load_model(MNIST_PATH).expect("ort load_model");
    let ov_model  = ov.load_model(MNIST_PATH).expect("ov load_model");

    let input = make_mnist_input();
    let ort_batch = batch_with(MNIST_INPUT_NAME, &input);
    let ov_batch  = batch_with(MNIST_INPUT_NAME, &input);

    let ort_out = ort.run(&ort_model, &ort_batch).expect("ort run");
    let ov_out  = ov.run(&ov_model,  &ov_batch).expect("ov run");

    let ort_scores = ort_out.named_tensors.get(MNIST_OUTPUT_NAME)
        .expect("ort output tensor").as_slice();
    let ov_scores  = ov_out.named_tensors.get(MNIST_OUTPUT_NAME)
        .expect("ov output tensor").as_slice();
    assert_eq!(ort_scores.len(), ov_scores.len(),
        "output lengths must match across backends");

    for (i, (a, b)) in ort_scores.iter().zip(ov_scores.iter()).enumerate() {
        let diff = (a - b).abs();
        assert!(diff <= EQUIV_TOL,
            "OrtBackend vs OvBackend disagree on MNIST output[{i}]: \
             ort={a} ov={b} |diff|={diff} > tol {EQUIV_TOL}. \
             A failure here is genuine numerical drift between the two \
             runtimes — don't loosen the bound without investigating.");
    }
}

#[test]
fn openvino_missing_model_returns_initialization_error_not_panic() {
    // Fail-closed: pointing the backend at a non-existent file must
    // return a structured error, never panic. Mirrors parko-onnx's
    // failure-mode contract.
    let result = OvBackend::new("tests/data/nonexistent-model.onnx");
    let err = match result {
        Ok(_) => panic!("constructing OvBackend against a missing file must error, not succeed"),
        Err(e) => e,
    };
    match err {
        BackendError::InitializationError(_) => {}
        other => panic!("expected InitializationError, got {other:?}"),
    }
}

#[test]
fn openvino_descriptor_is_intel_openvino() {
    let backend = OvBackend::new(MNIST_PATH).expect("OvBackend::new");
    assert_eq!(backend.descriptor(), BackendDescriptor::IntelOpenVino);
}

#[test]
fn openvino_capabilities_match_cpu_baseline() {
    let backend = OvBackend::new(MNIST_PATH).expect("OvBackend::new");
    let caps = backend.capabilities();
    assert_eq!(
        caps,
        BackendCapabilities {
            supports_int8:  false,
            supports_fp16:  false,
            max_batch_size: None,
        },
        "OvBackend capabilities must match the documented CPU baseline (parity with parko-onnx)"
    );
}
