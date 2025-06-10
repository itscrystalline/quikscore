{
  pkgs,
  lib,
  config,
  inputs,
  ...
}: {
  # # https://devenv.sh/basics/
  # fix not opening on gtk
  env.WEBKIT_DISABLE_COMPOSITING_MODE = 1;
  env.LIBCLANG_PATH = "${pkgs.libclang.lib}/lib/";
  #
  # # https://devenv.sh/packages/
  packages = with pkgs; [
    pkg-config
    gobject-introspection
    cargo-tauri
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
    openssl
    libllvm
    libclang
    opencv

    cargo-tarpaulin
  ];
  #
  # # https://devenv.sh/languages/
  # opencv headers
  languages.cplusplus.enable = true;
  languages.rust = {
    enable = true;
    channel = "nightly";
    components = ["rustc" "cargo" "clippy" "rustfmt" "rust-analyzer"];
  };
  languages.javascript = {
    enable = true;
    yarn = {
      enable = true;
      install.enable = true;
    };
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
  # enterShell = ''
  #   hello
  #   git --version
  # '';
  #
  # # https://devenv.sh/tasks/
  tasks = {
    "quikscore:check".exec = "cd $DEVENV_ROOT/src-tauri; cargo check";
    "quikscore:lint".exec = "cd $DEVENV_ROOT/src-tauri; RUSTFLAGS=\"-Dwarnings\" cargo-clippy";
    "quikscore:test".exec = "cd $DEVENV_ROOT/src-tauri; cargo test";
    "quikscore:coverage".exec = "cd $DEVENV_ROOT/src-tauri; ${pkgs.cargo-tarpaulin}/bin/cargo-tarpaulin --color always --verbose --all-features --workspace --timeout 120 --out xml";
  };
  #
  # # https://devenv.sh/tests/
  enterTest = ''
    echo "Running tests"
    devenv tasks run quikscore:test
  '';
  #
  # # https://devenv.sh/git-hooks/
  # # git-hooks.hooks.shellcheck.enable = true;
  #
  # # See full reference at https://devenv.sh/reference/options/
}
