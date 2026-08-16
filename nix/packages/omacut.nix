{
  lib,
  stdenv,
  fetchurl,
  ffmpeg,
  qt6,
}:

stdenv.mkDerivation rec {
  pname = "omacut";
  version = "0.4.0";

  src = fetchurl {
    name = "omacut-${version}.tar.gz";
    url = "https://api.github.com/repos/omacom-io/omacut/tarball/v${version}";
    hash = "sha256-jgnGqCXYlrPh2onpZDi2gaL32B6GV2D6cfTIqEzityI=";
  };

  sourceRoot = ".";
  nativeBuildInputs = [
    qt6.qmake
    qt6.wrapQtAppsHook
  ];
  buildInputs = [
    ffmpeg
    qt6.qtbase
    qt6.qtdeclarative
    qt6.qtmultimedia
    qt6.qtwayland
  ];

  postUnpack = ''
    sourceRoot="$(find . -mindepth 1 -maxdepth 1 -type d | head -n1)"
  '';
  installPhase = ''
    runHook preInstall
    install -Dm755 omacut "$out/bin/omacut"
    install -Dm644 pkgbuild/omacut.desktop "$out/share/applications/omacut.desktop"
    install -Dm644 pkgbuild/omacut.svg "$out/share/icons/hicolor/scalable/apps/omacut.svg"
    runHook postInstall
  '';

  meta = {
    description = "Focused video trimming application";
    homepage = "https://github.com/omacom-io/omacut";
    license = lib.licenses.mit;
    mainProgram = "omacut";
    platforms = lib.platforms.linux;
  };
}
