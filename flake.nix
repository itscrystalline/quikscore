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
        inherit (pkgs) lib stdenv yarn-berry_4;
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
          version = "0.2.0";

          src = ./.;

          missingHashes = ./missing-hashes.json;
          offlineCache = yarn-berry_4.fetchYarnBerryDeps {
            inherit (finalAttrs) src missingHashes;
            hash = builtins.readFile ./yarn-hash.txt;
          };

          nativeBuildInputs = with pkgs; [
            cargo-tauri.hook
            yarn-berry_4.yarnBerryConfigHook
            nightlyPlatform.bindgenHook

            nodejs
            yarn-berry_4
            pkg-config
            clang
            patchelf
          ];

          env = {
            RUSTFLAGS = "-Z threads=8";
            OPENCV_LINK_PATHS = "${pkgs.opencv}/lib";
            OPENCV_LINK_LIBS = "opencv_core,opencv_imgproc,opencv_imgcodecs,png,opencv_text";
            OPENCV_INCLUDE_PATHS = "+${pkgs.opencv}/include";
          };

          cargoRoot = "src-tauri";
          cargoLock = {
            lockFile = finalAttrs.src + "/${finalAttrs.cargoRoot}/Cargo.lock";
          };

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
              tesseract
              leptonica
            ]);

          postFixup =
            if stdenv.hostPlatform.isLinux
            then ''
              echo Patching ELF loader to a non-nix path...
              patchelf --set-interpreter ${loaderPath} $out/bin/quikscore

              echo "Libraries used:"
              ldd $out/bin/quikscore

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
              echo "Bundling additional libraries (OpenCV, OpenBLAS, OpenEXR, Tesseract, Leptonica)"
              mkdir -p $out/lib
              for lib in core imgproc imgcodecs; do
                cp "${pkgs.opencv}/lib/libopencv_$lib.so.411" "$out/lib/"
              done
              cp "${pkgs.openblas}/lib/libopenblas.so.0" "$out/lib/"
              cp "${pkgs.tesseract}/lib/libtesseract.so.5" "$out/lib/"
              cp "${pkgs.leptonica}/lib/libleptonica.so.6" "$out/lib/"
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
              echo "Bundling dylibs (OpenCV, libpng, libiconv)"
              binary="$out/Applications/quikscore.app/Contents/MacOS/quikscore"
              frameworks="$out/Applications/quikscore.app/Contents/Frameworks"

              mkdir -p "$frameworks"

              # 1. Copy required versioned dylibs unchanged
              for lib in core imgproc imgcodecs; do
                cp "${pkgs.opencv}/lib/libopencv_''${lib}.411.dylib" "$frameworks/"
                chmod +w "$frameworks/libopencv_''${lib}.411.dylib"
                install_name_tool -id "@loader_path/../Frameworks/libopencv_''${lib}.411.dylib" "$frameworks/libopencv_''${lib}.411.dylib"
              done
              cp "${pkgs.libpng}/lib/libpng16.16.dylib" "$frameworks/"
              chmod +w "$frameworks/libpng16.16.dylib"
              install_name_tool -id "@loader_path/../Frameworks/libpng16.16.dylib" "$frameworks/libpng16.16.dylib"
              cp "${pkgs.libiconv}/lib/libiconv.2.dylib" "$frameworks/"
              chmod +w "$frameworks/libiconv.2.dylib"
              install_name_tool -id "@loader_path/../Frameworks/libiconv.2.dylib" "$frameworks/libiconv.2.dylib"
              cp "${pkgs.tesseract}/lib/libtesseract.5.dylib" "$frameworks/"
              chmod +w "$frameworks/libtesseract.5.dylib"
              install_name_tool -id "@loader_path/../Frameworks/libtesseract.5.dylib" "$frameworks/libtesseract.5.dylib"
              cp "${pkgs.leptonica}/lib/libleptonica.6.dylib" "$frameworks/"
              chmod +w "$frameworks/libleptonica.6.dylib"
              install_name_tool -id "@loader_path/../Frameworks/libleptonica.6.dylib" "$frameworks/libleptonica.6.dylib"

              # 2. Recursively copy and patch Nix-store dependencies into Frameworks
              copy_deps() {
                local src="$1" dep base
                # Analyze dependencies of original src path
                deps=$(otool -L "$src" | awk 'NR>1 {print $1}' | grep '^/nix/store')
                for dep in $deps; do
                  base=$(basename "$dep")
                  if [[ ! -f "$frameworks/$base" ]]; then
                    echo "Copying dependency $base"
                    cp "$dep" "$frameworks/"
                    chmod +w "$frameworks/$base"
                    install_name_tool -id "@loader_path/../Frameworks/$base" "$frameworks/$base"
                    # Recurse on the original store path, not the copied file
                    copy_deps "$dep"
                  fi
                  # Patch the current copied library or binary
                  install_name_tool -change "$dep" "@loader_path/../Frameworks/$base" "$frameworks/$(basename "$src")"
                done
              }

              # Invoke on each initial copy
              for f in "$frameworks"/*.dylib; do
                copy_deps "${pkgs.opencv}/lib/$(basename "$f")" || true
              done

              # 3. Patch main binary to use local Frameworks versions
              for dep in $(otool -L "$binary" | awk 'NR>1 {print $1}' | grep '^/nix/store'); do
                depbase=$(basename "$dep")
                install_name_tool -change "$dep" "@loader_path/../Frameworks/$depbase" "$binary"
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
