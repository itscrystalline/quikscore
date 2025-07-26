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
          LIB_DIR=$(readlink -f $LIB_DIR)
          export LD_LIBRARY_PATH=$LIB_DIR:$LD_LIBRARY_PATH
          exec "`dirname $0`/.quikscore-wrapped" "$@"
        '';

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
            makeWrapper
          ];

          env = {
            RUSTFLAGS = "-Z threads=8";
            OPENCV_LINK_PATHS = "${pkgs.opencv}/lib";
            OPENCV_LINK_LIBS = "opencv_core,opencv_imgproc,opencv_imgcodecs,png";
            OPENCV_INCLUDE_PATHS = "+${pkgs.opencv}/include";
          };

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
              echo "Rewriting lib path to system libc++"
              binary="$out/Applications/quikscore.app/Contents/MacOS/quikscore"

              echo "Before:"
              otool -L "$binary"

              install_name_tool -change \
                @loader_path/../Frameworks/libc++.1.0.dylib \
                /usr/lib/libc++.1.dylib \
                "$binary"

              echo "After:"
              otool -L "$binary"
            '';

          postInstall =
            if stdenv.hostPlatform.isLinux
            then ''
              echo "Bundling additional libraries (OpenCV, OpenBLAS, OpenEXR)"
              mkdir -p $out/lib
              for lib in core imgproc imgcodecs ; do
                cp "${pkgs.opencv}/lib/libopencv_$lib.so.411" "$out/lib/"
              done
              cp "${pkgs.openblas}/lib/libopenblas.so.0" "$out/lib/"
              echo "Bundling only needed OpenEXR libraries..."

              # Collect needed .so names from ldd output
              needed_libs=$(ldd "$out/bin/quikscore" \
                | awk '{print $1}' \
                | grep '\.so' \
                | awk '!seen[$0]++' \
                | xargs -n1 basename)

              echo "Used libraries:"
              echo "$needed_libs"

              for f in ${pkgs.openexr.out}/lib/*.so*; do
                base=$(basename "$f")
                for needed in $needed_libs; do
                  if [ "$base" = "$needed" ]; then
                    echo "Copying $base"
                    cp "$f" "$out/lib/"
                    break
                  fi
                done
              done
            ''
            else ''
              echo "Recursively bundling dylib dependencies"

              binary="$out/Applications/quikscore.app/Contents/MacOS/quikscore"
              frameworks="$out/Applications/quikscore.app/Contents/Frameworks"

              mkdir -p "$frameworks"

              # Initial set of dylibs you want to bundle
              initial_libs=(
                "${pkgs.opencv}/lib/libopencv_core.411.dylib"
                "${pkgs.opencv}/lib/libopencv_imgproc.411.dylib"
                "${pkgs.opencv}/lib/libopencv_imgcodecs.411.dylib"
                "${pkgs.libpng}/lib/libpng16.16.dylib"
                "${pkgs.libiconv}/lib/libiconv.2.dylib"
              )

              cp_and_relocate() {
                local dylib="$1"
                local name=$(basename "$dylib")

                # Only copy if not already present
                if [[ ! -f "$frameworks/$name" ]]; then
                  echo "Copying $name"
                  cp "$dylib" "$frameworks/"
                  chmod +w "$frameworks/$name" # install_name_tool requires write access
                  install_name_tool -id "@loader_path/../Frameworks/$name" "$frameworks/$name"

                  # Recursively process this dylib’s dependencies
                  local deps=$(otool -L "$frameworks/$name" | awk '{print $1}' | grep '^/nix/store')
                  for dep in $deps; do
                    cp_and_relocate "$dep"
                    install_name_tool -change "$dep" "@loader_path/../Frameworks/$(basename "$dep")" "$frameworks/$name"
                  done
                fi
              }

              # Start with the manually selected dylibs
              for lib in "''${initial_libs[@]}"; do
                cp_and_relocate "$lib"
              done

              # Relocate main binary’s dependencies
              for dep in $(otool -L "$binary" | awk '{print $1}' | grep '^/nix/store'); do
                name=$(basename "$dep")
                install_name_tool -change "$dep" "@loader_path/../Frameworks/$name" "$binary"
                cp_and_relocate "$dep"
              done
            '';
        });
      in {
        packages = rec {
          quikscore = package;
          default = quikscore;
        };
      }
    );
}
