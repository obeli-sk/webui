#!/usr/bin/env bash

# Rebuild webui and webui-proxy, then push the proxy WASM component to Docker Hub.

set -exuo pipefail
cd "$(dirname "$0")/.."

which trunk # nix develop .#web --command

TAG="$1"
OUTPUT_FILE="${2:-/dev/stdout}"

just build

if [ "$TAG" != "dry-run" ]; then
    TMP_TOML=$(mktemp -t webui-deployment-XXXXXX.toml)
    trap "rm -f $TMP_TOML" EXIT
    cat > "$TMP_TOML" <<EOF
[[webhook_endpoint_wasm]]
name = "webui_proxy"
location = "$(pwd)/target/wasm32-wasip2/release/webui_proxy.wasm"
routes = [{ methods = ["GET"], route = "/" }]
EOF
    OUTPUT=$(obelisk component push --deployment "$TMP_TOML" \
        webui_proxy "oci://docker.io/getobelisk/webui:$TAG")
    echo -n $OUTPUT > $OUTPUT_FILE
fi
