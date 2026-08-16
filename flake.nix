{
  description = "ArcOS — native Sway desktop with optional persistent AI services";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    hermes = {
      url = "github:NousResearch/hermes-agent/c83ddd6a51ec211458b3145da7139aafb70191d0";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      hermes,
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      nvidiaPkgs = import nixpkgs {
        inherit system;
        config.allowUnfreePredicate =
          pkg:
          builtins.elem (nixpkgs.lib.getName pkg) [
            "nvidia-x11"
            "nvidia-settings"
            "nvidia-persistenced"
          ];
      };
    in
    {
      packages.${system} = rec {
        arc-core = pkgs.callPackage ./nix/packages/arc-core.nix { };
        hermes-agent = hermes.packages.${system}.default;
        nvidia610 = nvidiaPkgs.callPackage ./nix/packages/nvidia-610.nix {
          kernelPackages = nvidiaPkgs.linuxPackages_6_12;
        };
        nvidia610-open = nvidia610.open;
        nvidia610-linux618 = nvidiaPkgs.callPackage ./nix/packages/nvidia-610.nix {
          kernelPackages = nvidiaPkgs.linuxPackages_6_18;
        };
        nvidia610-linux618-open = nvidia610-linux618.open;
        vm = self.nixosConfigurations.arcos-vm.config.system.build.vm;
        iso = self.nixosConfigurations.arcos-iso.config.system.build.isoImage;
        mini-pc-vm = self.nixosConfigurations.arcos-mini-pc-vm.config.system.build.vm;
        universal-iso = self.nixosConfigurations.arcos-universal-iso.config.system.build.isoImage;
        desktop-iso = self.nixosConfigurations.arcos-desktop-iso.config.system.build.isoImage;
        mini-pc-iso = universal-iso;
        default = arc-core;
      };

      nixosModules = {
        default = import ./nix/modules/arcos.nix;
        personalDesktop = import ./nix/modules/personal-desktop.nix;
        universalGpu = import ./nix/hardware/universal-gpu.nix;
      };

      nixosConfigurations.arcos-vm = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          self.nixosModules.default
          { services.arcos.hermesPackage = self.packages.${system}.hermes-agent; }
          ./nix/hosts/vm.nix
        ];
      };

      nixosConfigurations.arcos-iso = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          self.nixosModules.default
          { services.arcos.hermesPackage = self.packages.${system}.hermes-agent; }
          ./nix/hosts/iso.nix
        ];
      };

      nixosConfigurations.arcos-mini-pc-vm = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          self.nixosModules.default
          { services.arcos.hermesPackage = self.packages.${system}.hermes-agent; }
          ./nix/hosts/mini-pc-vm.nix
        ];
      };

      nixosConfigurations.arcos-universal-iso = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          self.nixosModules.default
          { services.arcos.hermesPackage = self.packages.${system}.hermes-agent; }
          ./nix/hosts/mini-pc-iso.nix
        ];
      };
      nixosConfigurations.arcos-desktop-iso = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          self.nixosModules.default
          { services.arcos.hermesPackage = self.packages.${system}.hermes-agent; }
          ./nix/hosts/desktop-iso.nix
        ];
      };
      nixosConfigurations.arcos-mini-pc-iso = self.nixosConfigurations.arcos-universal-iso;

      checks.${system} = {
        inherit (self.packages.${system})
          arc-core
          nvidia610
          nvidia610-open
          nvidia610-linux618
          nvidia610-linux618-open
          ;
        arcos-vm = import ./nix/tests/arcos-vm.nix {
          inherit pkgs;
          arcosModule = self.nixosModules.default;
          hermesPackage = self.packages.${system}.hermes-agent;
        };
        arcos-mini-pc = import ./nix/tests/arcos-mini-pc.nix {
          inherit pkgs;
          arcosModule = self.nixosModules.default;
          hermesPackage = self.packages.${system}.hermes-agent;
        };
        arcos-desktop = import ./nix/tests/arcos-desktop.nix {
          inherit pkgs;
          arcosModule = self.nixosModules.default;
          hermesPackage = self.packages.${system}.hermes-agent;
        };
      };

      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          cargo
          rustc
          rustfmt
          clippy
          gtk3
          pkg-config
          openssl
          onnxruntime
          wayland
          libxkbcommon
          vulkan-loader
          jq
          qemu_kvm
        ];
        ARC_NIXPKGS_REV = nixpkgs.rev or "dirty";
      };

      formatter.${system} = pkgs.nixfmt;
    };
}
