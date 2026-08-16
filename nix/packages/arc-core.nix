{
  lib,
  fetchurl,
  rustPlatform,
  pkg-config,
  makeWrapper,
  onnxruntime,
  openssl,
  libxkbcommon,
  wayland,
  vulkan-loader,
  gtk3,
}:

let
  sileroVadModel = fetchurl {
    url = "https://raw.githubusercontent.com/sheldonix/silero-vad-rust/v6.2.1/src/silero_vad/data/silero_vad.onnx";
    hash = "sha256-GhU6IvRQnikqlOZ9b5uF6N6yW0mIaCt+F0xlJ52HiOM=";
  };
in

rustPlatform.buildRustPackage {
  pname = "arc-native-services";
  version = "0.2.0";

  src = lib.cleanSourceWith {
    src = ../..;
    filter =
      path: type:
      let
        relative = lib.removePrefix (toString ../.. + "/") (toString path);
      in
      relative == ""
      || relative == "Cargo.lock"
      || relative == "Cargo.toml"
      || relative == "crates"
      || lib.hasPrefix "crates/" relative;
  };

  cargoLock.lockFile = ../../Cargo.lock;
  cargoBuildFlags = [
    "-p"
    "arc-core"
    "-p"
    "arc-codex"
    "-p"
    "arc-speech"
    "-p"
    "arc-shell"
    "-p"
    "arc-inspector"
  ];
  cargoTestFlags = [
    "--workspace"
  ];

  nativeBuildInputs = [
    makeWrapper
    pkg-config
  ];
  buildInputs = [
    wayland
    vulkan-loader
    onnxruntime
    openssl
    libxkbcommon
    gtk3
  ];

  postFixup = ''
    wrapProgram $out/bin/arc-shell \
      --prefix LD_LIBRARY_PATH : ${
        lib.makeLibraryPath [
          wayland
          vulkan-loader
        ]
      }
    wrapProgram $out/bin/arc-speech \
      --set ORT_DYLIB_PATH ${onnxruntime}/lib/libonnxruntime.so \
      --set ARC_SILERO_MODEL ${sileroVadModel} \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath [ onnxruntime ]}
    wrapProgram $out/bin/arc-inspector \
      --set GDK_BACKEND wayland \
      --prefix XDG_DATA_DIRS : ${gtk3}/share
  '';

  meta = {
    description = "ArcOS native shell, orchestration, speech, Codex, policy, and audit services";
    license = lib.licenses.mit;
    mainProgram = "arc-core";
    platforms = lib.platforms.linux;
  };
}
