#!/usr/bin/env bash
# #146 guard: every parko member with a NON-OPTIONAL normal dep on a native
# runtime crate (ort/ort-sys/openvino/r2r) MUST be listed in
# ci/runtime-dependent-crates.txt, which the no-runtime gating lane excludes.
# Exact-match (also catches stale list entries). Fails loudly on drift.
set -euo pipefail
cd "$(dirname "$0")/.."   # -> parko/
LIST="ci/runtime-dependent-crates.txt"
MARKERS="ort|ort-sys|openvino|r2r"
declared=$(grep -vE '^[[:space:]]*(#|$)' "$LIST" | sort -u)
detected=$(cargo metadata --no-deps --format-version 1 \
  | jq -r --arg m "$MARKERS" '
      .packages[] as $p
      | $p.dependencies[]
      | select(.kind == null)          # normal deps
      | select(.optional == false)
      | select(.name | test("^(" + $m + ")$"))
      | $p.name' \
  | sort -u)
if [ "$declared" != "$detected" ]; then
  echo "ERROR (#146 runtime-isolation guard): list vs reality mismatch."
  echo "--- declared (ci/runtime-dependent-crates.txt) ---"; echo "$declared"
  echo "--- detected (crates with a non-optional native-runtime dep) ---"; echo "$detected"
  echo "Every crate that links a native inference/ROS runtime must be listed so"
  echo "the no-runtime gating lane excludes it; remove stale entries that no longer apply."
  exit 1
fi
echo "OK (#146): gating-lane exclude list matches runtime-dependent crates:"
echo "$detected"
