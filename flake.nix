{
  description = "Flake for quikscore";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = {
    self,
    nixpkgs,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {inherit system;};
        inherit (pkgs) lib stdenv fetchYarnDeps;
        inherit (pkgs.rustPlatform) buildRustPackage;
        package = buildRustPackage (finalAttrs: rec {
          pname = "quikscore";
          version = "0.1.0";

          src = ./.;

          yarnOfflineCache = fetchYarnDeps {
            yarnLock = finalAttrs.src + "/yarn.lock";
            hash = "sha256-fBrclUcHHLgviE6X6Os5zewuI4vLauz5N52N8jc2FQ0=";
          };

          nativeBuildInputs = with pkgs; [
            yarnConfigHook
            yarnBuildHook
            yarnInstallHook
            nodejs
            cargo-tauri.hook
            rustPlatform.bindgenHook
            pkg-config
            libclang
            libllvm
          ];

          LIBCLANG_PATH = "${pkgs.libclang}/lib";

          cargoRoot = "src-tauri";
          cargoLock = {
            lockFile = src + "/${cargoRoot}/Cargo.lock";
          };

          buildAndTestSubdir = "src-tauri";

          buildInputs = lib.optionals stdenv.hostPlatform.isLinux (with pkgs; [
            glib
            gtk3
            openssl
            webkitgtk_4_1
            opencv
          ]);
        });
      in {
        packages = {
          quikscore = package;
          default = package;
        };
      }
    );
}
