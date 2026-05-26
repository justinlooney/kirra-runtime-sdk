# Roadmap

> Lean-agile increments. Each increment ships a testable, standalone artifact.
> No increment depends on the next being complete before it delivers value.

---

## Increment 1 — Deterministic Runtime Core

**Goal:** A clock-driven, posture-aware inference loop that compiles, passes all tests,
and can be dropped into any Rust project as a library.

| # | Task | Artifact |
|---|------|----------|
| 1.1 | Attach `SafetyGovernor` to `ControlLoop` via `with_governor` builder | `ControlLoop::with_governor` API |
| 1.2 | Add `set_state_for_test` behind `#[cfg(test)]` in `parko-core` | Stable test seam |
| 1.3 | Write posture-divergence property test (governor clamps vs. built-in clamp) | `test_posture_divergence` passes |
| 1.4 | Wire `SafetyPosture` through `SafetyGovernor` trait into `InferenceLoop` | Posture visible in loop tick |
| 1.5 | Harden `ControlLoop` tick: reject `NaN`/`Inf` inputs before governor | Invariant: loop never feeds bad values to governor |
| 1.6 | Publish `parko-core` v0.1.0 to local registry (or tag) | `cargo add parko-core` works |

---

## Increment 2 — Hardware Abstraction Layer

**Goal:** A zero-copy `InferenceBackend` trait with one real backend (ONNX Runtime)
and stub backends for Qualcomm QNN, TI TIDL, and Intel OpenVINO.

| # | Task | Artifact |
|---|------|----------|
| 2.1 | Harden `parko-onnx` ORT backend: session reuse, error mapping, output slice lifetime | `parko-onnx` 0.1.0 |
| 2.2 | Define `BackendDescriptor` (hardware target enum: CPU, QNN, TIDL, ROCm, OpenVINO) | Shared enum in `parko-core` |
| 2.3 | Implement `QnnStubBackend` returning deterministic dummy outputs | `backend-qnn` stub passes CI |
| 2.4 | Implement `TidlStubBackend` with DSP latency simulation | `backend-tidl` stub passes CI |
| 2.5 | Implement `OpenVinoStubBackend` wrapping openvino crate (no-op feature gate) | `backend-openvino` stub passes CI |
| 2.6 | Benchmark ORT backend: latency P50/P95/P99 under 1 kHz tick rate | Benchmark report in `docs/benchmarks/` |

---

## Increment 3 — Behavioral Safety (RSS-Equivalent)

**Goal:** A pure-Rust RSS-class behavioral safety layer integrated with `parko-core`
and `kirra-runtime-sdk`. Replaces ad-hoc kinematics checks with a formal RSS distance model.

| # | Task | Artifact |
|---|------|----------|
| 3.1 | Implement `RssSafeDistance` model (longitudinal + lateral, ISO 26262 compliant) | `parko-core::rss` module |
| 3.2 | Wire `RssSafeDistance` into `KirraKernelGovernor` as a pre-actuator gate | RSS-gated `cmd_vel` path |
| 3.3 | Property-test: no RSS violation can reach actuator in any posture state | Proptest suite in `parko-core` |
| 3.4 | Add `RssViolationEvent` to `kirra-runtime-sdk` audit chain | Tamper-evident RSS log |
| 3.5 | Integrate RSS state into fleet posture: RSS violation → `Degraded` posture | Posture engine update |
| 3.6 | Simulate 10 000 adversarial trajectories; assert zero unsafe outputs | Simulation report |

---

## Increment 4 — Silicon Matrix Expansion

**Goal:** Real (non-stub) inference on at least two hardware targets: Qualcomm QNN and
Intel OpenVINO. TI TIDL and AMD ROCm/Vitis delivered as gated feature flags.

| # | Task | Artifact |
|---|------|----------|
| 4.1 | Implement `QnnBackend` using Qualcomm AI Engine Direct SDK (feature: `backend-qnn`) | Real QNN inference |
| 4.2 | Implement `OpenVinoBackend` using openvino-rs (feature: `backend-openvino`) | Real OpenVINO inference |
| 4.3 | Implement `TidlBackend` using TI TIDL runtime C bindings (feature: `backend-tidl`) | Real TIDL inference |
| 4.4 | Implement `RocmBackend` using MIGraphX or ROCm runtime (feature: `backend-rocm`) | Real ROCm inference |
| 4.5 | Cross-compile and test each backend on target hardware in CI matrix | CI: all 4 backends green |
| 4.6 | Add latency watchdog: backend exceeds deadline → `InferenceLoop` degrades posture | Deadline enforcement |

---

## Increment 5 — Safety OS Packaging

**Goal:** A single installable artifact that includes `kirra-runtime-sdk` + `parko-core`
+ chosen backends as a systemd-managed safety runtime with dashboard, installer, and
Helm chart.

| # | Task | Artifact |
|---|------|----------|
| 5.1 | Merge `kirra-runtime-sdk` posture engine with `parko-core` inference loop into unified binary | `kirra_safety_runtime` binary |
| 5.2 | Add `kirra_safety_runtime` systemd unit with WatchdogSec and memory limits | `scripts/kirra-safety-runtime.service` |
| 5.3 | Update installer (`install.sh`) to deploy parko backends via feature flag selection | Interactive + non-interactive modes |
| 5.4 | Update Helm chart to deploy full safety OS on Kubernetes edge node | `charts/kirra-verifier` updated |
| 5.5 | Dashboard: add InferenceLoop tick rate, backend latency P99, RSS status panels | Live metrics in UI |
| 5.6 | Release `kirra-v1.2.0` with x86_64, aarch64, armv7 tarballs | GitHub Release with SHA256 |

---

## Increment 6 — Certification-Ready Runtime

**Goal:** Sufficient documentation, traceability, and process evidence to begin a
formal ASIL-D / SIL 3 pre-assessment with a TÜV or SGS-TÜV Saar auditor.

| # | Task | Artifact |
|---|------|----------|
| 6.1 | Complete Requirements Traceability Matrix (RTM): source → code → test for all safety functions | `KIRRA-RTM-001` v1.0 |
| 6.2 | Generate MC/DC coverage report for safety-critical modules | Coverage report ≥ 100% MC/DC |
| 6.3 | Complete FMEA: failure modes for posture engine, governor, attestation chain | `KIRRA-FMEA-001` v1.0 |
| 6.4 | Write DFA (Dependent Failure Analysis) for HA active/passive pair | `KIRRA-DFA-001` v1.0 |
| 6.5 | Harden audit chain: Ed25519 key rotation procedure + offline verification tool | `kirra_audit_verify` binary |
| 6.6 | Submit pre-assessment package to auditor; track findings in `decisions.md` | Audit trail entry |
