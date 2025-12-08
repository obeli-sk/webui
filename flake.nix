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
            protobuf
            rustToolchain
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
          
          geminiSandboxedShell = pkgs.mkShell {
            packages = with pkgs; [
              gemini-cli
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
              echo "=========================================================="
              echo "Entering BUBBLEWRAP SANDBOX"
              echo "Gemini Version: $(gemini-cli --version)" 
              echo "=========================================================="
              
              CURRENT_DIR=$(pwd)
              
              # 1. SSL/Network Fixes
              export SSL_CERT_FILE="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
              export NIX_SSL_CERT_FILE=$SSL_CERT_FILE
              REAL_RESOLV=$(realpath /etc/resolv.conf)
              REAL_HOSTS=$(realpath /etc/hosts)

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
                
                # --- Project Mount ---
                --dir /workspace
                --bind "$CURRENT_DIR" /workspace
                --chdir /workspace
                
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
          devShells.geminiSandboxed = geminiSandboxedShell;
        }
      );
}
