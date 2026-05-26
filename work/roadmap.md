# Roadmap

> Lean-agile increments. Each is independently testable and ships a concrete
> artifact. Framing reflects current state: CPU ONNX backend exists in
> parko-onnx; multi-silicon backends are first-implementation new work;
> IEEE 2846, IEC 61508 SIL 3, and ASTM F3269 integrations are planned but
> not yet implemented.

---

## Increment 1 — Deterministic Runtime Core
**Milestone:** v0.1 | **Epic:** `epic:runtime-core`
**Artifact:** `parko-core` v0.1.0 — a clock-driven, posture-aware ControlLoop
consumable as a library by downstream inference and governor crates.

| Task | What | Done When |
|------|------|-----------|
| PARK-001 | Implement `ControlLoop::with_governor` builder. Stores governor as `Option<Box<dyn SafetyGovernor>>`. Suppresses built-in scalar clamp when a governor is present. | `test_builtin_clamp_suppressed` passes; all existing parko-core tests remain green. |
| PARK-002 | Add `set_state_for_test(state: PostureState)` behind `#[cfg(test)]`. Provides a test seam for posture-divergence tests without exposing a production mutation path. | Method absent from `cargo build --release` (verified with `nm`); callable in `cargo test`. |
| PARK-003 | Write proptest suite asserting governor output is at least as conservative as the built-in clamp for all valid `(proposed_output, PostureState)` pairs. | ≥ 10 000 cases pass for Nominal, Degraded, and LockedOut. |
| PARK-004 | Add NaN/Inf input guard at `ControlLoop::tick` entry. Any non-finite float returns `EnforcementAction::Halt` before reaching the governor or clamp. | Property test confirms zero non-finite values reach the governor. |
| PARK-005 | Wire `VirtualClock` / `SystemClock` abstraction into `ControlLoop`. Enables deterministic temporal tests without `sleep`. | Test advances `VirtualClock` manually; all timing logic exercisable without wall-clock. |
| PARK-006 | Tag `parko-core-v0.1.0`. Set version in `Cargo.toml`; verify `cargo publish --dry-run` exits cleanly. | Tag in repo; dry-run passes. |

---

## Increment 2 — Hardware Abstraction Layer
**Milestone:** v0.2 | **Epic:** `epic:hal`
**Artifact:** `parko-core` v0.2.0 with `InferenceBackend` trait, validated CPU
backend (parko-onnx), `MockBackend`, and feature-gated stubs for all four
hardware targets. Multi-silicon real backends are Increment 4.

| Task | What | Done When |
|------|------|-----------|
| PARK-007 | Define `InferenceBackend` trait with zero-copy `run(&self, input: &[f32], output: &mut [f32])` and `BackendDescriptor` enum. All scratch memory pre-allocated at init. | Trait compiles; parko-onnx CPU backend implements it; round-trip test passes. |
| PARK-008 | Validate parko-onnx CPU ONNX Runtime backend against the `InferenceBackend` trait. Confirm the MNIST-style integration test passes. | `cargo test -p parko-onnx` exits 0; MNIST integration test is verified green. |
| PARK-009 | Add `MockBackend` to parko-core: configurable deterministic output for unit tests. Eliminates the ORT dependency from the parko-core test binary. | parko-core tests use `MockBackend`; no ORT link in `cargo test -p parko-core`. |
| PARK-010 | Feature-gated stub backends for QNN, TIDL, ROCm/Vitis, OpenVINO. Each returns deterministic zeros; gated behind `features = ["backend-<name>"]`. | `cargo test --features backend-<name>` passes on ubuntu-latest for all four stubs without hardware. |
| PARK-011 | Backend latency watchdog in `InferenceLoop`: deadline exceeded → `LatencyViolation`, hold last safe output, three consecutive violations → posture `Degraded`. | Test with configurable-latency stub + short deadline triggers watchdog and degrades posture. |
| PARK-012 | GitHub Actions matrix: build and test all four stub backends in one workflow run on ubuntu-latest. | All four feature flags green in the same CI run. |

---

