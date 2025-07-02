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
        inherit (pkgs) lib stdenv fetchYarnDeps fetchFromGitHub fetchurl;
        inherit (pkgs.rustPlatform) buildRustPackage;

        tesseract = fetchFromGitHub {
          owner = "tesseract-ocr";
          repo = "tesseract";
          rev = "de095fc074e64586e47c68c37a6be66a38bd8c4b";
          hash = "sha256-IQij/wt8tzNt3bJNWn6Ls+ZBe1bw2sf0PdD8HkoGIpA=";
        };
        leptonica = fetchFromGitHub {
          owner = "DanBloomberg";
          repo = "leptonica";
          rev = "0473e44d435c25d29b3f4ad082d5ec79e656b181";
          hash = "sha256-9EYldEnibJl2x61qsPMOeYQkwcbWtH91M4rPaXJOiLY=";
        };

        tessdata_eng = fetchurl {
          url = "https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/refs/heads/main/eng.traineddata";
          hash = "sha256-goCu0Hgv4nJXpo6hD+fvMkyg+Nhb0v0UXRwrVgvLZro=";
        };
        tessdata_tur = fetchurl {
          url = "https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/refs/heads/main/tur.traineddata";
          hash = "sha256-4MMzjcF1A9x9M1pQfJrgGytGz9B1YRceHhrFXYXo5Dg=";
        };

        package = buildRustPackage (finalAttrs: {
          pname = "quikscore";
          version = "0.1.0";

          src = ./.;

          preBuild = ''
            echo "Putting tesseract and leptonica sources into $HOME/.tesseract-rs/third_party..."
            mkdir -p $HOME/.tesseract-rs/third_party/tesseract
            mkdir -p $HOME/.tesseract-rs/third_party/leptonica
            cp -v -r ${tesseract}/* $HOME/.tesseract-rs/third_party/tesseract/
            cp -v -r ${leptonica}/* $HOME/.tesseract-rs/third_party/leptonica/
            echo "Putting English (eng) and Turkish (tur) tesseract OCR models into $HOME/.tesseract-rs/tessdata..."
            mkdir -p $HOME/.tesseract-rs/tessdata
            cp -v ${tessdata_eng} $HOME/.tesseract-rs/tessdata/eng.traineddata
            cp -v ${tessdata_tur} $HOME/.tesseract-rs/tessdata/tur.traineddata
            echo "Making $HOME/.tesseract-rs read-writable..."
            chmod -R +rw $HOME/.tesseract-rs
          '';

          yarnOfflineCache = fetchYarnDeps {
            yarnLock = finalAttrs.src + "/yarn.lock";
            hash = "sha256-287hUCyVI1o4D1iCLqBp42KHDT+bLmRyt3qrf8TN++A=";
          };

          nativeBuildInputs = with pkgs; [
            yarnConfigHook
            cargo-tauri.hook
            rustPlatform.bindgenHook
            writableTmpDirAsHomeHook

            nodejs
            pkg-config

            cmake
            clang
            sccache
          ];

          env = {
            OPENCV_LINK_PATHS = "+${pkgs.opencv}/lib";
            OPENCV_LINK_LIBS = "+opencv_core,opencv_calib3d,opencv_dnn,opencv_features2d,opencv_imgproc,opencv_video,opencv_flann,opencv_imgcodecs,opencv_objdetect,opencv_stitching,png";
            OPENCV_INCLUDE_PATHS = "+${pkgs.opencv}/include";
          };

          cargoRoot = "src-tauri";
          cargoLock = {
            lockFile = finalAttrs.src + "/${finalAttrs.cargoRoot}/Cargo.lock";
            outputHashes."tesseract-rs-0.1.19" = "sha256-+Qx1tss1GeV0rRLyC6HiNrJc47XiXQkwsjuTMoGLTGc=";
          };

          buildAndTestSubdir = "src-tauri";
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
