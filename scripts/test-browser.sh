#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
export WASM_BINDGEN_TEST_ONLY_WEB=1
cargo test --locked -p webui --target wasm32-unknown-unknown "$@"
