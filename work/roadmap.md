# Roadmap

> Lean-agile increments. Each is independently testable and ships a concrete
> artifact. Framing is corrected: hardware backends are first-implementation new
> work; IEEE 2846, IEC 61508 SIL 3, and ASTM F3269 are planned/target standards
> not yet implemented; the CPU ONNX backend (parko-onnx) exists but its MNIST
> integration test must be verified before being called green.

---

## Hardware Availability Matrix

| Backend | Hardware needed | Available now? | Notes |
|---------|----------------|----------------|-------|
| CPU ONNX | Any x86/ARM | Yes | Baseline backend |
| TensorRT | NVIDIA GPU/Jetson | Jetson arriving | Highest leverage — start now |
| QNN | Snapdragon SoC | No | Tied to QNX deployment |
| TIDL | TI TDA4/J7 DSP | No | Industrial robotics |
| OpenVINO | Intel x86/VPU | Partial | x86 only for now |
| AMD Vitis | Xilinx FPGA | No | Likely AMD path |
| AMD ROCm | AMD GPU | No | Defer unless needed |

---

## What Exists Today vs New Work

| Component | Status |
|-----------|--------|
| Parko-core runtime | EXISTS (~30–40 tests) |
| Parko-ONNX CPU backend | EXISTS (integration test unverified) |
| Kirra Governor (kinematic) | EXISTS (crate name unverified — search before renaming) |
| Kirra Runtime SDK safety fabric | EXISTS (~333 tests) |
| React dashboard | EXISTS |
| Docker + Helm deployment | EXISTS |
| ISO 26262 ASIL-D docs | EXISTS (HARA, Goals, RTM, Arch) |
| Backend trait definition | EXISTS (needs refinement) |
| TensorRT backend | NEW WORK (Jetson arriving) |
| QNN backend | NEW WORK (no hardware) |
| TIDL backend | NEW WORK (no hardware) |
| OpenVINO backend | NEW WORK (x86 partial only) |
| AMD backend | NEW WORK (no hardware) |
| IEC 61508 SIL 3 mapping | NEW WORK (target only) |
| ASTM F3269 RTA mapping | NEW WORK (target only) |
| IEEE 2846 behavioral safety | PLANNED (design doc only, not implemented) |
| QNX deployment | IN PROGRESS (30-day license) |
| Reference robot stack | BLOCKED (Hiwonder hardware + ROS2 Jazzy) |

---

## Execution Order

- **Now:** CPU ONNX stabilization, Backend trait refinement, TensorRT API spike
  (when Jetson arrives), QNX deployment spike (TIME-SENSITIVE — 30-day license)
- **Soon:** QNN coordination with QNX path; posture divergence tests; NaN guard
- **Later:** TIDL, OpenVINO, AMD (blocked on hardware/customers); IEEE 2846
  behavioral safety; IEC 61508 / ASTM F3269 certification mappings

---

## Increment 1 — Deterministic Runtime Core
**Milestone:** v0.1 | **Epic:** `epic:runtime-core`
**Artifact:** `parko-core` v0.1.0 — a clock-driven, posture-aware ControlLoop
consumable as a library by downstream inference and governor crates.

| Task | What | Done When |
|------|------|-----------|
| PARK-001 | Attach `SafetyGovernor` to `ControlLoop`. Stores governor as `Option<Box<dyn SafetyGovernor>>`. Built-in scalar clamp suppressed when governor present. | `test_builtin_clamp_suppressed` passes; all existing parko-core tests green. |
| PARK-002 | Add `set_state_for_test(state: PostureState)` behind `#[cfg(test)]`. Test seam for posture-divergence tests; no production mutation path. | Method absent from release build (nm); callable under cargo test. |
| PARK-003 | Posture-divergence proptest: governor output ≤ builtin ceiling for all (proposed, PostureState) pairs. 10,000 cases per variant. | ≥ 10,000 cases pass for Nominal, Degraded, LockedOut. |
| PARK-004 | NaN/Inf input guard at `tick()` entry: non-finite float → `EnforcementAction::Halt` before governor or clamp. | Property test confirms zero non-finite values reach governor. |
| PARK-005 | Wire `RuntimeClock` / `MockClock` abstraction into `ControlLoop`. All timing calls use `self.clock.now_ms()`; no wall-clock in timing-sensitive code. | Test advances `MockClock` manually; timing logic exercisable without sleep. |
| PARK-006 | Tag `parko-core-v0.1.0`. Set version in `Cargo.toml`; verify `cargo publish --dry-run` exits cleanly. | Tag in repo; dry-run passes. |

---

