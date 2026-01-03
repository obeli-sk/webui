#!/usr/bin/env bash

# Rebuild webui and webui-proxy, then push the proxy WASM component to Docker Hub.

set -exuo pipefail
cd "$(dirname "$0")/.."

which trunk # nix develop .#web --command

TAG="$1"
OUTPUT_FILE="${2:-/dev/stdout}"

just build

if [ "$TAG" != "dry-run" ]; then
    OUTPUT=$(obelisk client component push "target/wasm32-wasip2/release/webui_proxy.wasm" \
        "docker.io/getobelisk/webui:$TAG")
    echo -n $OUTPUT > $OUTPUT_FILE
fi
