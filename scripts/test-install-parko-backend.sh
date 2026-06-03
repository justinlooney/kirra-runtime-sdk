#!/usr/bin/env bash
#
# test-install-parko-backend.sh — framework self-test for the target-parameterized
# Parko backend installer. PURE SHELL, no hardware, no root, no network — runnable
# anywhere (sandbox + CI). It asserts the FRAMEWORK behavior (dispatch, selection,
# stub honesty, fail-closed refusals, non-skippable gates), NOT real installs.
#
# What it does NOT cover (hardware-gated, by design): real runtime acquisition,
# real backend-load probes, on-silicon validation. Those are per-target and gated.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="${SCRIPT_DIR}/install-parko-backend.sh"

pass=0; fail=0
ok()   { echo "  ok   - $1"; pass=$((pass+1)); }
bad()  { echo "  FAIL - $1"; fail=$((fail+1)); }

# Run the installer; capture combined output + exit code.
run() { bash "$INSTALLER" "$@" >/tmp/parko_install_test.out 2>&1; echo $?; }
out() { cat /tmp/parko_install_test.out; }

echo "== install-parko-backend.sh framework self-test =="

# 1. --list and --help succeed and name all six targets.
rc=$(run --list)
if [ "$rc" = "0" ] && out | grep -q "ort-cpu" && out | grep -q "amd-vitis"; then
    ok "--list prints the full target matrix"
else bad "--list should print all targets and exit 0 (rc=$rc)"; fi

# 2. Real targets (ort-cpu, openvino) dry-run end to end (exit 0) and reach gates.
for t in ort-cpu openvino; do
    rc=$(run --target "$t" --dry-run --non-interactive)
    if [ "$rc" = "0" ] && out | grep -q "Common safety gates"; then
        ok "real target '$t' dry-runs through all 5 steps + safety gates"
    else bad "real target '$t' should dry-run to completion (rc=$rc)"; fi
done

# 3. tensorrt (scaffold) dry-runs (logic present) and flags Jetson-gated.
rc=$(run --target tensorrt --dry-run --non-interactive)
if [ "$rc" = "0" ] && out | grep -qi "jetson"; then
    ok "scaffold target 'tensorrt' dry-runs and flags Jetson-gating"
else bad "tensorrt should dry-run and flag Jetson-gating (rc=$rc)"; fi

# 4. STUB targets REFUSE (exit non-zero) and state the vendor-SDK gating — even
#    in dry-run, because there is nothing real to install.
for t in qnn ti-tidl amd-vitis; do
    rc=$(run --target "$t" --dry-run --non-interactive)
    if [ "$rc" != "0" ] && out | grep -qi "requires" && out | grep -qi "SDK"; then
        ok "stub target '$t' refuses honestly (requires vendor SDK)"
    else bad "stub target '$t' must refuse + name the vendor SDK (rc=$rc)"; fi
done

# 5. Unknown target is refused.
rc=$(run --target bogus --dry-run --non-interactive)
if [ "$rc" != "0" ] && out | grep -qi "Unknown target"; then
    ok "unknown target is refused"
else bad "unknown target must be refused (rc=$rc)"; fi

# 6. Auto-detect WITHOUT confirmation refuses to proceed (no silent guess).
rc=$(run --auto-detect --non-interactive --dry-run)
if [ "$rc" != "0" ] && out | grep -qi "requires explicit confirmation"; then
    ok "auto-detect alone refuses (requires explicit confirmation)"
else bad "auto-detect without --confirm must refuse (rc=$rc)"; fi

# 7. Auto-detect WITH --confirm proceeds (suggestion accepted explicitly).
rc=$(run --auto-detect --confirm --non-interactive --dry-run)
if [ "$rc" = "0" ] && out | grep -qi "Accepted via --confirm"; then
    ok "auto-detect + --confirm proceeds"
else bad "auto-detect + --confirm should proceed (rc=$rc)"; fi

# 8. No --skip-safety-gates escape hatch exists (it's an unknown arg → refused).
rc=$(run --target ort-cpu --skip-safety-gates --dry-run)
if [ "$rc" != "0" ]; then
    ok "no --skip-safety-gates bypass (gates are non-skippable)"
else bad "a safety-gate bypass flag must not be accepted (rc=$rc)"; fi

# 9. A REAL (non-dry-run) install of a not-yet-probeable target fails closed
#    rather than claiming a validated install.
rc=$(run --target ort-cpu --non-interactive)
if [ "$rc" != "0" ] && out | grep -qi "fail-closed"; then
    ok "real install without a wired probe fails closed (no false 'validated')"
else bad "real install must fail closed when the load probe isn't wired (rc=$rc)"; fi

echo ""
echo "== results: ${pass} passed, ${fail} failed =="
[ "$fail" -eq 0 ]
