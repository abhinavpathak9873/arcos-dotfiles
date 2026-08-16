{
  fetchurl,
  runCommand,
}:

let
  revision = "f3ff3571791e39611d31c381e3a41a3af07b4987";
  baseUrl = "https://huggingface.co/hexgrad/Kokoro-82M/resolve/${revision}";
  config = fetchurl {
    url = "${baseUrl}/config.json";
    hash = "sha256-WrsB4kA7ByvwPQT94WBEPiCdeg2tSaQjvhUZa5tDwX8=";
  };
  model = fetchurl {
    url = "${baseUrl}/kokoro-v1_0.pth";
    hash = "sha256-SW26EY0aWPXz2y78iNvcIW4Eg/yJ/m5H7h8sU/GK0eQ=";
  };
  voice = fetchurl {
    url = "${baseUrl}/voices/af_heart.pt";
    hash = "sha256-CrVwm4/6sZv9hJzRHZj3W2CvdzMlOtDWexI4KhAstP8=";
  };
in
runCommand "kokoro-82m-offline-${builtins.substring 0 8 revision}" { } ''
  install -Dm0444 ${config} $out/share/kokoro-82m/config.json
  install -Dm0444 ${model} $out/share/kokoro-82m/kokoro-v1_0.pth
  install -Dm0444 ${voice} $out/share/kokoro-82m/voices/af_heart.pt
''
