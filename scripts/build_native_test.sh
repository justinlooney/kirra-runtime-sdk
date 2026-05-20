#!/usr/bin/env bash
# Build and run the Aegis native C++ FFI integration test.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AEGIS_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "Building Aegis shared library..."
cargo build --release --manifest-path "${AEGIS_DIR}/Cargo.toml"

LIB_PATH="${AEGIS_DIR}/target/release"
INCLUDE_PATH="${AEGIS_DIR}/include"
TEST_SRC="${AEGIS_DIR}/tests/native_test.cpp"
TEST_BIN="${AEGIS_DIR}/target/native_test"

echo "Compiling native test..."
g++ -std=c++17 -o "${TEST_BIN}" "${TEST_SRC}" \
    -I "${INCLUDE_PATH}" \
    -L "${LIB_PATH}" \
    -laegis_runtime_sdk \
    -Wl,-rpath,"${LIB_PATH}"

echo "Running native test..."
"${TEST_BIN}"
echo "Native test passed."
