#!/usr/bin/env bash
#
# install-parko-backend.sh — TARGET-PARAMETERIZED Parko backend / chipset
# installer. The runtime (ML inference backend) layer that composes WITH the
# gateway installer (install.sh). install.sh installs the gateway
# (kirra_verifier_service) as a systemd binary; THIS installs and validates the
# selected Parko inference backend for a chosen silicon target. It does not
# replace install.sh — see INSTALL.md "Multi-Backend / Multi-Chipset".
#
# ONE framework, per-target dispatch keyed on BackendDescriptor. The flow for
# every target is identical:
#
#   select target → acquire runtime → build Parko (right feature/crate)
#     → apply posture config → FAIL-CLOSED validate the backend loads
#     → run the common (chipset-independent) safety gates
#
# Target names are aligned 1:1 with the scheduler's descriptor strings
# (parko-core/src/scheduler.rs `descriptor_vendor`):
#   ort-cpu  openvino  tensorrt  qnn  ti-tidl  amd-vitis
#
# FAIL-CLOSED is the whole point: if the selected backend's runtime/EP does not
# load, the install REFUSES — it NEVER silently substitutes another backend.
# This generalizes parko-tensorrt's `.error_on_failure()` to every target.
#
# SAFETY: a wrong-but-loaded backend, or a silent CPU fallback on a GPU target,
# is exactly the confidently-wrong hazard the governor cannot catch. So backend
# selection is EXPLICIT (auto-detect only suggests, never auto-proceeds), and the
# common safety gates are NOT skippable.

set -euo pipefail

# ── presentation (mirrors install.sh) ────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'
BOLD='\033[1m'; NC='\033[0m'
info()    { echo -e "${BLUE}[INFO]${NC}  $*"; }
success() { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
fatal()   { error "$*"; exit 1; }
section() { echo ""; echo -e "${BOLD}━━━ $* ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"; }

# ── the matrix: single source, aligned with scheduler descriptor strings ──────
# Columns per target: descriptor | kind | crate | cargo-feature | runtime-source
#   kind: real     — installable + validatable here (CPU anywhere; Intel on dev box)
#         scaffold — install LOGIC present; real inference is hardware-gated (Jetson)
#         stub     — framework slot only; real backend pending + VENDOR-SDK-gated
#
# REAL    Cpu          → ort-cpu   → parko-onnx     (ONNX Runtime, freely pullable)
# REAL    IntelOpenVino→ openvino  → parko-openvino (OpenVINO runtime, freely pullable)
# SCAFFOLD TensorRT    → tensorrt  → parko-tensorrt (NVIDIA TRT-enabled ORT; Jetson)
# STUB    QualcommQnn  → qnn       → (PARK-027)     requires Qualcomm QNN SDK
# STUB    TiTidl       → ti-tidl   → (PARK-028)     requires TI TIDL / Processor SDK
# STUB    AmdVitis     → amd-vitis → (PARK-030)     requires AMD Vitis AI

ALL_TARGETS="ort-cpu openvino tensorrt qnn ti-tidl amd-vitis"

target_descriptor() { case "$1" in
    ort-cpu)   echo "Cpu" ;;        openvino)  echo "IntelOpenVino" ;;
    tensorrt)  echo "TensorRT" ;;   qnn)       echo "QualcommQnn" ;;
    ti-tidl)   echo "TiTidl" ;;     amd-vitis) echo "AmdVitis" ;;
    *) return 1 ;; esac; }

target_kind() { case "$1" in
    ort-cpu|openvino)        echo "real" ;;
    tensorrt)                echo "scaffold" ;;
    qnn|ti-tidl|amd-vitis)   echo "stub" ;;
    *) return 1 ;; esac; }

target_crate() { case "$1" in
    ort-cpu)   echo "parko-onnx" ;;     openvino) echo "parko-openvino" ;;
    tensorrt)  echo "parko-tensorrt" ;; qnn|ti-tidl|amd-vitis) echo "(pending)" ;;
    *) return 1 ;; esac; }

