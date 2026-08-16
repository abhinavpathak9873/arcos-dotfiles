{ config, lib, ... }:

let
  nvidia610 = import ../packages/nvidia-610.nix {
    kernelPackages = config.boot.kernelPackages;
  };
in
{
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
      message = "ArcOS workstation profile requires the NVIDIA 610 series or newer";
    }
  ];

  services.xserver.videoDrivers = [ "nvidia" ];
  hardware.graphics.enable = true;
  hardware.nvidia = {
    modesetting.enable = true;
    open = true;
    nvidiaSettings = true;
    package = nvidia610;
  };
  boot.kernelParams = [ "nvidia-drm.fbdev=1" ];
}
