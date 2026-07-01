#!/usr/bin/env bash

# Rebuild webui and webui-proxy, then push the proxy WASM component to Docker Hub.

set -exuo pipefail
cd "$(dirname "$0")/.."

TAG="$1"
OUTPUT_FILE="${2:-/dev/stdout}"

just build

if [ "$TAG" != "dry-run" ]; then
    TMP_TOML= deployment-for-push.toml
    OUTPUT=$(obelisk component push --deployment "$TMP_TOML" \
        webui_proxy "oci://docker.io/getobelisk/webui:$TAG")
    echo -n $OUTPUT > $OUTPUT_FILE
fi
