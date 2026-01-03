clean:
    cargo clean
    rm -rf crates/webui/dist
    rm -rf crates/webui/dist-dev
    rm -f crates/webui/blueprint.css crates/webui/syntect.css

serve:
    cargo build --package webui --target=wasm32-unknown-unknown # Trunk fails to run this before needing CSS files
    trunk --log=debug serve --config crates/webui/Trunk-dev.toml

build:
    rm -rf crates/webui/dist
    cargo build --package webui --release --target=wasm32-unknown-unknown # Trunk fails to run this before needing CSS files
    trunk --log=debug --offline=true build --config crates/webui/Trunk.toml
    cargo build --package webui-proxy --target=wasm32-wasip2 --release
