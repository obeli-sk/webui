#!/usr/bin/env bash

# Collect versions of binaries installed by `nix develop` producing file `dev-deps.txt`.
# This script should be executed after every `nix flake update`.

set -exuo pipefail
cd "$(dirname "$0")/.."


get_litestream_version() {
  WORKSPACE_DIR="$(pwd -P)"
  TMP_DIR=$(mktemp -d)
  (
    cd "$TMP_DIR" || exit 1
    touch obelisk obelisk.toml

    docker build -f "$WORKSPACE_DIR/.github/workflows/release/docker-image/ubuntu-24.04-litestream.Dockerfile" . --tag temp >/dev/null
    docker run --rm --entrypoint litestream temp version
    docker rmi temp >/dev/null
  )
  rm -rf "$TMP_DIR"
}

rm -f dev-deps.txt
cargo upgrade --version >> dev-deps.txt
cargo-expand --version >> dev-deps.txt
just --version >> dev-deps.txt
nix develop .#withObelisk --command obelisk --version >> dev-deps.txt
protoc --version >> dev-deps.txt
rustc --version >> dev-deps.txt

sass --version >> dev-deps.txt # dart-sass for SCSS compilation
wasm-opt --version >> dev-deps.txt # binaryen
echo "trunk $(grep wasm_opt crates/webui/Trunk.toml)" >> dev-deps.txt
trunk --version >> dev-deps.txt
wasm-bindgen --version >> dev-deps.txt
echo "trunk $(grep wasm_bindgen crates/webui/Trunk.toml)" >> dev-deps.txt
echo "cargo $(grep 'wasm-bindgen =' Cargo.toml)" >> dev-deps.txt
