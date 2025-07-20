{
  description = "Flake for quikscore";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    oxalica = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = {
    self,
    nixpkgs,
    flake-utils,
    oxalica,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import oxalica)];
        pkgs = import nixpkgs {inherit system overlays;};
        inherit (pkgs) lib stdenv fetchYarnDeps;
        nightlyVersion = "2025-06-08";
        nightlyPlatform = pkgs.makeRustPlatform {
          cargo = pkgs.rust-bin.nightly.${nightlyVersion}.minimal;
          rustc = pkgs.rust-bin.nightly.${nightlyVersion}.minimal;
        };

        loaderPath =
          if stdenv.isx86_64
          then "/lib64/ld-linux-x86-64.so.2"
          else "/lib/ld-linux-aarch64.so.1";

        package = nightlyPlatform.buildRustPackage (finalAttrs: {
          pname = "quikscore";
          version = "0.1.0";

          src = ./.;

          yarnOfflineCache = fetchYarnDeps {
            yarnLock = finalAttrs.src + "/yarn.lock";
            hash = "sha256-bnSADccuQUUuvQE7TnQgEujfUGJF0BXdWzH4ZKxy+OM=";
          };

          nativeBuildInputs = with pkgs; [
            yarnConfigHook
            cargo-tauri.hook
            nightlyPlatform.bindgenHook

            nodejs
            pkg-config
            clang
            patchelf
          ];

          env = {
            RUSTFLAGS = "-Z threads=8";
            OPENCV_LINK_PATHS = "+${pkgs.opencv}/lib";
            OPENCV_LINK_LIBS = "+opencv_core,opencv_calib3d,opencv_dnn,opencv_features2d,opencv_imgproc,opencv_video,opencv_flann,opencv_imgcodecs,opencv_objdetect,opencv_stitching,png";
            OPENCV_INCLUDE_PATHS = "+${pkgs.opencv}/include";
          };

          # Optional: uncomment if needed
          # buildEnv = {
          #   LIBCLANG_PATH = "${pkgs.libclang}/lib";
          #   CPLUS_INCLUDE_PATH = "${pkgs.llvmPackages.libcxx.dev}/include/c++";
          # };

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