# Cargo feature on parko-core's backend-* set (CI stubs) / the consuming node.
target_feature() { case "$1" in
    ort-cpu)   echo "(default — parko-onnx is its own crate)" ;;
    openvino)  echo "backend-openvino" ;;
    tensorrt)  echo "backend-tensorrt" ;;
    qnn)       echo "backend-qnn" ;;
    ti-tidl)   echo "backend-tidl" ;;
    amd-vitis) echo "backend-amd" ;;
    *) return 1 ;; esac; }

target_runtime_note() { case "$1" in
    ort-cpu)   echo "ONNX Runtime shared lib (Microsoft CPU build; freely pullable)" ;;
    openvino)  echo "OpenVINO runtime (pip wheel / apt; freely pullable)" ;;
    tensorrt)  echo "NVIDIA TensorRT-enabled ONNX Runtime (JetPack/L4T on the Jetson)" ;;
    qnn)       echo "requires Qualcomm QNN SDK (vendor-gated: registration/license)" ;;
    ti-tidl)   echo "requires TI TIDL / Processor SDK (vendor-gated)" ;;
    amd-vitis) echo "requires AMD Vitis AI (vendor-gated)" ;;
    *) return 1 ;; esac; }

# Determinism / precision posture applied per target (audit-relevant).
target_posture() { case "$1" in
    ort-cpu)   echo "single-thread + GraphOptimizationLevel::Disable (bitwise-reproducible)" ;;
    openvino)  echo "ACCURACY + INFERENCE_PRECISION_HINT=f32 + LATENCY (mirrors ORT-CPU)" ;;
    tensorrt)  echo "fp16=false, int8=false, engine-cache on; TF32 UNENFORCED (Jetson-gated); NOT bitwise-reproducible — decision-agreement posture" ;;
    qnn|ti-tidl|amd-vitis) echo "(defined when the real backend lands)" ;;
    *) return 1 ;; esac; }

# ── argument parsing ──────────────────────────────────────────────────────────
TARGET=""
AUTO_DETECT=false
CONFIRMED=false
NON_INTERACTIVE=false
DRY_RUN=false

usage() {
    cat <<EOF
Usage: sudo bash install-parko-backend.sh --target <TARGET> [OPTIONS]

Installs and FAIL-CLOSED-validates a Parko inference backend for one silicon
target. Composes with the gateway installer (install.sh).

Targets (aligned with the scheduler's descriptor strings):
  ort-cpu     REAL      Cpu           → parko-onnx      (CPU, anywhere)
  openvino    REAL      IntelOpenVino → parko-openvino  (Intel CPU/iGPU/VPU)
  tensorrt    SCAFFOLD  TensorRT      → parko-tensorrt  (NVIDIA Jetson; HW-gated)
  qnn         STUB      QualcommQnn   → PARK-027        (requires Qualcomm QNN SDK)
  ti-tidl     STUB      TiTidl        → PARK-028        (requires TI TIDL SDK)
  amd-vitis   STUB      AmdVitis      → PARK-030        (requires AMD Vitis AI)

Options:
  --target <name>    Select the backend target (EXPLICIT — recommended).
  --auto-detect      SUGGEST a target from detected hardware; requires --confirm
                     (or an interactive yes) before proceeding. Never auto-runs.
  --confirm          Accept the auto-detected suggestion non-interactively.
  --non-interactive  No prompts.
  --dry-run          Print every step without acquiring/building/installing.
                     Safe to run anywhere (no hardware, no root). Used by the
                     framework self-test.
  --list             Print the target matrix and exit.
  --help             This help.

FAIL-CLOSED: if the selected backend's runtime/EP does not load, the install
REFUSES — it never substitutes another backend. The common safety gates always
run and cannot be skipped.
EOF
}

print_matrix() {
    section "Parko backend / chipset target matrix"
    printf "%-10s %-9s %-16s %-16s %s\n" "TARGET" "KIND" "DESCRIPTOR" "CRATE" "RUNTIME"
    for t in $ALL_TARGETS; do
        printf "%-10s %-9s %-16s %-16s %s\n" \
            "$t" "$(target_kind "$t")" "$(target_descriptor "$t")" \
            "$(target_crate "$t")" "$(target_runtime_note "$t")"
    done
}

while [ $# -gt 0 ]; do
    case "$1" in
        --target) TARGET="${2:-}"; shift 2 ;;
        --target=*) TARGET="${1#*=}"; shift ;;
        --auto-detect) AUTO_DETECT=true; shift ;;
        --confirm) CONFIRMED=true; shift ;;
        --non-interactive) NON_INTERACTIVE=true; shift ;;
        --dry-run) DRY_RUN=true; shift ;;
        --list) print_matrix; exit 0 ;;
        --help|-h) usage; exit 0 ;;
        # NOTE: there is deliberately NO --skip-safety-gates. The common safety
        # gates are non-skippable; a flag to bypass them would defeat the point.
        *) fatal "Unknown argument: $1 (see --help)" ;;
    esac
