{
  config,
  lib,
  pkgs,
  modulesPath,
  ...
}:

{
  imports = [ (modulesPath + "/virtualisation/qemu-vm.nix") ];

  # See nix/tests/arcos-vm.nix. Disposable VM and installer validation use the
  # same conservative LTS kernel; the physical host profile is decided later.
  boot.kernelPackages = pkgs.linuxPackages_6_12;

  networking.hostName = "arcos-vm";
  networking.networkmanager.enable = true;

  users.users.arc = {
    isNormalUser = true;
    description = "Arc VM User";
    extraGroups = [
      "audio"
      "video"
      "input"
      "networkmanager"
      "wheel"
    ];
    initialPassword = "arc";
  };

  services.arcos = {
    enable = true;
    user = "arc";
    autoLogin = true;
    enableHeadlessOutput = true;
  };

  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];
  virtualisation = {
    memorySize = 6144;
    cores = 4;
    diskSize = 24576;
    graphics = true;
  };

  services.qemuGuest.enable = true;
  environment.systemPackages = with pkgs; [
    git
    vim
    curl
    btrfs-progs
  ];
  system.stateVersion = "26.05";
}
