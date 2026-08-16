{
  config,
  lib,
  pkgs,
  ...
}:

let
  nvidia610 = import ../packages/nvidia-610.nix {
    kernelPackages = config.boot.kernelPackages;
  };
in
{
  # Mesa remains the default for Intel and AMD. The NVIDIA driver is present in
  # the same closure and udev loads it only when a matching PCI device exists.
  # This is what makes one live ISO usable across all three GPU families.
  nixpkgs.config.allowUnfreePredicate =
    pkg:
    builtins.elem (lib.getName pkg) [
      "nvidia-x11"
      "nvidia-settings"
      "nvidia-persistenced"
    ];

  assertions = [
    {
      assertion = lib.versionAtLeast nvidia610.version "610";
      message = "The ArcOS universal image requires NVIDIA 610 or newer";
    }
  ];

  hardware = {
    enableRedistributableFirmware = true;
    cpu.amd.updateMicrocode = lib.mkDefault true;
    cpu.intel.updateMicrocode = lib.mkDefault true;
    graphics = {
      enable = true;
      enable32Bit = true;
    };
    nvidia = {
      modesetting.enable = true;
      open = true;
      nvidiaSettings = true;
      package = nvidia610;
      powerManagement.enable = true;
    };
    nvidia-container-toolkit.enable = true;
  };

  # modesetting covers current Intel graphics; amdgpu and NVIDIA are selected
  # by PCI modalias. Keeping every driver here also gives the NixOS NVIDIA and
  # container-toolkit modules the complete runtime integration they expect.
  services.xserver.videoDrivers = [
    "modesetting"
    "amdgpu"
    "nvidia"
  ];

  boot.kernelParams = lib.mkAfter [
    "nvidia-drm.modeset=1"
    "nvidia-drm.fbdev=1"
  ];

  environment.systemPackages = with pkgs; [
    libva-utils
    mesa-demos
    vulkan-tools
    nvidia610.settings
  ];
}
