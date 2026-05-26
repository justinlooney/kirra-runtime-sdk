# Kirra — Engineering Roadmap

This directory contains pre-execution architecture sketches and execution plans
for planned integrations and extensions. Each document represents a reviewed
and approved roadmap item with honest caveats, effort estimates, and explicit
sequencing dependencies.

For the full task-level roadmap (PARK-001 through PARK-040), see
[/work/roadmap.md](/work/roadmap.md).

## Documents

| Document | Description | Priority | Gating Dependencies | PARK Tasks |
|----------|-------------|----------|---------------------|------------|
| [RSS_KIRRA_INTEGRATION.md](RSS_KIRRA_INTEGRATION.md) | IEEE 2846 / RSS-style behavioral safety extension — canonical perception types, safe-distance evaluator, posture engine wiring, and audit chain entries | After Increment 2 (HAL complete) | parko-core v0.1.0 tagged (PARK-006); InferenceBackend finalized (PARK-008) | PARK-013 – PARK-019 |
| [APOLLO_KIRRA_INTEGRATION.md](APOLLO_KIRRA_INTEGRATION.md) | Apollo AV stack integration — Cyber RT bridge between Control and Canbus, posture sideband, demo scenarios | After Increment 3 + ROS2 demo | QNX spike (PARK-024); ROS2 bring-up (PARK-036, PARK-037) | Extends Increment 4 |

## Current Execution Order

**Now (actively in progress — see /work/active.md):**
- [~] PARK-001 — Attach `SafetyGovernor` to `ControlLoop` (GitHub Issue #6)
- [~] PARK-002 — Add `set_state_for_test` test seam (GitHub Issue #7)
- [~] PARK-003 — Posture divergence proptest (GitHub Issue #16)

**Increment 1 completion (v0.1):**
- PARK-004 NaN/Inf guard, PARK-005 Clock abstraction, PARK-006 release tag

**TIME-SENSITIVE parallel tracks:**
- PARK-020–023 TensorRT spike (Jetson arriving — highest leverage)
- PARK-024–026 QNX deployment (30-day license — do not let this expire)

**Increment 2 — HAL (v0.2):**
- PARK-007 through PARK-012 (backend trait, CPU ONNX validation, MockBackend, stubs)

**Increment 3 — Behavioral Safety / IEEE 2846 (v0.3):**
- PARK-013–019: RSS safe-distance, posture engine wiring, audit chain, 10k-scenario sim
- RSS_KIRRA_INTEGRATION.md is the detailed specification for this increment

**Increment 4 — Silicon Matrix Expansion (v0.4):**
- QNN, TIDL, OpenVINO, AMD backends (most blocked on hardware)

**Increment 5 — Packaging (v1.2):**
- PARK-031–035 Docker/Helm/installer/systemd/QNX artifact

**Increment 6 — Robot Stack + Certification (v2.0):**
- PARK-036–038 ROS2 + Hiwonder (BLOCKED on hardware delivery)
- PARK-039–040 IEC 61508 SIL 3 / ASTM F3269 mappings
- Apollo integration (APOLLO_KIRRA_INTEGRATION.md) follows ROS2 demo

## Sequencing Rules

1. Do not start Increment 3 (RSS / IEEE 2846) before Increment 1 is tagged.
2. Do not start Apollo integration before the ROS2 interlock demo (PARK-037) is on record.
3. Do not let PARK-024 (QNX, 30-day license) slip — treat as a hard deadline regardless of other increment progress.
4. The RSS evaluator and Apollo bridge are capability extensions; they add value to an already-working product. Core runtime + QNX + hardware validation come first.