## Increment 3 — Behavioral Safety (IEEE 2846-equivalent)
**Milestone:** v0.3 | **Epic:** `epic:behavioral-safety`
**Artifact:** RSS-gated Kirra governor with tamper-evident violation log, passing
a 10 000-scenario adversarial simulation. This is the first implementation of
IEEE 2846-style behavioral safety — no prior code exists.

| Task | What | Done When |
|------|------|-----------|
| PARK-013 | Implement `longitudinal_safe_distance` per IEEE 2846-2022 §5.1. Inputs: ego_vel, lead_vel, reaction_time, accel_max, brake_min, brake_max. | Unit tests cover equal, faster, slower, and zero-speed cases; values match IEEE reference. |
| PARK-014 | Implement `lateral_safe_distance` per IEEE 2846-2022 §5.2. Inputs: lateral velocities, max lateral accel, reaction time. | Unit tests cover converging, diverging, and stationary cases. |
| PARK-015 | Wire `RssState { safe, longitudinal_margin, lateral_margin }` into `kirra-runtime-sdk` posture engine. RSS violation → `Degraded`; recovery uses existing 5-tick hysteresis. | Integration test: inject violation → Degraded; inject 5 clean ticks → Nominal. |
| PARK-016 | RSS pre-actuator gate in the Kirra governor crate: `rss_state.safe == false` clamps velocity to 0.0 before any kinematic envelope check. | Unit test: safe=false + positive velocity → output 0.0; safe=true → normal kinematics. |
| PARK-017 | RSS property test (proptest): for all valid `(ego_vel, lead_vel, gap, commanded_vel)`, no RSS-violating command exits the governor under any posture state. | ≥ 10 000 cases pass; all three PostureState variants covered. |
| PARK-018 | `RssViolationEvent { ego_vel, lead_vel, gap, longitudinal_margin, lateral_margin, timestamp_ms }` appended to hash-chained audit ledger in `kirra-runtime-sdk`. | `append_rss_violation` + `verify_chain` test passes; single-byte corruption detected. |
| PARK-019 | 10 000-scenario adversarial trajectory simulation via `ScenarioRunner` + `VirtualClock`. Assert zero unsafe commands exit the full stack. | Test completes in < 60 s on CI; zero violations escape. |

---

## Increment 4 — Silicon Matrix Expansion
**Milestone:** v0.4 | **Epic:** `epic:silicon-matrix`
**Artifact:** Real backends for QNN and OpenVINO (CI-testable); TIDL and
ROCm/Vitis real backends require hardware CI. All four are first-implementation
work — the architecture is defined but no backend code exists yet.

| Task | What | Done When |
|------|------|-----------|
| PARK-020 | `QnnBackend` (first implementation) via Qualcomm AI Engine Direct SDK C FFI. int8 quantization from tensor metadata. Hardware test `#[ignore]`'d in CI. | Inference on QCS6490 or SA8295; top-1 class matches ORT CPU reference within tolerance. |
| PARK-021 | `TidlBackend` (first implementation) via TI TIDL runtime C FFI, cross-compiled to `aarch64-unknown-linux-gnu`. | Inference on TDA4VM; output within 1e-3 of ORT reference. |
| PARK-022 | `RocmBackend` (first implementation) via ROCm HIP C FFI or MIGraphX bindings. GPU memory allocated once at init, not per inference. | Inference on RX 6000 or MI100; output within tolerance of ORT reference. |
| PARK-023 | `OpenVinoBackend` (first implementation) using `openvino-rs`. Model loading, input shape validation, zero-copy output slice writing. | Integration test with identity model confirms output matches input within 1e-6. |
| PARK-024 | `BackendSelector`: runtime backend selection by `BackendDescriptor`. Falls back to stub (`tracing::warn!`) when target unavailable. | `BackendSelector::new(QualcommQnn)` on CI falls back to stub; returns `Ok`. |
| PARK-025 | Cross-backend determinism: same input on ORT + QnnStub + TidlStub → outputs within 1e-5 element-wise. Comment notes real-hardware tolerance update for PARK-020/021. | Test passes on CI without hardware. |

---

