#!/usr/bin/env bash

# Sync wasm-bindgen pinned versions in Cargo.toml and Trunk configs
# to match the wasm-bindgen-cli version provided by nix.
#
# Runs `wasm-bindgen --version` to get the CLI version, then fetches
# companion crate versions from the rustwasm/wasm-bindgen repo
# as referenced in the Cargo.toml comments.
#
# Must run BEFORE dev-deps.sh (which reads from the updated TOMLs).
#
# Updates: Cargo.toml, crates/webui/Trunk.toml, crates/webui/Trunk-dev.toml, Cargo.lock

set -euo pipefail
cd "$(dirname "$0")/.."

# --- Determine target version from wasm-bindgen CLI ---
NEW_VER=$(wasm-bindgen --version | awk '{print $2}')
if [[ -z "$NEW_VER" ]]; then
  echo "ERROR: Could not determine wasm-bindgen version from 'wasm-bindgen --version'" >&2
  exit 1
fi

# --- Read current version from Cargo.toml ---
OLD_VER=$(grep '^wasm-bindgen =' Cargo.toml | sed 's/.*"=\([^"]*\)".*/\1/')
if [[ -z "$OLD_VER" ]]; then
  echo "ERROR: Could not read current wasm-bindgen version from Cargo.toml" >&2
  exit 1
fi

# --- Sync wasm-bindgen and friends if the CLI version changed ---
if [[ "$OLD_VER" == "$NEW_VER" ]]; then
  echo "wasm-bindgen already at $NEW_VER, skipping."
else
  echo "Updating wasm-bindgen: $OLD_VER -> $NEW_VER"

  # --- Fetch companion versions from upstream Cargo.toml files ---
  BASE_URL="https://raw.githubusercontent.com/rustwasm/wasm-bindgen/$NEW_VER"

  fetch_version() {
    local crate_path="$1"
    local url="$BASE_URL/$crate_path/Cargo.toml"
    local ver
    ver=$(curl -sfL "$url" | grep '^version = ' | head -1 | sed 's/version = "\(.*\)"/\1/')
    if [[ -z "$ver" ]]; then
      echo "ERROR: Could not fetch version from $url" >&2
      exit 1
    fi
    echo "$ver"
  }

  FUTURES_VER=$(fetch_version "crates/futures")
  TEST_VER=$(fetch_version "crates/test")
  WEB_SYS_VER=$(fetch_version "crates/web-sys")
  JS_SYS_VER=$(fetch_version "crates/js-sys")

  echo "  wasm-bindgen-futures: $FUTURES_VER"
  echo "  wasm-bindgen-test:    $TEST_VER"
  echo "  web-sys:              $WEB_SYS_VER"
  echo "  js-sys:               $JS_SYS_VER"

  # --- Update Cargo.toml ---
  # wasm-bindgen = "=0.2.108" # Must be equal to wasm-bindgen-cli in nix. Update Trunk.toml.
  sed -i "s|^wasm-bindgen = \"=$OLD_VER\"|wasm-bindgen = \"=$NEW_VER\"|" Cargo.toml
  # Update the version references in comments
  sed -i "s|wasm-bindgen/blob/$OLD_VER/|wasm-bindgen/blob/$NEW_VER/|g" Cargo.toml

  # wasm-bindgen-futures = "=..."
  OLD_FUTURES_VER=$(grep '^wasm-bindgen-futures =' Cargo.toml | sed 's/.*"=\([^"]*\)".*/\1/')
  sed -i "s|^wasm-bindgen-futures = \"=$OLD_FUTURES_VER\"|wasm-bindgen-futures = \"=$FUTURES_VER\"|" Cargo.toml

  # wasm-bindgen-test = "=..."
  OLD_TEST_VER=$(grep '^wasm-bindgen-test =' Cargo.toml | sed 's/.*"=\([^"]*\)".*/\1/')
  sed -i "s|^wasm-bindgen-test = \"=$OLD_TEST_VER\"|wasm-bindgen-test = \"=$TEST_VER\"|" Cargo.toml

  # web-sys = { version = "=..." ...
  OLD_WEB_SYS_VER=$(grep '^web-sys =' Cargo.toml | sed 's/.*version = "=\([^"]*\)".*/\1/')
  sed -i "s|web-sys = { version = \"=$OLD_WEB_SYS_VER\"|web-sys = { version = \"=$WEB_SYS_VER\"|" Cargo.toml

  # js-sys = "=..."
  OLD_JS_SYS_VER=$(grep '^js-sys =' Cargo.toml | sed 's/.*"=\([^"]*\)".*/\1/')
  sed -i "s|^js-sys = \"=$OLD_JS_SYS_VER\"|js-sys = \"=$JS_SYS_VER\"|" Cargo.toml

  # --- Update Trunk configs ---
  for trunk_toml in crates/webui/Trunk.toml crates/webui/Trunk-dev.toml; do
    sed -i "s|^wasm_bindgen = \"$OLD_VER\"|wasm_bindgen = \"$NEW_VER\"|" "$trunk_toml"
  done
fi

# --- Sync wasm_opt in Trunk configs to match nix-provided wasm-opt (binaryen) ---
# `wasm-opt --version` prints e.g. "wasm-opt version 129"; Trunk pins it as "version_129".
WASM_OPT_VER=$(wasm-opt --version | awk '{print $3}')
if [[ -z "$WASM_OPT_VER" ]]; then
  echo "ERROR: Could not determine wasm-opt version from 'wasm-opt --version'" >&2
  exit 1
fi
NEW_WASM_OPT="version_${WASM_OPT_VER}"
echo "Syncing wasm_opt -> $NEW_WASM_OPT"
for trunk_toml in crates/webui/Trunk.toml crates/webui/Trunk-dev.toml; do
  OLD_WASM_OPT=$(grep '^wasm_opt = ' "$trunk_toml" | sed 's/wasm_opt = "\(.*\)"/\1/')
  if [[ "$OLD_WASM_OPT" != "$NEW_WASM_OPT" ]]; then
    sed -i "s|^wasm_opt = \"$OLD_WASM_OPT\"|wasm_opt = \"$NEW_WASM_OPT\"|" "$trunk_toml"
  fi
done

# --- Update Cargo.lock ---
echo "Running cargo update..."
cargo update 2>&1

echo "Done."
