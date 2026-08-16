{
  lib,
  stdenv,
  fetchurl,
  qt6,
}:

stdenv.mkDerivation rec {
  pname = "omawrite";
  version = "0.5.0";

  src = fetchurl {
    name = "omawrite-${version}.tar.gz";
    url = "https://api.github.com/repos/omacom-io/omawrite/tarball/v${version}";
    hash = "sha256-EelSBKRN5WYdBXk3SYzN+3cePNP6lkUMGIDYjNZJyuY=";
  };

  sourceRoot = ".";
  nativeBuildInputs = [
    qt6.qmake
    qt6.wrapQtAppsHook
  ];
  buildInputs = [
    qt6.qtbase
    qt6.qtdeclarative
    qt6.qtwayland
  ];

  postUnpack = ''
    sourceRoot="$(find . -mindepth 1 -maxdepth 1 -type d | head -n1)"
  '';
  installPhase = ''
    runHook preInstall
    install -Dm755 omawrite "$out/bin/omawrite"
    install -Dm644 pkgbuild/omawrite.desktop "$out/share/applications/omawrite.desktop"
    install -Dm644 pkgbuild/omawrite.svg "$out/share/icons/hicolor/scalable/apps/omawrite.svg"
    runHook postInstall
  '';

  meta = {
    description = "Focused Markdown writing application";
    homepage = "https://github.com/omacom-io/omawrite";
    license = lib.licenses.mit;
    mainProgram = "omawrite";
    platforms = lib.platforms.linux;
  };
}
