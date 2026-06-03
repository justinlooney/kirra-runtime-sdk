#!/usr/bin/env bash
#
# test-install-parko-backend.sh — framework self-test for the full-stack,
# target-parameterized installer (Kirra + Occy + Parko). PURE SHELL: no hardware,
# root, network — runs anywhere (sandbox + CI). It asserts FRAMEWORK behavior
# (dispatch, full-stack composition, the two-dimension readiness model, the
# operator-supplied-SDK path, fail-closed refusals, non-skippable gates), NOT
# real installs or on-silicon validation (those are externally gated).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="${SCRIPT_DIR}/install-parko-backend.sh"

pass=0; fail=0
ok()  { echo "  ok   - $1"; pass=$((pass+1)); }
bad() { echo "  FAIL - $1"; fail=$((fail+1)); }
run() { bash "$INSTALLER" "$@" >/tmp/parko_install_test.out 2>&1; echo $?; }
out() { cat /tmp/parko_install_test.out; }

echo "== install-parko-backend.sh full-stack framework self-test =="

# 1. --list and --readiness succeed and cover all six targets honestly.
rc=$(run --list)
if [ "$rc" = "0" ] && out | grep -q "ort-cpu" && out | grep -q "amd-vitis"; then
    ok "--list prints the full target matrix"
else bad "--list should list all targets (rc=$rc)"; fi

rc=$(run --readiness)
if [ "$rc" = "0" ] && out | grep -q "READY" && out | grep -q "stub:PARK-027" \
   && out | grep -qi "PATH ready, backend code"; then
    ok "--readiness shows both dimensions honestly (path READY all; backend code distinct)"
else bad "--readiness must distinguish install-path (ready) from backend-code (rc=$rc)"; fi

# 2. Real targets dry-run the FULL stack (Kirra + Occy + Parko) to the gates.
for t in ort-cpu openvino; do
    rc=$(run --target "$t" --dry-run --non-interactive)
    if [ "$rc" = "0" ] && out | grep -q "KIRRA gateway" && out | grep -q "OCCY" \
       && out | grep -q "Common safety gates"; then
        ok "real target '$t' dry-runs the composed Kirra+Occy+Parko stack to the gates"
    else bad "real target '$t' should compose the full stack in dry-run (rc=$rc)"; fi
done

# 3. --parko-only skips the Kirra + Occy composition.
rc=$(run --target ort-cpu --parko-only --dry-run --non-interactive)
if [ "$rc" = "0" ] && out | grep -q "Skipping Kirra" && out | grep -q "Skipping Occy"; then
    ok "--parko-only skips Kirra + Occy composition"
else bad "--parko-only should skip Kirra + Occy (rc=$rc)"; fi

# 4. tensorrt (scaffold) dry-runs and flags Jetson-gating + scaffold backend code.
rc=$(run --target tensorrt --dry-run --non-interactive)
if [ "$rc" = "0" ] && out | grep -qi "jetson" && out | grep -q "scaffold"; then
    ok "scaffold target 'tensorrt' dry-runs; flags Jetson + scaffold backend"
else bad "tensorrt should dry-run and flag Jetson + scaffold (rc=$rc)"; fi

# 5. Vendor target WITHOUT --sdk-path: refuse, but as PATH-waiting-on-external
#    (operator must supply the licensed artifact) — NOT a missing install.
for t in qnn ti-tidl amd-vitis; do
    rc=$(run --target "$t" --dry-run --non-interactive)
    if [ "$rc" != "0" ] && out | grep -q -- "--sdk-path" && out | grep -qi "operator-supplied"; then
        ok "vendor target '$t' without SDK refuses as path-waiting-on-external (--sdk-path)"
    else bad "vendor target '$t' must ask for --sdk-path, not imply a missing install (rc=$rc)"; fi
done

# 6. Vendor target WITH --sdk-path dry-runs the authored PATH to completion —
#    proving the install PATH is READY (the deliverable), backend code aside.
rc=$(run --target qnn --sdk-path /tmp/fake-qnn-sdk --dry-run --non-interactive)
if [ "$rc" = "0" ] && out | grep -q "Common safety gates"; then
    ok "vendor target 'qnn' + --sdk-path dry-runs the authored path end to end"
else bad "vendor target 'qnn' + --sdk-path should dry-run to completion (rc=$rc)"; fi

# 7. Unknown target refused.
rc=$(run --target bogus --dry-run --non-interactive)
if [ "$rc" != "0" ] && out | grep -qi "Unknown target"; then
    ok "unknown target is refused"
else bad "unknown target must be refused (rc=$rc)"; fi

# 8. Auto-detect alone refuses; with --confirm proceeds.
rc=$(run --auto-detect --non-interactive --dry-run)
if [ "$rc" != "0" ] && out | grep -qi "requires explicit confirmation"; then
    ok "auto-detect alone refuses (explicit confirmation required)"
else bad "auto-detect without --confirm must refuse (rc=$rc)"; fi
rc=$(run --auto-detect --confirm --non-interactive --dry-run)
if [ "$rc" = "0" ] && out | grep -qi "Accepted via --confirm"; then
    ok "auto-detect + --confirm proceeds"
else bad "auto-detect + --confirm should proceed (rc=$rc)"; fi

# 9. No safety-gate bypass exists.
rc=$(run --target ort-cpu --skip-safety-gates --dry-run)
if [ "$rc" != "0" ]; then
    ok "no --skip-safety-gates bypass (gates non-skippable)"
else bad "a safety-gate bypass flag must not be accepted (rc=$rc)"; fi

# 10. Real (non-dry-run) install of a STUB-backend target, with an EXISTING
#     operator SDK artifact, fails closed at FINAL validation with the explicit
#     code-gate boundary (path ran end to end; backend code is the remaining gate).
SDK_FIXTURE="$(mktemp -d)/qnn-sdk-artifact"; : > "$SDK_FIXTURE"
rc=$(run --target qnn --sdk-path "$SDK_FIXTURE" --non-interactive)
if [ "$rc" != "0" ] && out | grep -qi "remaining CODE gate" && out | grep -qi "fail-closed"; then
    ok "real install of stub-backend target fails closed at FINAL validation (code-gate boundary)"
else bad "stub-backend real install must defer FINAL validation as the code gate (rc=$rc)"; fi

# 11. Real install of a done/scaffold target without a wired probe fails closed
#     (no false 'validated' — runtime presence unverified).
rc=$(run --target ort-cpu --non-interactive)
if [ "$rc" != "0" ] && out | grep -qi "fail-closed" && out | grep -qi "probe"; then
    ok "real install without a wired load probe fails closed (no false 'validated')"
else bad "real install must fail closed when the load probe isn't wired (rc=$rc)"; fi

echo ""
echo "== results: ${pass} passed, ${fail} failed =="
[ "$fail" -eq 0 ]