## Increment 2 — Hardware Abstraction Layer
**Milestone:** v0.2 | **Epic:** `epic:hal`
**Artifact:** Refined `InferenceBackend` trait, validated CPU ONNX backend,
`MockBackend` for unit tests, feature-gated stubs for all hardware targets,
and a TensorRT MVP when the Jetson arrives. All multi-silicon real backends
are Increment 4.

| Task | What | Done When |
|------|------|-----------|
| PARK-007 | Verify actual crate and struct names in `parko/` workspace before any rename or refactor. Document findings in decisions.md. | decisions.md updated with verified names; no broken imports. |
| PARK-008 | Finalize `InferenceBackend` trait zero-copy boundary: `run(&self, input: &[f32], output: &mut [f32]) -> Result<(), BackendError>`. All scratch memory pre-allocated at `new()`. | Trait compiles; shape mismatch returns `BackendError::ShapeMismatch`; never panics. |
| PARK-009 | Validate parko-onnx CPU backend against `InferenceBackend` trait. Verify MNIST integration test is green — do not assume it passes without running it. | `cargo test -p parko-onnx` exits 0; MNIST test verified green. |
| PARK-010 | Add `MockBackend` to parko-core: configurable deterministic output. Eliminates ORT dependency from parko-core test binary. | parko-core tests use `MockBackend`; no ORT link in `cargo test -p parko-core`. |
| PARK-011 | Define backend capability reporting: `capabilities()` method + `BackendDescriptor` enum covering all target backends. | All stubs return valid descriptors; capability query compiles on CI. |
| PARK-012 | Feature-gated zero-output stub backends for TensorRT, QNN, TIDL, OpenVINO, AMD. CI builds and tests all stubs without hardware. | `cargo test --features backend-<name>` passes on ubuntu-latest for all stubs. |

---

## Increment 3 — Behavioral Safety (IEEE 2846 — Planned)
**Milestone:** v0.3 | **Epic:** `epic:behavioral-safety`
**Status:** IEEE 2846 is a design doc only — no behavioral-safety code exists
yet. This increment implements the RSS governor integration from scratch.

| Task | What | Done When |
|------|------|-----------|
| PARK-013 | Implement `longitudinal_safe_distance` per IEEE 2846-2022 §5.1. First implementation; no prior behavioral-safety code exists. | Unit tests match IEEE reference values; no NaN/overflow on edge cases. |
| PARK-014 | Implement `lateral_safe_distance` per IEEE 2846-2022 §5.2. | Unit tests cover converging, diverging, and stationary cases. |
| PARK-015 | Wire `RssState { safe, longitudinal_margin, lateral_margin }` into posture engine. RSS violation → Degraded; 5-tick / 10 s hysteresis recovery. | Integration test: violation → Degraded; 5 clean ticks → Nominal. |
| PARK-016 | RSS pre-actuator gate in KirraGovernor: `rss_state.safe == false` clamps to 0.0 before any kinematic check. | Unit test: safe=false + positive velocity → 0.0; safe=true → normal kinematics. |
| PARK-017 | RSS property test: for all (ego_vel, lead_vel, gap, commanded_vel) in plausible range, no RSS-violating command exits governor. 10,000 cases per PostureState variant. | All three PostureState variants covered; all cases pass. |
| PARK-018 | `RssViolationEvent` appended to SHA-256 hash-chained audit ledger. Single-byte corruption causes `verify_chain()` to fail. | append + verify_chain test passes; tamper detection confirmed. |
| PARK-019 | 10,000-scenario adversarial trajectory simulation via `ScenarioRunner` + `MockClock`. Zero unsafe commands exit; < 60 s on CI. | Zero violations escape; test completes in < 60 s on CI. |

---

## Increment 4 — Silicon Matrix Expansion
**Milestone:** v0.4 | **Epic:** `epic:silicon-matrix`
**Status:** All hardware backends are NEW WORK. No backend code exists beyond
the CPU ONNX baseline. TensorRT is highest-priority (Jetson arriving). QNX is
TIME-SENSITIVE (30-day license in progress).

### TensorRT (TIME-SENSITIVE — Jetson arriving)

| Task | What | Done When |
|------|------|-----------|
| PARK-020 | TensorRT API spike: set up FFI bindings; verify trivial model loads and runs on Jetson. | TRT runtime loads; test model executes without segfault on Jetson. |
| PARK-021 | Implement `TensorRTBackend` struct: `new(engine_path)`, pre-allocated CUDA buffers, zero per-inference allocation. | Inference on Jetson; no per-inference alloc; `run()` matches CPU output within 1e-3. |
| PARK-022 | Integrate TensorRT into `BackendSelector`: `KIRRA_BACKEND=tensorrt` selects TRT; falls back to stub with `tracing::warn!`. | Backend selection works; fallback to stub on CI without GPU. |
| PARK-023 | CPU vs TensorRT output comparison: same input, outputs within 1e-3 element-wise. Hardware test `#[ignore]`'d in CI. | Tolerance test passes on Jetson; comment documents hardware-only status. |

