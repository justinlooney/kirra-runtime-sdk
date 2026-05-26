# Roadmap

> Lean-agile increments. Each is independently testable and ships a concrete
> artifact. See project-board/columns.yaml for milestone → epic mappings.

---

## Increment 1 — Deterministic Runtime Core
**Milestone:** v0.1 | **Epic:** `epic:runtime-core`
**Artifact:** `parko-core` v0.1.0 — a clock-driven, posture-aware inference
loop consumable as a library by any downstream crate.

| Task | What | Done When |
|------|------|-----------|
| PARK-001 | Implement `ControlLoop::with_governor` builder. Stores governor as `Option<Box<dyn SafetyGovernor>>`. Suppresses built-in scalar clamp when governor is present. | `test_builtin_clamp_suppressed` passes; all existing tests green. |
| PARK-002 | Add `set_state_for_test(state: PostureState)` behind `#[cfg(test)]`. Unblocks posture-divergence tests without exposing a production mutation path. | Method absent from `cargo build --release` (verified with `nm`); present in `cargo test`. |
| PARK-003 | Write proptest suite asserting governor output ≤ built-in clamp for all valid `(proposed_output, posture_state)` pairs. Core correctness invariant. | ≥ 10 000 cases pass for Nominal, Degraded, and LockedOut. |
| PARK-004 | Add NaN/Inf input guard at `ControlLoop::tick` entry. Any bad float returns `EnforcementAction::Halt` before reaching the governor. | Property test generating adversarial floats confirms zero reach the governor. |
| PARK-005 | Wire `VirtualClock` / `SystemClock` abstraction into `ControlLoop`. Enables deterministic temporal tests without `sleep`. | Test advances `VirtualClock` manually; all timing logic exercisable without wall-clock. |
| PARK-006 | Tag `parko-core-v0.1.0`. Update `Cargo.toml`, verify `cargo publish --dry-run` exits cleanly. | Tag in repo; dry-run green. |

---

## Increment 2 — Hardware Abstraction Layer
**Milestone:** v0.2 | **Epic:** `epic:hal`
**Artifact:** `parko-core` v0.2.0 with zero-copy `InferenceBackend` trait and
four stub backends passing CI without hardware.

| Task | What | Done When |
|------|------|-----------|
| PARK-007 | Define `BackendDescriptor { Cpu, QualcommQnn, TiTidl, AmdRocm, IntelOpenVino }` in `parko-core`. Derive `Debug, Clone, PartialEq, Eq, Hash`. Re-export from crate root. | All downstream crates compile against new type; round-trip Debug test passes. |
| PARK-008 | `QnnStubBackend`: returns deterministic zeroed outputs, gated behind `features = ["backend-qnn"]`. | `cargo test --features backend-qnn` passes on ubuntu-latest. |
| PARK-009 | `TidlStubBackend`: configurable simulated DSP latency (default 2 ms), gated behind `features = ["backend-tidl"]`. | Latency simulation observable in benchmark; CI passes. |
| PARK-010 | `OpenVinoStubBackend`: zero-output stub gated behind `features = ["backend-openvino"]`. | `cargo test --features backend-openvino` passes without Intel platform. |
| PARK-011 | `RocmStubBackend`: zero-output stub gated behind `features = ["backend-rocm"]`. | `cargo test --features backend-rocm` passes without AMD GPU. |
| PARK-012 | Backend latency watchdog in `InferenceLoop`: if `backend.run()` exceeds `deadline_ms`, emit `LatencyViolation`, hold last safe output, transition posture to `Degraded` after N=3 consecutive violations. | Test with `TidlStubBackend` + short deadline triggers watchdog and confirms posture degrades. |
| PARK-013 | GitHub Actions matrix: build and test all four stub backends in one workflow run on ubuntu-latest. | All four feature flags green in the same CI run. |
| PARK-014 | `OpenVinoBackend` (real): model loading, input shape validation, output slice writing via `openvino-rs`. | Integration test with trivial identity model confirms output matches input. |

---

## Increment 3 — Behavioral Safety (RSS-Equivalent)
**Milestone:** v0.3 | **Epic:** `epic:behavioral-safety`
**Artifact:** RSS-gated `kirra_verifier_service` with tamper-evident violation
log, passing a 10 000-scenario adversarial simulation.

| Task | What | Done When |
|------|------|-----------|
| PARK-015 | `RssSafeDistance::longitudinal` per IEEE 2846-2022 §5.1. Inputs: ego_vel, lead_vel, reaction_time, accel_max, brake_max, brake_min. | Unit tests cover equal speeds, ego faster, ego slower, zero speed; matches IEEE reference values. |
| PARK-016 | `RssSafeDistance::lateral` per IEEE 2846-2022 §5.2. Inputs: lateral velocities, max lateral accel, reaction time. | Unit tests cover converging, diverging, stationary cases. |
| PARK-017 | `RssState { safe, longitudinal_margin, lateral_margin }` wired into `kirra-runtime-sdk` posture engine. RSS violation → `Degraded`; recovery uses existing 5-tick hysteresis. | Integration test: inject violation → Degraded; inject 5 clean ticks → Nominal. |
| PARK-018 | RSS pre-actuator gate in `KirraKernelGovernor`: `rss_state.safe == false` clamps velocity to 0.0 before any kinematics envelope check. | Unit test: safe=false + positive velocity → output 0.0; safe=true → normal kinematics. |
| PARK-019 | RSS property test: proptest over `(ego_vel, lead_vel, gap, commanded_vel)`. Asserts no RSS-violating command exits the governor for any valid input. | ≥ 10 000 cases pass; all posture states covered. |
| PARK-020 | `RssViolationEvent { ego_vel, lead_vel, gap, longitudinal_margin, lateral_margin, timestamp_ms }` appended to hash-chained audit ledger. | `append_rss_violation` + `verify_chain` test passes. |
| PARK-021 | 10 000 adversarial trajectory simulation via `ScenarioRunner` + `VirtualClock`. Assert zero unsafe commands exit the stack. | Test completes in < 60 s on CI; zero violations escape. |

