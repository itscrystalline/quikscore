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
        libPathPatchScript = pkgs.writeScript "quikscore" ''
          #!/bin/sh
          LIB_DIR="$(dirname "$0")/../lib"
          LIBS=""
          for lib in core imgproc imgcodecs ; do
            LIBS=$LIB_DIR/libopencv_$lib.so.411:$LIBS
          done
          export LD_LIBRARY_PATH=$LIBS$LD_LIBRARY_PATH
          exec -a "$0" "`dirname $0`/.quikscore-wrapped" "$@"
        '';

        package = avx:
          nightlyPlatform.buildRustPackage (finalAttrs: {
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
              makeWrapper
            ];

            env = {
              RUSTFLAGS = "-Z threads=8";
              OPENCV_LINK_PATHS = "+${pkgs.opencv}/lib";
              OPENCV_LINK_LIBS = "+opencv_core,opencv_imgproc,opencv_imgcodecs,png";
              OPENCV_INCLUDE_PATHS = "+${pkgs.opencv}/include";
            };

            cargoRoot = "src-tauri";
            cargoLock = {
              lockFile = finalAttrs.src + "/${finalAttrs.cargoRoot}/Cargo.lock";
            };
            cargoBuildFeatures = lib.optionals avx ["avx512"];

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

            postFixup =
              if stdenv.hostPlatform.isLinux
              then ''
                echo Patching ELF loader to a non-nix path...
                patchelf --set-interpreter ${loaderPath} $out/bin/quikscore

                echo Adding wrapper script...
                mv $out/bin/quikscore $out/bin/.quikscore-wrapped
                cp ${libPathPatchScript} $out/bin/quikscore
                chmod +x $out/bin/quikscore
              ''
              else ''
                echo Patching libc++ dylib path...
                install_name_tool -change \
                  ${lib.makeLibraryPath [pkgs.libcxx]}/lib/libc++.1.dylib \
                  /usr/lib/libc++.1.dylib \
                  $out/Applications/quikscore.app/Contents/MacOS/quikscore
              '';

            postInstall =
              if stdenv.hostPlatform.isLinux
              then ''
                mkdir -p $out/lib
                for lib in core imgproc imgcodecs ; do
                  cp "${pkgs.opencv}/lib/libopencv_$lib.so.411" "$out/lib/"
                done
              ''
              else ''
                mkdir -p $out/Applications/quikscore.app/Contents/Frameworks
                for lib in core imgproc imgcodecs ; do
                  cp "${pkgs.opencv}/lib/libopencv_$lib.dylib" $out/Applications/quikscore.app/Contents/Frameworks/
                done

                for dylib in $out/Applications/quikscore.app/Contents/Frameworks/*.dylib; do
                  install_name_tool -id "@loader_path/../Frameworks/$(basename "$dylib")" "$dylib"
                done

                for dep in $(otool -L $out/Applications/quikscore.app/Contents/MacOS/quikscore | grep opencv | awk '{print $1}'); do
                  name=$(basename "$dep")
                  install_name_tool -change "$dep" "@loader_path/../Frameworks/$name" $out/Applications/quikscore.app/Contents/MacOS/quikscore
                done
              '';
          });
      in {
        packages = rec {
          quikscore = package true;
          quikscore-compat = package false;
          default = quikscore;
        };
      }
    );
}
