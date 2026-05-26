# Backlog

> Tasks are small, actionable, and independently testable.
> Pull into `active.md` when starting. Move to `done.md` on merge.
>
> Labels: `backend-qnn` `backend-tidl` `backend-openvino` `backend-rocm`
>         `behavioral-safety` `control-loop` `aegis-integration`
>         `docs` `packaging` `simulation`

---

## Increment 1 — Deterministic Runtime Core

- [ ] **PARK-001** `control-loop` — Implement `ControlLoop::with_governor(impl SafetyGovernor)` builder; suppress built-in clamp when governor is attached
- [ ] **PARK-002** `control-loop` — Add `set_state_for_test(state: PostureState)` behind `#[cfg(test)]` in `parko-core`; confirm not compiled into release binary
- [ ] **PARK-003** `control-loop` — Write `test_posture_divergence`: property test asserting governor output ≤ built-in clamp output for all valid inputs
- [ ] **PARK-004** `control-loop` — Wire `SafetyPosture` enum through `InferenceLoop::tick` so posture is visible at each step without heap allocation
- [ ] **PARK-005** `control-loop` — Add NaN/Inf guard at `ControlLoop` input boundary; any bad float → `EnforcementAction::Halt` before governor sees it
- [ ] **PARK-006** `control-loop` — Benchmark `ControlLoop::tick` under 1 kHz sustained load; assert P99 < 500 µs on reference x86_64 host
- [ ] **PARK-007** `docs` — Write `parko-core/README.md`: posture state machine diagram, governor contract, tick grid guarantees

---

## Increment 2 — Hardware Abstraction Layer

- [ ] **PARK-008** `backend-qnn` — Define `BackendDescriptor` enum in `parko-core`: `Cpu`, `QualcommQnn`, `TiTidl`, `AmdRocm`, `IntelOpenVino`; derive `Debug`, `Clone`, `PartialEq`
- [ ] **PARK-009** `backend-qnn` — Implement `QnnStubBackend`: returns deterministic fixed-point outputs; gated behind `features = ["backend-qnn"]`
- [ ] **PARK-010** `backend-qnn` — Implement `QnnBackend` wrapping Qualcomm AI Engine Direct SDK C bindings; zero-copy input/output via `&[f32]` slices
- [ ] **PARK-011** `backend-tidl` — Implement `TidlStubBackend` with configurable DSP latency simulation (default 2 ms) for CI without hardware
- [ ] **PARK-012** `backend-tidl` — Implement `TidlBackend` using TI TIDL runtime C FFI; feature-gated; cross-compile target: `aarch64-unknown-linux-gnu`
- [ ] **PARK-013** `backend-openvino` — Implement `OpenVinoStubBackend`: wraps `openvino` crate behind `features = ["backend-openvino"]`; CI always passes
- [ ] **PARK-014** `backend-openvino` — Implement `OpenVinoBackend` with real model loading; validate output shape before returning to `InferenceLoop`
- [ ] **PARK-015** `backend-rocm` — Implement `RocmStubBackend` returning zeroed outputs; gated behind `features = ["backend-rocm"]`
- [ ] **PARK-016** `backend-rocm` — Implement `RocmBackend` using MIGraphX Rust bindings or ROCm HIP C FFI; cross-compile target: `x86_64-unknown-linux-gnu`
- [ ] **PARK-017** `control-loop` — Add backend latency watchdog: if `InferenceBackend::run` exceeds `deadline_ms`, set posture to `Degraded` and emit watchdog event
- [ ] **PARK-018** `docs` — Document `InferenceBackend` zero-copy contract in `parko-core/docs/backend_contract.md`; include lifetime rules for input/output slices

---

## Increment 3 — Behavioral Safety (RSS-Equivalent)