---

## Increment 4 — Silicon Matrix Expansion
**Milestone:** v0.4 | **Epic:** `epic:silicon-matrix`
**Artifact:** Feature-gated binaries for QNN and OpenVINO (real); TIDL and ROCm
promoted from stubs to real backends when hardware CI is available.

| Task | What | Done When |
|------|------|-----------|
| PARK-022 | `QnnBackend` via Qualcomm AI Engine Direct SDK C FFI. Internal quantization for int8 models. `#[ignore]` hardware test vs. ORT CPU reference. | Inference on QCS6490 or SA8295; top-1 class matches ORT within tolerance. |
| PARK-023 | `TidlBackend` via TI TIDL runtime C FFI, cross-compiled to `aarch64-unknown-linux-gnu`. | Inference on TDA4VM; output within 1e-3 of ORT reference. |
| PARK-024 | `RocmBackend` via MIGraphX Rust bindings or ROCm HIP C FFI. GPU memory allocated once at init, not per inference. | Inference on RX 6000 or MI100; output within tolerance. |
| PARK-025 | `BackendSelector`: runtime backend selection by `BackendDescriptor`. Falls back to stub (with `tracing::warn!`) when target unavailable. | `BackendSelector::new(QualcommQnn)` on CI falls back to stub; returns `Ok`. |
| PARK-026 | Cross-backend determinism validation: same input on ORT + QnnStub + TidlStub → outputs within 1e-5 element-wise. | Test passes on CI; comment notes real-backend tolerance update in PARK-022/023. |

---

## Increment 5 — Safety OS Packaging
**Milestone:** v1.2 | **Epic:** `epic:packaging`
**Artifact:** `kirra v1.2.0` GitHub Release with x86_64/aarch64/armv7 tarballs
per backend variant.

| Task | What | Done When |
|------|------|-----------|
| PARK-027 | `kirra_safety_runtime` binary: posture engine + inference loop in one process. `KIRRA_BACKEND` env var selects backend via `BackendSelector`. Serves `/health`. | Binary starts, `/health` returns 200, inference loop ticks. |
| PARK-028 | `scripts/kirra-safety-runtime.service`: `WatchdogSec=5`, `MemoryMax=512M`, `CPUQuota=80%`. Restarts on watchdog timeout. | `systemd-analyze verify` reports no errors; service restarts on simulated timeout. |
| PARK-029 | `install.sh --backend <ort\|qnn\|tidl\|openvino\|rocm>`: downloads correct binary, configures systemd unit, non-interactive with `--yes`. | Unattended install with each backend variant completes without prompts. |
| PARK-030 | Dashboard panels: inference tick rate, backend P99 latency, RSS margin, posture sparkline. Renders live data; handles service-unreachable gracefully. | All four panels render against a running `kirra_safety_runtime`; show "—" when offline. |
| PARK-031 | Release pipeline: CI matrix builds all backend variants for three arches; attaches tarballs + SHA256SUMS to GitHub Release. | `kirra v1.2.0` release page shows all artifacts with checksums. |

---

## Increment 6 — Certification-Ready Runtime
**Milestone:** v2.0 | **Epic:** `epic:certification`
**Artifact:** Pre-assessment package for TÜV or SGS-TÜV Saar review.

| Task | What | Done When |
|------|------|-----------|
| PARK-032 | RTM (`KIRRA-RTM-001`) v1.0: every safety requirement traced to source line, test ID, and coverage entry. | Auditor can follow every requirement to a passing test without ambiguity. |
| PARK-033 | MC/DC coverage report for `posture_cache.rs`, `posture_engine_v2.rs`, `kirra_core.rs`, `rss.rs` via `cargo-llvm-cov`. CI fails if < 100%. | All four files at 100% MC/DC; report committed to `docs/coverage/`. |
| PARK-034 | FMEA (`KIRRA-FMEA-001`): posture stale cache, governor bypass, attestation replay, nonce exhaustion, RSS numerical overflow. Each mode has detection + mitigation. | Every failure mode has a severity, detection method, and mitigation entry. |
| PARK-035 | DFA (`KIRRA-DFA-001`): common-cause failures in HA active/passive pair on NFS-shared SQLite per ISO 26262 Part 9. | All single points of failure identified; independent protections proposed. |
| PARK-036 | `kirra_audit_verify` binary: read audit chain from SQLite, verify Ed25519 signatures, print tamper-evidence report, exit 1 on any corruption. | Correctly detects a single-byte corruption injected mid-chain. |
| PARK-037 | SOTIF (`KIRRA-SOTIF-001`): intended function boundaries, triggering conditions, evaluation scenarios per ISO 21448. | Document covers inference loop + RSS governor integration scenarios. |
| PARK-038 | HIL test harness: scriptable harness connecting `kirra_safety_runtime` to CARLA or kinematics integrator at 100 Hz. Zero RSS escapes on 1 000 trajectories. | Harness runs nightly; failures print timestamped CSV; README covers CARLA setup. |
| PARK-039 | Helm chart: `inferenceBackend`, `tickRateHz`, `modelPath`, `rssReactionTimeS` values. Chart deploys correct backend binary. | `helm lint` and `helm template` produce valid manifests for each backend. |
| PARK-040 | `docs/architecture.md`: Mermaid block diagram, data-flow, security boundaries, ASIL decomposition table. | Diagram matches current codebase; ASIL claims consistent with HARA. |
