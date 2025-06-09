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
  ];
  #
  # # https://devenv.sh/languages/
  languages.rust = {
    enable = true;
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
  # # tasks = {
  # #   "myproj:setup".exec = "mytool build";
  # #   "devenv:enterShell".after = [ "myproj:setup" ];
  # # };
  #
  # # https://devenv.sh/tests/
  # enterTest = ''
  #   echo "Running tests"
  #   git --version | grep --color=auto "${pkgs.git.version}"
  # '';
  #
  # # https://devenv.sh/git-hooks/
  # # git-hooks.hooks.shellcheck.enable = true;
  #
  # # See full reference at https://devenv.sh/reference/options/
}
