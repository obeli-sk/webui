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
        in
        {
          devShells.noObelisk = noObeliskShell;
          devShells.withObelisk = withObeliskShell;
          devShells.default = noObeliskShell;
        }
      );
}
