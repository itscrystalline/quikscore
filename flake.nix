{
  description = "Flake for quikscore";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = {
    self,
    nixpkgs,
    flake-utils,
    fenix,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {inherit system;};
        inherit (pkgs) lib stdenv fetchYarnDeps;
        nightlyPlatform = pkgs.makeRustPlatform {inherit (fenix.packages.${system}.minimal) cargo rustc;};

        package = nightlyPlatform.buildRustPackage (finalAttrs: {
          pname = "quikscore";
          version = "0.1.0";

          src = ./.;

          yarnOfflineCache = fetchYarnDeps {
            yarnLock = finalAttrs.src + "/yarn.lock";
            hash = "sha256-287hUCyVI1o4D1iCLqBp42KHDT+bLmRyt3qrf8TN++A=";
          };

          nativeBuildInputs = with pkgs; [
            yarnConfigHook
            cargo-tauri.hook
            nightlyPlatform.bindgenHook

            nodejs
            pkg-config
            clang
          ];

          RUSTFLAGS = "-Z threads=8";
          OPENCV_LINK_PATHS = "+${pkgs.opencv}/lib";
          OPENCV_LINK_LIBS = "+opencv_core,opencv_calib3d,opencv_dnn,opencv_features2d,opencv_imgproc,opencv_video,opencv_flann,opencv_imgcodecs,opencv_objdetect,opencv_stitching,png";
          OPENCV_INCLUDE_PATHS = "+${pkgs.opencv}/include";

          cargoRoot = "src-tauri";
          cargoLock = {
            lockFile = finalAttrs.src + "/${finalAttrs.cargoRoot}/Cargo.lock";
          };
          cargoBuildFeatures = ["avx512"];

          buildAndTestSubdir = "src-tauri";
          # useNextest = true;
          doCheck = false;

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
        });
      in {
        packages = {
          quikscore = package;
          default = package;
        };
      }
    );
}
