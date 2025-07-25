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

              echo "Cleaning bundled libs: remove those not needed…"
              cd $out/lib
              needed=$(ldd $out/bin/.quikscore-wrapped \
                | awk '{print $1}' \
                | grep '\.so' \
                | awk '!seen[$0]++')
              echo "Needed libs:"
              echo "$needed"
                for lib in *.so*; do
                base=$(basename "$lib")
                if ! grep -qx "$base" <<< "''${needed##*/}"; then
                  echo "Removing unused $lib"
                  rm "$lib"
                fi
              done
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
              cp -a "${pkgs.openexr.out}/lib/." "$out/lib/"
            ''
            else ''
              echo "Bundling additional dylibs (OpenCV, libpng, libiconv)"
              binary="$out/Applications/quikscore.app/Contents/MacOS/quikscore"
              frameworks="$out/Applications/quikscore.app/Contents/Frameworks"

              mkdir -p "$frameworks"
              for lib in core imgproc imgcodecs ; do
                cp "${pkgs.opencv}/lib/libopencv_$lib.dylib" $frameworks/
              done
              cp "${pkgs.libpng}/lib/libpng16.16.dylib" $frameworks/
              cp "${pkgs.libiconv}/lib/libiconv.2.dylib" $frameworks/

              for dylib in $out/Applications/quikscore.app/Contents/Frameworks/*.dylib; do
                install_name_tool -id "@loader_path/../Frameworks/$(basename "$dylib")" "$dylib"
              done

              for dep in $(otool -L $out/Applications/quikscore.app/Contents/MacOS/quikscore | grep "/nix/store" | awk '{print $1}'); do
                name=$(basename "$dep")
                install_name_tool -change "$dep" "@loader_path/../Frameworks/$name" $out/Applications/quikscore.app/Contents/MacOS/quikscore
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
