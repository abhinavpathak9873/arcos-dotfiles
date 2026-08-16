{
  appimageTools,
  fetchurl,
  lib,
}:

let
  pname = "ultimaker-cura";
  version = "5.13.0";
  src = fetchurl {
    url = "https://github.com/Ultimaker/Cura/releases/download/${version}/UltiMaker-Cura-${version}-linux-X64.AppImage";
    hash = "sha256-EA8GgSeyWYFn8Auk2w4Gmd7UWt+Xu6stIv8XGh4ezEA=";
  };
  contents = appimageTools.extractType2 { inherit pname version src; };
in
appimageTools.wrapType2 {
  inherit pname version src;
  extraInstallCommands = ''
    install -Dm444 ${contents}/cura-icon.png \
      $out/share/icons/hicolor/256x256/apps/ultimaker-cura.png
    install -Dm444 ${contents}/com.ultimaker.cura.desktop \
      $out/share/applications/com.ultimaker.cura.desktop
    substituteInPlace $out/share/applications/com.ultimaker.cura.desktop \
      --replace-fail 'Exec=UltiMaker-Cura' 'Exec=ultimaker-cura' \
      --replace-fail 'Icon=cura-icon.png' 'Icon=ultimaker-cura' \
      --replace-fail 'Categories=Utility;' 'Categories=Graphics;Engineering;'
  '';
  meta = {
    description = "UltiMaker Cura 3D-printing slicer";
    homepage = "https://ultimaker.com/software/ultimaker-cura/";
    license = lib.licenses.lgpl3Plus;
    platforms = [ "x86_64-linux" ];
    mainProgram = "ultimaker-cura";
  };
}
