{ lib, ... }:

{
  imports = [ ./mini-pc-iso.nix ];

  networking.hostName = lib.mkForce "arcos-desktop-live";
  services.arcos = {
    enableAi = false;
    fullAppSuite = true;
    enableHeadlessOutput = false;
    softwareRendering = lib.mkForce false;
  };

  isoImage.volumeID = lib.mkForce "ARCOS_WORKSTATION";
  image.fileName = lib.mkForce "arcos-universal-v1.3.iso";
}
