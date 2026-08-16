{
  lib,
  modulesPath,
  pkgs,
  ...
}:

{
  imports = [ (modulesPath + "/installer/cd-dvd/installation-cd-minimal.nix") ];

  # Keep the installer on the same LTS kernel exercised by the graphical VM.
  # The 6.18 guest kernel currently exposes a QEMU/KVM graphics/CPUID failure
  # on this Zen 3 validation host; physical NVIDIA support is supplied by the
  # out-of-tree driver selected in the eventual hardware configuration.
  boot.kernelPackages = pkgs.linuxPackages_6_12;

  networking.hostName = "arcos-installer";
  services.arcos = {
    enable = true;
    user = "arc";
    autoLogin = true;
    enableHeadlessOutput = false;
    softwareRendering = true;
  };

  users.users.arc = {
    isNormalUser = true;
    description = "ArcOS Installer";
    extraGroups = [
      "audio"
      "video"
      "input"
      "networkmanager"
      "wheel"
    ];
    initialPassword = "arc";
  };

  boot.supportedFilesystems = [
    "btrfs"
    "ntfs"
  ];
  environment.systemPackages = with pkgs; [
    btrfs-progs
    efibootmgr
    git
    jq
    rsync
  ];
  boot.zfs.forceImportRoot = false;
  isoImage.volumeID = "ARCOS_2605";
  image.fileName = lib.mkForce "arcos-26.05-x86_64.iso";
  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];
  system.stateVersion = "26.05";
}
