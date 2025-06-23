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
          ];

          # buildEnv = {
          #   LIBCLANG_PATH = "${pkgs.libclang}/lib";
          #   CPLUS_INCLUDE_PATH = "${pkgs.llvmPackages.libcxx.dev}/include/c++";
          # };
          env = {
            OPENCV_LINK_PATHS = "+${pkgs.opencv}/lib";
            OPENCV_LINK_LIBS = "+opencv_core,opencv_calib3d,opencv_dnn,opencv_features2d,opencv_imgproc,opencv_video,opencv_flann,opencv_imgcodecs,opencv_objdetect,opencv_stitching,png";
            OPENCV_INCLUDE_PATHS = "+${pkgs.opencv}/include";
          };

          cargoRoot = "src-tauri";
          cargoLock = {
            lockFile = finalAttrs.src + "/${finalAttrs.cargoRoot}/Cargo.lock";
          };

          buildAndTestSubdir = "src-tauri";

          buildInputs = with pkgs; (lib.optionals stdenv.hostPlatform.isLinux [
              glib
              gtk3
              openssl
              webkitgtk_4_1
            ]
            ++ [opencv]);
        });
      in {
        packages = {
          quikscore = package;
          default = package;
        };
      }
    );
}
