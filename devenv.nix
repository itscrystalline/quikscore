{
  pkgs,
  lib,
  config,
  inputs,
  ...
}: let
  libs = with pkgs; [
    opencv
    libclang.lib
  ];
  lib-path = lib.makeLibraryPath libs;
in {
  # # https://devenv.sh/basics/
  # fix not opening on gtk
  env.WEBKIT_DISABLE_COMPOSITING_MODE = 1;
  env.LIBCLANG_PATH = "${pkgs.libclang.lib}/lib/";
  env.RUST_LOG_STYLE = "always";
  env.RUST_LOG = "debug";
  #
  # # https://devenv.sh/packages/
  packages = with pkgs;
    [
      pkg-config
      cargo-tauri
      cargo-nextest
      openssl
      libllvm
      libclang
      opencv
      sccache

      cargo-tarpaulin
      bacon

      # cd testing
      act

      # yarn hash
      yarn-berry_4.yarn-berry-fetcher
    ]
    ++ lib.optionals pkgs.stdenv.isLinux [
      gobject-introspection
      at-spi2-atk
      atkmm
      cairo
      gdk-pixbuf
      glib
      gtk3
      harfbuzz
      librsvg
      libsoup_3
      pango
      webkitgtk_4_1
    ];
  #
  # # https://devenv.sh/languages/
  # opencv headers
  languages.cplusplus.enable = true;
  languages.rust = {
    enable = true;
    channel = "nightly";
    version = "2025-06-08";
    rustflags = "-Z threads=8";
    components = ["rustc" "cargo" "clippy" "rustfmt" "rust-analyzer"];
  };
  languages.javascript = {
    enable = true;
    corepack.enable = true;
  };

  #
  # # https://devenv.sh/processes/
  # # processes.cargo-watch.exec = "cargo-watch";
  #
  # # https://devenv.sh/services/
  # # services.postgres.enable = true;
  #
  # # https://devenv.sh/scripts/
  # scripts.hello.exec = ''
  #   echo hello from $GREET
  # '';
  #
  enterShell = ''
    export LD_LIBRARY_PATH=${lib-path}:$LD_LIBRARY_PATH
  '';
  #
  # # https://devenv.sh/tasks/
  tasks = {
    "quikscore:check".exec = "cd $DEVENV_ROOT/src-tauri; cargo check";
    "quikscore:lint".exec = "cd $DEVENV_ROOT/src-tauri; RUSTFLAGS=\"-Dwarnings\" cargo-clippy";
    "quikscore:test-full".exec = "cd $DEVENV_ROOT/src-tauri; cargo nextest run --features ocr-tests";
    "quikscore:coverage".exec = "cd $DEVENV_ROOT/src-tauri; ${pkgs.cargo-tarpaulin}/bin/cargo-tarpaulin --color always --verbose --features ocr-tests --workspace --timeout 120 --out xml --no-dead-code --engine llvm --release";
  };
  #
  # # https://devenv.sh/tests/
  enterTest = ''
    echo "Running tests"
    cd $DEVENV_ROOT/src-tauri
    cargo nextest run
  '';
  #
  # # https://devenv.sh/git-hooks/
  # # git-hooks.hooks.shellcheck.enable = true;
  #
  # # See full reference at https://devenv.sh/reference/options/
}
