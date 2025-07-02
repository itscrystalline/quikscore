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

        loaderPath =
          if stdenv.isx86_64
          then "/lib64/ld-linux-x86-64.so.2"
          else "/lib/ld-linux-aarch64.so.1";

        package = buildRustPackage (finalAttrs: {
          pname = "quikscore";
          version = "0.1.0";

          src = ./.;

          yarnOfflineCache = fetchYarnDeps {
            yarnLock = finalAttrs.src + "/yarn.lock";
            hash = "sha256-fBrclUcHHLgviE6X6Os5zewuI4vLauz5N52N8jc2FQ0=";
          };

          nativeBuildInputs = with pkgs; [
            yarnConfigHook
            nodejs
            cargo-tauri.hook
            rustPlatform.bindgenHook
            pkg-config
            clang
            patchelf
          ];

          # buildEnv = {
          #   LIBCLANG_PATH = "${pkgs.libclang}/lib";
          #   CPLUS_INCLUDE_PATH = "${pkgs.llvmPackages.libcxx.dev}/include/c++";
          # };
          env = {
            OPENCV_LINK_PATHS = "+${pkgs.opencv}/lib";
            OPENCV_LINK_LIBS = "+opencv_core,opencv_imgproc,opencv_imgcodecs,png";
            OPENCV_INCLUDE_PATHS = "+${pkgs.opencv}/include";
          };

          cargoRoot = "src-tauri";
          cargoLock = {
            lockFile = finalAttrs.src + "/${finalAttrs.cargoRoot}/Cargo.lock";
          };

          buildAndTestSubdir = "src-tauri";
          useNextest = true;

          buildInputs = with pkgs; (lib.optionals stdenv.hostPlatform.isLinux [
              glib
              gtk3
              openssl
              webkitgtk_4_1
            ]
            ++ [
              opencv
              libpng
              openssl
            ]);

          postFixup = lib.optionalString stdenv.hostPlatform.isLinux ''
            echo Patching ELF loader to a non-nix path...
            patchelf --set-interpreter ${loaderPath} $out/bin/quikscore
          '';
        });
      in {
        packages = {
          quikscore = package;
          default = package;
        };
      }
    );
}
