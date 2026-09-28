#!/usr/bin/env bash

# Rebuild WebUI and its serving component, then push the WASM component to Docker Hub.

set -exuo pipefail
cd "$(dirname "$0")/.."

TAG="$1"
OUTPUT_FILE="${2:-/dev/stdout}"

just build

if [ "$TAG" != "dry-run" ]; then
    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TMP_DIR"' EXIT
    WASM=target/wasm32-wasip2/release/webui_proxy.wasm
    REF="docker.io/getobelisk/webui:$TAG"

    cargo run --quiet --package webui-oci-config -- "$WASM" "$TMP_DIR/config.json"
    METADATA='{"component_type":"webhook_endpoint_wasm","env_vars":[],"allowed_hosts":[]}'
    DIGEST=$(oras push --no-tty --format 'go-template={{.digest}}' \
        --config "$TMP_DIR/config.json:application/vnd.wasm.config.v0+json" \
        --annotation "obelisk.component_metadata:0.2.0=$METADATA" \
        "$REF" "$WASM:application/wasm")
    printf 'oci://%s@%s' "$REF" "$DIGEST" > "$OUTPUT_FILE"
fi
