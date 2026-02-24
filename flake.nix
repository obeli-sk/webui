{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    obelisk = {
      url = "github:obeli-sk/obelisk/latest";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
        rust-overlay.follows = "rust-overlay";
      };
    };
  };
  outputs = { self, nixpkgs, flake-utils, rust-overlay, obelisk }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          overlays = [
            (import rust-overlay)
          ];

          pkgs = import nixpkgs {
            inherit system overlays;
          };
          rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          commonDeps = with pkgs; [
            binaryen # wasm-opt
            cargo-edit
            cargo-expand
            just
            protobuf
            rustToolchain
            dart-sass # SCSS compiler for styles
            trunk
            wasm-bindgen-cli
          ];
          withObelisk = commonDeps ++ [ obelisk.packages.${system}.default ];
          noObeliskShell = pkgs.mkShell {
            nativeBuildInputs = commonDeps;
          };
          withObeliskShell = pkgs.mkShell {
            nativeBuildInputs = withObelisk;
          };
          sandboxShell = pkgs.mkShell {
            packages = with pkgs; [
              codex
              gemini-cli
              claude-code
              bubblewrap
              # tools
              git
              curl
              wget
              htop
              zellij
              procps
              ripgrep
              which
              less
            ];
            shellHook = ''
              CURRENT_DIR=$(pwd)

              # SSL/Network Fixes
              export SSL_CERT_FILE="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
              export NIX_SSL_CERT_FILE=$SSL_CERT_FILE
              REAL_RESOLV=$(realpath /etc/resolv.conf)
              REAL_HOSTS=$(realpath /etc/hosts)

              # Construct mocked /run/current-system/sw/bin
              MOCKED_SYSTEM_BIN=$(mktemp -d)
              # Iterate through current PATH and symlink executables.
              # We use 'ln -s' without '-f' (force) so that the FIRST entry found
              # in the PATH (highest priority) wins, mimicking actual shell behavior.
              IFS=':' read -ra PATH_DIRS <<< "$PATH"
              for dir in "''${PATH_DIRS[@]}"; do
                if [ -d "$dir" ]; then
                   ln -s "$dir"/* "$MOCKED_SYSTEM_BIN/" 2>/dev/null || true
                fi
              done

              BWRAP_CMD=(
                ${pkgs.bubblewrap}/bin/bwrap
                --unshare-all
                --share-net
                --die-with-parent
                # --- Essential Binds ---
                --ro-bind /nix /nix
                --proc /proc
                --dev /dev
                --tmpfs /tmp
                # Tools need these to know "who" is running the process
                --ro-bind /etc/passwd /etc/passwd
                --ro-bind /etc/group /etc/group
                # --- Network ---
                --ro-bind "$REAL_RESOLV" /etc/resolv.conf
                --ro-bind "$REAL_HOSTS"  /etc/hosts
                # Claude
                --bind $HOME/.claude /tmp/.claude
                --bind $HOME/.claude.json /tmp/.claude.json
                # --- Project Mount ---
                --dir /workspace
                --bind "$CURRENT_DIR" /workspace
                --chdir /workspace
                # --- Mocked System Bin ---
                # Create the directory structure in the sandbox
                --dir /run/current-system/sw/bin
                # Bind our constructed temp folder to it
                --ro-bind "$MOCKED_SYSTEM_BIN" /run/current-system/sw/bin
                # --- Environment ---
                --setenv PS1 "[BWRAP] \w> "
                --setenv HOME /tmp
                --setenv TMPDIR /tmp
                --setenv TEMP /tmp
              )
              exec "''${BWRAP_CMD[@]}" ${pkgs.bashInteractive}/bin/bash -l +m
            '';
          };
        in
        {
          devShells.noObelisk = noObeliskShell;
          devShells.withObelisk = withObeliskShell;
          devShells.default = noObeliskShell;
          devShells.sandbox = sandboxShell;
        }
      );
}
