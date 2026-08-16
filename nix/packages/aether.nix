{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  wrapGAppsHook3,
  glib,
  gtk3,
  libsoup_3,
  webkitgtk_4_1,
}:

stdenv.mkDerivation rec {
  pname = "aether";
  version = "4.29.0";

  src = fetchurl {
    url = "https://github.com/bjarneo/aether/releases/download/v${version}/aether-linux-amd64";
    hash = "sha256-u7VuwwBK5eg8/a+J0uKLcnwMqUpEwZQsXda/in8mEZc=";
  };

  dontUnpack = true;
  nativeBuildInputs = [
    autoPatchelfHook
    wrapGAppsHook3
  ];
  buildInputs = [
    glib
    gtk3
    libsoup_3
    webkitgtk_4_1
  ];

  installPhase = ''
    runHook preInstall
    install -Dm755 "$src" "$out/bin/aether"
    install -Dm644 /dev/stdin "$out/share/applications/aether.desktop" <<'EOF'
    [Desktop Entry]
    Type=Application
    Name=Aether
    GenericName=Theme Designer
    Comment=Create and inspect coordinated desktop color themes
    Exec=aether
    Icon=preferences-desktop-theme
    Terminal=false
    Categories=Settings;Utility;
    StartupNotify=true
    EOF
    runHook postInstall
  '';

  meta = {
    description = "Desktop color scheme and theming tool";
    homepage = "https://github.com/bjarneo/aether";
    license = lib.licenses.mit;
    platforms = [ "x86_64-linux" ];
    mainProgram = "aether";
  };
}
