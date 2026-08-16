{ kernelPackages }:

# Pinned from the upstream NixOS new-feature branch. Keeping the hashes here
# makes the RTX workstation decision reproducible without moving all of ArcOS
# away from the locked NixOS 26.05 input.
kernelPackages.nvidiaPackages.mkDriver {
  version = "610.57.04";
  sha256_64bit = "sha256-suk1xmuDuwDAyFe8jg7g/VLekoa0DJzB7sKafOfrEW0=";
  sha256_aarch64 = "sha256-QCefrMBCmpOwuOyXv1k5Gj0iB2CYlPgnG3JToUw/j54=";
  openSha256 = "sha256-rQHOOOY4KL92Ww3KDwh+j4eGU7oNAH8LutZC5wmFnPo=";
  settingsSha256 = "sha256-ZEMo8I8Zc2Tq6RVDNYpAH+f094dUaZiBqO+5f6lIjRI=";
  persistencedSha256 = "sha256-aXmD2VY1RLlgAnlHhOUMWzvMyhI6JTClcFLm4imF/mA=";
}