done

# ── selection (explicit; auto-detect only suggests) ───────────────────────────
auto_detect_suggest() {
    # Best-effort hardware sniff → a SUGGESTION only. Never authoritative.
    if command -v nvidia-smi >/dev/null 2>&1 || [ -e /dev/nvgpu ] || [ -d /proc/device-tree ] && grep -qi nvidia /proc/device-tree/model 2>/dev/null; then
        echo "tensorrt"; return
    fi
    if [ -d /dev/dri ] && grep -qi "GenuineIntel" /proc/cpuinfo 2>/dev/null; then
        echo "openvino"; return
    fi
    echo "ort-cpu"  # safe default suggestion
}

select_target() {
    if [ "$AUTO_DETECT" = true ]; then
        local suggested; suggested="$(auto_detect_suggest)"
        warn "Auto-detect is a SUGGESTION, not a decision (it can mask a misconfig"
        warn "or pick the wrong silicon — unsafe to trust blindly for a safety runtime)."
        info "Suggested target from detected hardware: ${BOLD}${suggested}${NC}"
        if [ "$CONFIRMED" = true ]; then
            TARGET="$suggested"
            info "Accepted via --confirm."
        elif [ "$NON_INTERACTIVE" = true ]; then
            fatal "Auto-detect requires explicit confirmation. Re-run with --confirm \
or pass --target ${suggested} to proceed."
        else
            read -r -p "Use '${suggested}'? Type the target name to confirm: " answer
            [ "$answer" = "$suggested" ] || fatal "Not confirmed — refusing to guess. Pass --target explicitly."
            TARGET="$suggested"
        fi
    fi
    [ -n "$TARGET" ] || { usage; fatal "No target selected. Pass --target <name> (recommended) or --auto-detect."; }
    target_descriptor "$TARGET" >/dev/null 2>&1 || fatal "Unknown target '${TARGET}'. Valid: ${ALL_TARGETS}"
}

# ── per-target runtime acquisition ────────────────────────────────────────────
acquire_runtime() {
    local t="$1"; local kind; kind="$(target_kind "$t")"
    section "1/5 Acquire runtime — ${t} ($(target_descriptor "$t"))"
    info "Runtime: $(target_runtime_note "$t")"
    if [ "$kind" = "stub" ]; then
        # HONEST: vendor SDKs are gated (registration/license), not freely
        # pullable. The slot does not pretend to auto-fetch a real runtime.
        fatal "Target '${t}' is a STUB slot (no real backend yet). $(target_runtime_note "$t"). \
Install the vendor SDK out-of-band, then a future release fills this slot. Refusing to fake an install."
    fi
    if [ "$DRY_RUN" = true ]; then
        info "[dry-run] would acquire the ${t} runtime here."
        return 0
    fi
    case "$t" in
        ort-cpu)
            # Freely pullable. Mirrors the CI parko-onnx install (ONNX Runtime
            # matched to ort 2.0.0-rc.11 / ORT_API_VERSION 23 → v1.23.2).
            info "Acquire the Microsoft ONNX Runtime CPU build (v1.23.x) and export ORT_DYLIB_PATH."
            warn "Acquisition step is environment-specific; see INSTALL.md for the pinned steps." ;;
        openvino)
            info "Acquire the OpenVINO runtime (pip wheel >= 2025.1 or apt) and set LD_LIBRARY_PATH." ;;
        tensorrt)
            # Jetson-gated: NVIDIA ships TRT-enabled ORT via JetPack/L4T.
            warn "TensorRT runtime is JESTON-GATED: install NVIDIA's TensorRT-enabled ONNX Runtime"
            warn "from JetPack/L4T on the device. Cannot be pulled on a non-Jetson host." ;;
    esac
}

