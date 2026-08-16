{ modulesPath, pkgs, ... }:

{
  imports = [
    (modulesPath + "/virtualisation/qemu-vm.nix")
    ../hardware/amd-mini-pc.nix
    ../modules/personal-desktop.nix
  ];

  # QEMU/KVM on the current Zen 3 development host is stable on the LTS guest
  # kernel. The physical mini-PC ISO uses the newer NixOS 26.05 kernel.
  boot.kernelPackages = pkgs.linuxPackages_6_12;
  networking.hostName = "arcos-amd-dev-vm";
  networking.networkmanager.enable = true;

  users.users.arc = {
    isNormalUser = true;
    description = "ArcOS AMD development user";
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
    softwareRendering = true;
  };

  virtualisation = {
    memorySize = 4096;
    cores = 4;
    diskSize = 16384;
    graphics = true;
  };
  services.qemuGuest.enable = true;
  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];
  system.stateVersion = "26.05";
}
