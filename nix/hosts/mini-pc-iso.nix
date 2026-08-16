{
  config,
  lib,
  modulesPath,
  pkgs,
  ...
}:

let
  repoSource = lib.cleanSourceWith {
    src = ../..;
    filter =
      path: type:
      let
        relative = lib.removePrefix (toString ../.. + "/") (toString path);
        excludedRoots = [
          ".git"
          "artifacts"
          "dist"
          "docs"
          "node_modules"
          "reports"
          "result"
          "target"
        ];
      in
      relative == ""
      || !lib.any (root: relative == root || lib.hasPrefix "${root}/" relative) excludedRoots;
  };
  hermesPackage = config.services.arcos.hermesPackage;
  aiEnabled = config.services.arcos.enableAi;
  fullAppSuite = config.services.arcos.fullAppSuite;
  arcInstallerFinalize = pkgs.writeShellApplication {
    name = "arc-installer-finalize";
    runtimeInputs = with pkgs; [
      coreutils
      gnugrep
      gnused
      util-linux
    ];
    text = ''
      if [ "$EUID" -ne 0 ]; then
        printf 'ArcOS finalization must run as root.\n' >&2
        exit 1
      fi
      if [ "$#" -ne 2 ]; then
        printf 'Usage: arc-installer-finalize TARGET_ROOT USERNAME\n' >&2
        exit 2
      fi

      target_root="$1"
      username="$2"
      if [[ "$target_root" != /* || "$target_root" = / || ! -d "$target_root/etc/nixos" ]]; then
        printf 'Refusing invalid installer target root: %s\n' "$target_root" >&2
        exit 1
      fi
      if [[ ! "$username" =~ ^[a-z_][a-z0-9_-]*$ ]]; then
        printf 'Refusing invalid installed username: %s\n' "$username" >&2
        exit 1
      fi

      install -d -m 0755 "$target_root/etc/arcos"
      cp -a ${repoSource}/. "$target_root/etc/arcos/"
      chmod -R u+w "$target_root/etc/arcos"

      cat >"$target_root/etc/nixos/arcos-profile.nix" <<EOF
      { config, lib, pkgs, ... }:

      {
        imports = [
          /etc/arcos/nix/modules/arcos.nix
          /etc/arcos/nix/modules/personal-desktop.nix
          /etc/arcos/nix/hardware/amd-mini-pc.nix
          /etc/arcos/nix/hardware/universal-gpu.nix
        ];

        boot.kernelPackages = pkgs.linuxPackages_6_18;
        boot.supportedFilesystems = lib.mkForce [ "btrfs" "ext4" "ntfs" "vfat" ];
        boot.loader.timeout = 5;
        boot.loader.grub.enable = lib.mkForce false;
        boot.loader.systemd-boot.enable = lib.mkForce false;
        boot.loader.limine = {
          enable = true;
          maxGenerations = 20;
          enableEditor = false;
          style = {
            wallpapers = [ /etc/arcos/assets/wallpapers/arcos-default.png ];
            wallpaperStyle = "stretched";
            backdrop = "11121A";
            interface = {
              branding = "ARCOS  ◆  NIXOS GENERATIONS";
              brandingColor = "CBA6F7";
              helpColor = "B5BFE2";
              helpColorBright = "F4B8E4";
            };
          };
        };

        services.arcos = {
          enable = true;
          enableAi = ${if aiEnabled then "true" else "false"};
          fullAppSuite = ${if fullAppSuite then "true" else "false"};
          user = "$username";
          hermesPackage = ${if hermesPackage == null then "null" else "${hermesPackage}"};
          autoLogin = false;
          enableHeadlessOutput = ${if aiEnabled then "true" else "false"};
          softwareRendering = false;
        };

        services.greetd = {
          enable = true;
          settings.default_session = {
            command = "${pkgs.tuigreet}/bin/tuigreet --time --remember --remember-user-session --cmd /run/current-system/sw/bin/arc-sway-start";
            user = "greeter";
          };
        };

        users.users."$username".extraGroups = lib.mkAfter [
          "audio"
          "video"
          "input"
          "networkmanager"
          "wheel"
        ];

        nix.settings.experimental-features = [ "nix-command" "flakes" ];
      }
      EOF

      filesystem_type="$(findmnt -n -o FSTYPE --target "$target_root" 2>/dev/null || true)"
      if [ "$filesystem_type" = btrfs ]; then
        cat >"$target_root/etc/nixos/arcos-filesystem.nix" <<EOF
      { ... }:

      {
        services.btrfs.autoScrub = {
          enable = true;
          interval = "weekly";
          fileSystems = [ "/" ];
        };
        services.snapper.configs.root = {
          SUBVOLUME = "/";
          ALLOW_USERS = [ "$username" ];
          TIMELINE_CREATE = true;
          TIMELINE_CLEANUP = true;
          TIMELINE_LIMIT_HOURLY = 8;
          TIMELINE_LIMIT_DAILY = 7;
          TIMELINE_LIMIT_WEEKLY = 4;
          TIMELINE_LIMIT_MONTHLY = 6;
          NUMBER_CLEANUP = true;
          NUMBER_LIMIT = 20;
          NUMBER_LIMIT_IMPORTANT = 8;
        };
      }
      EOF
      else
        cat >"$target_root/etc/nixos/arcos-filesystem.nix" <<EOF
      { ... }: { }
      EOF
      fi

      configuration="$target_root/etc/nixos/configuration.nix"
      if ! grep -q './arcos-profile.nix' "$configuration"; then
        sed -i '/\.\/hardware-configuration\.nix/a\      ./arcos-profile.nix' "$configuration"
      fi
      if ! grep -q './arcos-filesystem.nix' "$configuration"; then
        sed -i '/\.\/hardware-configuration\.nix/a\      ./arcos-filesystem.nix' "$configuration"
      fi

      cat >"$target_root/etc/nixos/ARCOS-DEVELOPMENT" <<EOF
      ArcOS is under active development.
      The reproducible project source used for this installation is in /etc/arcos.
      Rebuild with: sudo nixos-rebuild switch
      Keep the live USB until physical hardware and rollback tests pass.
      EOF

      if [ "''${ARCOS_INSTALLER_DRY_RUN:-0}" = 1 ]; then
        printf 'ArcOS installer dry run prepared %s for %s.\n' "$target_root" "$username"
        exit 0
      fi

      nixos-install --no-root-passwd --root "$target_root"
    '';
  };
  arcCalamaresModule = pkgs.runCommand "arcos-calamares-module" { } ''
    install -d "$out/lib/calamares/modules/arcos"
    cp ${../../config/calamares/arcos/module.desc} "$out/lib/calamares/modules/arcos/module.desc"
    substitute ${../../config/calamares/arcos/main.py} "$out/lib/calamares/modules/arcos/main.py" \
      --subst-var-by finalizeScript ${arcInstallerFinalize}/bin/arc-installer-finalize
  '';
  arcCalamaresConfig =
    pkgs.runCommand "arcos-calamares-config"
      {
        nativeBuildInputs = [ pkgs.gnused ];
      }
      ''
        install -d "$out/etc/calamares/modules"
        cp -R ${pkgs.calamares-nixos-extensions}/etc/calamares/. "$out/etc/calamares/"
        chmod -R u+w "$out/etc/calamares"
        install -d "$out/share/calamares/branding/arcos"
        cp ${../../config/calamares/branding/branding.desc} \
          "$out/share/calamares/branding/arcos/branding.desc"
        cp ${../../config/calamares/branding/arcos-mark.svg} \
          "$out/share/calamares/branding/arcos/arcos-mark.svg"
        cp ${../../config/calamares/branding/stylesheet.qss} \
          "$out/share/calamares/branding/arcos/stylesheet.qss"
        sed -i \
          's|^modules-search:.*|modules-search: [ local, ${pkgs.calamares-nixos-extensions}/lib/calamares/modules, ${arcCalamaresModule}/lib/calamares/modules ]|' \
          "$out/etc/calamares/settings.conf"
        sed -i 's/^branding:.*/branding: arcos/' "$out/etc/calamares/settings.conf"
        sed -i '/^  - nixos$/a\  - arcos' "$out/etc/calamares/settings.conf"
        cp ${../../config/calamares/packagechooser.conf} \
          "$out/etc/calamares/modules/packagechooser.conf"
        sed -i 's/^defaultFileSystemType:.*/defaultFileSystemType: "btrfs"/' \
          "$out/etc/calamares/modules/partition.conf"
      '';
  arcInstaller = pkgs.writeShellApplication {
    name = "arc-installer";
    runtimeInputs = [ pkgs.calamares ];
    text = ''
      wayland_socket="''${XDG_RUNTIME_DIR:?ArcOS installer requires a graphical session}/''${WAYLAND_DISPLAY:?ArcOS installer requires Wayland}"
      exec /run/wrappers/bin/sudo -n env \
        XDG_RUNTIME_DIR=/run \
        WAYLAND_DISPLAY="$wayland_socket" \
        QT_QPA_PLATFORM=wayland \
        XDG_SESSION_TYPE=wayland \
        XDG_CONFIG_DIRS=${arcCalamaresConfig}/etc:${pkgs.calamares-nixos-extensions}/etc:/etc/xdg \
        XDG_DATA_DIRS=${arcCalamaresConfig}/share:${pkgs.calamares-nixos-extensions}/share:/run/current-system/sw/share \
        ${pkgs.calamares}/bin/calamares --xdg-config
    '';
  };
  installerDesktop = pkgs.makeDesktopItem {
    name = "arcos-installer";
    desktopName = "Install ArcOS";
    comment = "Install ArcOS on this computer";
    icon = "calamares";
    exec = "${arcInstaller}/bin/arc-installer";
    categories = [ "System" ];
  };
in
{
  imports = [
    (modulesPath + "/installer/cd-dvd/installation-cd-graphical-base.nix")
    ../hardware/amd-mini-pc.nix
    ../hardware/universal-gpu.nix
    ../modules/personal-desktop.nix
  ];

  # The physical candidate uses the current 26.05 kernel for newer AMD iGPU,
  # Wi-Fi, and power-management support. KVM acceptance has a separate LTS
  # profile because of a host-specific QEMU CPUID issue.
  boot.kernelPackages = pkgs.linuxPackages_6_18;
  # The live candidate does not need ZFS; forcing this list avoids coupling the
  # newest hardware kernel to an out-of-tree ZFS module during evaluation.
  boot.supportedFilesystems = lib.mkForce [
    "btrfs"
    "ext4"
    "ntfs"
    "vfat"
  ];
  networking.hostName = "arcos-universal-live";
  networking.networkmanager.enable = true;
  security.sudo.wheelNeedsPassword = false;
  programs.partition-manager.enable = true;
  i18n.supportedLocales = [ "all" ];

  services.arcos = {
    enable = true;
    user = "arc";
    autoLogin = true;
    enableHeadlessOutput = false;
    softwareRendering = true;
  };

  users.users.arc = {
    isNormalUser = true;
    description = "ArcOS live user";
    extraGroups = [
      "audio"
      "video"
      "input"
      "networkmanager"
      "wheel"
    ];
    initialPassword = "arc";
  };

  environment.systemPackages = with pkgs; [
    arcInstaller
    arcInstallerFinalize
    installerDesktop
    calamares-nixos-extensions
    btrfs-progs
    efibootmgr
    git
    jq
    rsync
  ];
  environment.etc."sway/config.d/95-live-installer.conf".text = ''
    exec ${pkgs.writeShellScript "arc-installer-autostart" ''
      sleep 3
      exec ${arcInstaller}/bin/arc-installer
    ''}
  '';
  environment.etc."arcos".source = repoSource;
  boot.zfs.forceImportRoot = false;
  isoImage.volumeID = "ARCOS_UNIV_DEV";
  image.fileName = lib.mkDefault "arcos-universal-26.05-dev.iso";
  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];
  system.stateVersion = "26.05";
}