# ── build Parko with the right crate/feature ──────────────────────────────────
build_parko() {
    local t="$1"
    section "2/5 Build Parko — ${t}"
    local crate; crate="$(target_crate "$t")"
    info "Backend crate: ${crate} | cargo feature: $(target_feature "$t")"
    if [ "$DRY_RUN" = true ]; then
        info "[dry-run] would build the ${t} backend (crate ${crate})."
        return 0
    fi
    case "$t" in
        ort-cpu)  info "cargo build --release -p parko-onnx" ;;
        openvino) info "cargo build --release -p parko-openvino" ;;
        tensorrt) info "cargo build --release -p parko-tensorrt   # CI-buildable; real run Jetson-gated" ;;
    esac
}

# ── posture config per target ─────────────────────────────────────────────────
apply_posture() {
    local t="$1"
    section "3/5 Apply posture config — ${t}"
    info "Posture: $(target_posture "$t")"
    if [ "$DRY_RUN" = true ]; then
        info "[dry-run] would write the ${t} posture config."
    fi
}

# ── FAIL-CLOSED backend-load validation (generalizes error_on_failure) ─────────
# Every target must PROVE its runtime/EP loads, or the install fails loud. The
# probe is the backend crate's own load path (the TRT crate already fail-closes
# via .error_on_failure(); ORT/OV panic/err without their runtime). This hook is
# the framework's single chokepoint for "the backend actually loaded".
validate_backend_loads() {
    local t="$1"
    section "4/5 FAIL-CLOSED validation — ${t}"
    if [ "$DRY_RUN" = true ]; then
        info "[dry-run] would run the ${t} backend-load probe and REFUSE on failure"
        info "          (no silent substitution to another backend)."
        return 0
    fi
    # Real run: invoke the backend's load probe. Non-zero → refuse the install.
    # Contract: a probe exits 0 ONLY if the selected backend's runtime/EP loaded.
    case "$t" in
        ort-cpu|openvino|tensorrt)
            warn "Backend-load probe is the ${t} crate's own fail-closed load path."
            warn "On this host the probe will FAIL for a hardware-gated target (e.g."
            warn "tensorrt off-Jetson) — that is correct: refuse, never substitute."
            # A future release wires the concrete probe binary here. Until then,
            # a real (non-dry-run) install of a not-yet-probeable target refuses.
            fatal "Backend-load probe not yet wired for '${t}' in this scaffold — \
refusing to claim a validated install (fail-closed). Use --dry-run to exercise the framework." ;;
    esac
}

# ── common safety gates (chipset-independent; run for EVERY target) ───────────
gate() { # name, description
    if [ "$DRY_RUN" = true ]; then
        info "[dry-run] GATE: $1 — $2"
    else
        info "GATE: $1 — $2"
        # Real gates are refuse-to-proceed; wired against the deployed node.
    fi
}

run_common_safety_gates() {
    local t="$1"
    section "5/5 Common safety gates — chipset-independent, NON-skippable"
    gate "backend-load"      "the selected backend loaded fail-closed (step 4) — no silent substitution"
    gate "chokepoint"        "exactly ONE publisher on the motor command topic (no second writer)"
    gate "envelope-config"   "kinematic envelope + posture config present and parseable"
    gate "e-stop"            "emergency-stop path verified reachable and authoritative"
    gate "wheels-up smoke"   "an over-limit command is clamped/denied with the vehicle on stands"
    success "Common safety gates defined for ${t} (run as refuse-to-proceed steps on deploy)."
}

# ── main ──────────────────────────────────────────────────────────────────────
main() {
    select_target
    section "Parko backend install — target '${TARGET}' ($(target_descriptor "$TARGET"), $(target_kind "$TARGET"))"
    [ "$DRY_RUN" = true ] && warn "DRY-RUN: no runtime acquired, nothing built or installed."
    acquire_runtime          "$TARGET"
    build_parko              "$TARGET"
    apply_posture            "$TARGET"
    validate_backend_loads   "$TARGET"
    run_common_safety_gates  "$TARGET"
    success "Parko backend flow complete for '${TARGET}'."
    info "Gateway (kirra_verifier_service) is installed separately via install.sh — see INSTALL.md."
}

main