- [ ] **PARK-019** `behavioral-safety` — Implement `RssSafeDistance::longitudinal(ego_vel, lead_vel, reaction_time, accel_max, brake_max) -> f64` per IEEE 2846
- [ ] **PARK-020** `behavioral-safety` — Implement `RssSafeDistance::lateral(ego_lat_vel, obj_lat_vel, lat_accel_max, mu) -> f64`
- [ ] **PARK-021** `behavioral-safety` — Add `RssState { safe: bool, longitudinal_margin: f64, lateral_margin: f64 }` to posture evaluation pipeline
- [ ] **PARK-022** `aegis-integration` — Wire `RssSafeDistance` check into `KirraKernelGovernor::enforce`: RSS violation → clamp velocity to zero immediately
- [ ] **PARK-023** `behavioral-safety` — Property-test: for all `(ego_vel, lead_vel)` in valid range, no `cmd_vel` that violates RSS reaches actuator in any posture
- [ ] **PARK-024** `aegis-integration` — Add `RssViolationEvent` variant to `kirra-runtime-sdk` audit chain; include ego state, object state, computed margins
- [ ] **PARK-025** `aegis-integration` — Update posture engine: RSS violation → transition fleet posture to `Degraded`; recovery requires 5 clean ticks (matches AV hysteresis)
- [ ] **PARK-026** `simulation` — Run 10 000 adversarial trajectory scenarios via `ScenarioRunner`; assert zero RSS violations pass the governor
- [ ] **PARK-027** `docs` — Write `docs/rss_integration.md`: formal RSS model derivation, integration diagram, test coverage table

---

## Increment 4 — Silicon Matrix Expansion

- [ ] **PARK-028** `backend-qnn` — CI matrix: add `backend-qnn` feature to GitHub Actions; run stub backend tests on ubuntu-latest
- [ ] **PARK-029** `backend-tidl` — CI matrix: add `backend-tidl` feature; cross-compile to `aarch64` using `cross`; run stub tests
- [ ] **PARK-030** `backend-openvino` — CI matrix: add `backend-openvino` feature; install OpenVINO runtime in CI via `apt`
- [ ] **PARK-031** `backend-rocm` — CI matrix: add `backend-rocm` feature; stub tests only on CI (no GPU runner)
- [ ] **PARK-032** `control-loop` — Add `BackendSelector`: runtime selection of active backend by `BackendDescriptor`; fall back to CPU ORT if target unavailable
- [ ] **PARK-033** `simulation` — Validate determinism: run identical scenario on ORT + QnnStub + TidlStub; assert outputs are bit-identical within tolerance

---

## Increment 5 — Safety OS Packaging

- [ ] **PARK-034** `packaging` — Create unified `kirra_safety_runtime` binary merging posture engine + inference loop; reads config from env vars
- [ ] **PARK-035** `packaging` — Write `scripts/kirra-safety-runtime.service` systemd unit: `WatchdogSec=5`, `MemoryMax=512M`, `CPUQuota=80%`
- [ ] **PARK-036** `packaging` — Update `install.sh`: add `--backend` flag (`ort|qnn|tidl|openvino|rocm`); download correct feature-gated binary
- [ ] **PARK-037** `packaging` — Update `charts/kirra-verifier` Helm chart: add `inferenceBackend` value, mount model volume, expose metrics port
- [ ] **PARK-038** `packaging` — Dashboard: add panels for inference tick rate, backend P99 latency, RSS margin (longitudinal/lateral), posture history sparkline
- [ ] **PARK-039** `packaging` — Release pipeline: build all backend variants in CI matrix; upload per-target tarballs to GitHub Release

---

## Increment 6 — Certification-Ready Runtime

- [ ] **PARK-040** `docs` — Expand RTM (`KIRRA-RTM-001`): trace every safety requirement to source line, test ID, and coverage report entry
- [ ] **PARK-041** `docs` — Generate MC/DC coverage for `posture_cache.rs`, `posture_engine_v2.rs`, `kirra_core.rs`, `rss.rs` using `cargo-llvm-cov`
- [ ] **PARK-042** `docs` — Write FMEA (`KIRRA-FMEA-001`): posture engine stale cache, governor bypass, attestation replay, nonce exhaustion
- [ ] **PARK-043** `docs` — Write DFA (`KIRRA-DFA-001`): common-cause failures in HA active/passive pair sharing SQLite on NFS
- [ ] **PARK-044** `aegis-integration` — Implement offline `kirra_audit_verify` binary: reads audit chain from SQLite, verifies Ed25519 signatures without running service
- [ ] **PARK-045** `docs` — Write SOTIF analysis (`KIRRA-SOTIF-001`): intended function, triggering conditions, evaluation scenarios per ISO 21448