## Increment 5 — Safety OS Packaging
**Milestone:** v1.2 | **Epic:** `epic:packaging`
**Artifact:** `kirra v1.2.0` GitHub Release with x86_64/aarch64/armv7 tarballs
per backend variant; systemd-managed service; React dashboard panels.

| Task | What | Done When |
|------|------|-----------|
| PARK-026 | `kirra_safety_runtime` binary: posture engine + inference loop in one process. `KIRRA_BACKEND` env var selects backend. Serves `/health`. | Binary starts, `/health` returns 200, inference loop ticks at configured rate. |
| PARK-027 | `scripts/kirra-safety-runtime.service`: `WatchdogSec=5`, `MemoryMax=512M`, `CPUQuota=80%`. Restarts on watchdog timeout. | `systemd-analyze verify` reports no errors; service restarts on simulated timeout. |
| PARK-028 | `install.sh --backend <ort\|qnn\|tidl\|openvino\|rocm>`: downloads correct binary, configures systemd unit, non-interactive with `--yes`. | Unattended install with each variant completes without prompts. |
| PARK-029 | Dashboard panels: inference tick rate, backend P99 latency, RSS margin, posture sparkline. Graceful offline handling. | All four panels render against running service; show "—" when offline. |
| PARK-030 | Release pipeline: CI matrix builds all backend variants for three arches; attaches tarballs + SHA256SUMS to GitHub Release. | `kirra v1.2.0` release page shows all artifacts with checksums. |

---

## Increment 6 — Certification-Ready Runtime
**Milestone:** v2.0 | **Epic:** `epic:certification`
**Artifact:** Pre-assessment package for TÜV or SGS-TÜV Saar review, including
first-implementation IEC 61508 SIL 3 and ASTM F3269 mappings (these do not
yet exist; Increment 6 is where they are written).

| Task | What | Done When |
|------|------|-----------|
| PARK-031 | Complete RTM (`KIRRA-RTM-001`) v1.0: every ISO 26262 ASIL-D safety requirement traced to source line, test ID, and coverage entry. | Auditor can follow every requirement to a passing test without ambiguity. |
| PARK-032 | **First implementation:** IEC 61508 SIL 3 requirements mapping. Map existing safety functions to SIL 3 claims; identify gaps and required mitigations. | Every SIL 3 safety function claim has a corresponding implementation entry or explicit gap note. |
| PARK-033 | **First implementation:** ASTM F3269-21 bounded-operation envelope mapping. Define Nominal, Degraded, and BLLOS operational envelopes per §6. | Each operational mode has a defined envelope; claims traceable to posture engine states. |
| PARK-034 | MC/DC coverage for `posture_cache.rs`, `posture_engine_v2.rs`, `kirra_core.rs`, `rss.rs` via `cargo-llvm-cov`. CI fails if < 100%. | All four files at 100% MC/DC; report committed to `docs/coverage/`. |
| PARK-035 | FMEA (`KIRRA-FMEA-001`): posture stale cache, governor bypass, attestation replay, nonce exhaustion, RSS numerical overflow, backend latency. | Every failure mode has severity, detection method, and mitigation entry. |
| PARK-036 | DFA (`KIRRA-DFA-001`): common-cause failures in HA active/passive pair on NFS-shared SQLite per ISO 26262 Part 9. | All single points of failure identified; independent protections proposed. |
| PARK-037 | `kirra_audit_verify` binary: read audit chain from SQLite, verify Ed25519 signatures, print tamper-evidence report, exit 1 on corruption. | Correctly detects a single-byte corruption injected mid-chain. |
| PARK-038 | SOTIF (`KIRRA-SOTIF-001`): intended function boundaries, triggering conditions, evaluation scenarios per ISO 21448. | Document covers inference loop + RSS governor integration scenarios. |
| PARK-039 | HIL test harness: connects `kirra_safety_runtime` to CARLA or kinematics integrator at 100 Hz; zero RSS escapes on 1 000 trajectories. | Harness runs nightly; failures print timestamped CSV; README covers CARLA setup. |
| PARK-040 | Update `docs/architecture.md`: Mermaid block diagram, data-flow, security boundaries, ASIL decomposition, multi-silicon backend map. | Diagram matches current codebase; ASIL claims consistent with HARA. |