### QNX + QNN Coordination (TIME-SENSITIVE — 30-day license)

| Task | What | Done When |
|------|------|-----------|
| PARK-024 | QNX deployment spike: bring up `kirra_verifier_service` binary on QNX. Identify POSIX subset gaps. | Service starts on QNX; `/health` returns 200. |
| PARK-025 | QNN + QNX compatibility analysis: document SDK version requirements, FFI linking, memory model differences from Linux. | Analysis in decisions.md; no surprises at link time. |
| PARK-026 | Define QNX-safe backend selection rules: no dynamic allocation in hot-path; document POSIX subset constraints. | Rules documented; `BackendSelector` respects QNX constraints. |

### Other Hardware (Deferred — blocked on hardware/customers)

| Task | What | Done When |
|------|------|-----------|
| PARK-027 | QNN backend MVP via Qualcomm AI Engine Direct SDK C FFI. First implementation; no prior QNN code exists. Hardware test `#[ignore]`'d. | Inference on QCS6490; top-1 class matches CPU reference within tolerance. |
| PARK-028 | TI TIDL backend MVP via TIDL C FFI, cross-compiled to `aarch64-unknown-linux-gnu`. First implementation. Hardware test `#[ignore]`'d. | Inference on TDA4VM; output within 1e-3 of CPU reference. |
| PARK-029 | OpenVINO backend MVP using `openvino-rs`. Testable in CI via CPU plugin. First implementation. | CI test with identity model passes; output within 1e-6 of input. |
| PARK-030 | AMD backend MVP: decide Vitis AI vs ROCm based on customer; implement chosen path. First implementation. | Decision recorded in decisions.md; MVP runs on target hardware. |

---

## Increment 5 — Safety OS Packaging
**Milestone:** v1.2 | **Epic:** `epic:packaging`

| Task | What | Done When |
|------|------|-----------|
| PARK-031 | Normalize Kirra naming across Docker image, Helm chart, env vars, and service unit files. Remove remaining Aegis references. | `grep -r aegis` returns only intentional references; build and install pass. |
| PARK-032 | Add Parko runtime into Kirra Docker image. One image: parko-core + kirra-runtime-sdk + KirraGovernor + dashboard. | Single image starts; `/health` and inference loop both respond. |
| PARK-033 | Backend-aware installer: `install.sh --backend <cpu|tensorrt|qnn|...>`. Non-interactive with `--yes`. | Unattended install for each variant completes without prompts. |
| PARK-034 | systemd unit with watchdog: `WatchdogSec=5`, `MemoryMax=512M`, `CPUQuota=80%`. | `systemd-analyze verify` passes; service restarts on simulated watchdog timeout. |
| PARK-035 | QNX packaging stub: define `kirra-qnx.tar.gz` artifact structure and placeholder Makefile. Blocked until PARK-024. | Stub artifact builds; README covers what fills in when QNX work lands. |

---

## Increment 6 — Reference Robot Stack + Certification
**Milestone:** v2.0 | **Epic:** `epic:certification`

### Reference Robot Stack (BLOCKED — Hiwonder hardware + ROS2 Jazzy)

| Task | What | Done When |
|------|------|-----------|
| PARK-036 | Bring up ROS2 Jazzy on Ubuntu 24.04. Configure colcon workspace; verify basic pub/sub. | `ros2 topic echo` works; workspace builds cleanly. |
| PARK-037 | Integrate Parko + KirraGovernor with ROS2 cmd_vel topics. Governor clamps observable on `filtered_cmd_vel`. | Closed-loop behavior on Hiwonder; governor clamps verified on filtered topic. |
| PARK-038 | Build full reference robot stack: Parko + KirraGovernor + ROS2 + kirra_safety interlock + CARLA alternative. | BLOCKED: depends on Hiwonder hardware + ROS2 Jazzy setup. |

### Safety Case (all NEW WORK)

| Task | What | Done When |
|------|------|-----------|
| PARK-039 | Map IEC 61508 SIL 3 requirements: identify SIL 3 claims in existing safety functions; document gaps and required mitigations. | Every SIL 3 claim has an implementation entry or explicit gap note. |
| PARK-040 | Map ASTM F3269-21 bounded-operation envelope: define Nominal, Degraded, BLLOS envelopes per §6; trace to posture engine states. | Each mode has a defined envelope; claims traceable to posture engine states. |
