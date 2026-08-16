{
  config,
  lib,
  pkgs,
  ...
}:

let
  arcUser = config.services.arcos.user;
  fullAppSuite = config.services.arcos.fullAppSuite;
  curaAppImage = pkgs.callPackage ../packages/cura-appimage.nix { };
  systemSettingsDesktop = pkgs.makeDesktopItem {
    name = "arcos-settings";
    desktopName = "System Settings";
    comment = "Displays, network, Bluetooth, sound, appearance, and system tools";
    icon = "preferences-system";
    exec = "arcos-settings";
    categories = [ "Settings" ];
  };
  hiddenGtkSettings = pkgs.writeTextDir "share/applications/nwg-look.desktop" ''
    [Desktop Entry]
    Name=Appearance Internals
    Type=Application
    NoDisplay=true
  '';
  updateDesktop = pkgs.makeDesktopItem {
    name = "arcos-update";
    desktopName = "ArcOS Update";
    comment = "Update the system and installed application sources";
    icon = "system-software-update";
    exec = "arcos-update";
    categories = [ "System" ];
  };
  captureDesktop = pkgs.makeDesktopItem {
    name = "arcos-capture";
    desktopName = "Capture";
    comment = "Take screenshots, record the screen with sound, or open the camera";
    icon = "applets-screenshooter";
    exec = "arcos-capture";
    categories = [
      "Graphics"
      "AudioVideo"
    ];
  };
  tailscaleConnect = pkgs.writeShellApplication {
    name = "arcos-tailscale";
    runtimeInputs = with pkgs; [
      kitty
      sudo
      tailscale
    ];
    text = ''
      exec kitty --class arcos-tailscale --title Tailscale --hold sudo tailscale up
    '';
  };
  tailscaleDesktop = pkgs.makeDesktopItem {
    name = "arcos-tailscale";
    desktopName = "Tailscale";
    comment = "Connect this computer to your tailnet";
    icon = "network-vpn";
    exec = "${tailscaleConnect}/bin/arcos-tailscale";
    categories = [ "Network" ];
  };
in
{
  # This profile intentionally includes proprietary applications requested for
  # Abhinav's workstation. The flake remains reproducible; accepting a package
  # license does not download or configure a personal account.
  nixpkgs.config.allowUnfree = true;

  users.users.${arcUser}.extraGroups = lib.mkAfter (
    [ "scanner" ]
    ++ lib.optionals fullAppSuite [
      "docker"
      "i2c"
      "openrgb"
    ]
  );

  networking.networkmanager.enable = true;
  networking.firewall = {
    enable = true;
    trustedInterfaces = [ "tailscale0" ];
    checkReversePath = "loose";
    allowedTCPPorts = lib.optionals fullAppSuite [ 53317 ];
    allowedUDPPorts = lib.optionals fullAppSuite [ 53317 ];
  };

  services.openssh = {
    enable = true;
    settings = {
      PasswordAuthentication = false;
      KbdInteractiveAuthentication = false;
      PermitRootLogin = "no";
    };
  };

  hardware.bluetooth = {
    enable = true;
    powerOnBoot = true;
    settings.General = {
      AutoEnable = true;
      Experimental = true;
    };
  };

  hardware.graphics.extraPackages = with pkgs; [
    intel-media-driver
    intel-vaapi-driver
    libvdpau-va-gl
    libva-vdpau-driver
  ];
  hardware.sane = {
    enable = true;
    extraBackends = with pkgs; [ sane-airscan ];
  };

  services = {
    blueman.enable = true;
    flatpak.enable = true;
    fwupd.enable = true;
    tailscale.enable = true;
    gvfs.enable = true;
    tumbler.enable = true;
    udisks2.enable = true;
    upower.enable = true;
    printing = {
      enable = true;
      drivers = with pkgs; [
        gutenprint
        hplip
      ];
    };
    pcscd.enable = true;
    gnome.gnome-keyring.enable = true;
    ddccontrol.enable = fullAppSuite;
    hardware.openrgb.enable = fullAppSuite;
    syncthing = lib.mkIf fullAppSuite {
      enable = true;
      user = arcUser;
      dataDir = "/home/${arcUser}/Sync";
      configDir = "/home/${arcUser}/.config/syncthing";
      openDefaultPorts = true;
      guiAddress = "127.0.0.1:8384";
    };
    avahi = {
      enable = true;
      nssmdns4 = true;
      openFirewall = true;
    };
  };

  programs = {
    appimage = {
      enable = true;
      binfmt = true;
    };
    dconf.enable = true;
    direnv = {
      enable = true;
      nix-direnv.enable = true;
    };
    gnupg.agent = {
      enable = true;
      enableSSHSupport = true;
    };
    nix-ld.enable = true;
    xwayland.enable = true;
    gamemode.enable = fullAppSuite;
    gamescope.enable = fullAppSuite;
    obs-studio.enable = fullAppSuite;
    steam = {
      enable = fullAppSuite;
      remotePlay.openFirewall = true;
      dedicatedServer.openFirewall = true;
      gamescopeSession.enable = true;
      extraCompatPackages = lib.optionals fullAppSuite [ pkgs.proton-ge-bin ];
    };
  };

  virtualisation = lib.mkIf fullAppSuite {
    docker = {
      enable = true;
      enableOnBoot = true;
      autoPrune.enable = true;
      daemon.settings.features.cdi = true;
    };
  };

  hardware.i2c.enable = fullAppSuite;
  hardware.xpadneo.enable = fullAppSuite;

  security.pam.services.greetd.enableGnomeKeyring = true;
  environment.sessionVariables = {
    NIXOS_OZONE_WL = "1";
    MOZ_ENABLE_WAYLAND = "1";
    QT_QPA_PLATFORM = "wayland;xcb";
    QT_QPA_PLATFORMTHEME = "gtk3";
    QT_STYLE_OVERRIDE = "adwaita-dark";
    SDL_VIDEODRIVER = "wayland,x11";
    SAL_USE_VCLPLUGIN = "gtk3";
    GTK_THEME = "adw-gtk3-dark";
    EDITOR = "nvim";
    VISUAL = "nvim";
    TERMINAL = "kitty";
  }
  // lib.optionalAttrs fullAppSuite {
    BROWSER = "google-chrome-stable";
    DEFAULT_BROWSER = "google-chrome-stable";
  };

  environment.systemPackages =
    (with pkgs; [
      # Files, media, disks, displays, and settings.
      nautilus
      file-roller
      gnome-disk-utility
      gparted
      baobab
      loupe
      papers
      celluloid
      mpv
      simple-scan
      qalculate-gtk
      nwg-displays
      wdisplays
      pavucontrol
      networkmanagerapplet
      blueman
      polkit_gnome
      mako
      brightnessctl
      playerctl
      pamixer
      wl-clipboard
      cliphist
      grimblast
      swappy
      papirus-icon-theme
      adw-gtk3
      adwaita-qt
      adwaita-qt6
      qgnomeplatform
      qgnomeplatform-qt6
      bibata-cursors
      systemSettingsDesktop
      updateDesktop
      captureDesktop
      tailscaleDesktop
      (lib.hiPrio hiddenGtkSettings)

      # Complete local media/document support without codec prompts.
      ffmpeg-full
      libdvdcss
      libheif
      libjxl
      poppler-utils
      webp-pixbuf-loader
      gst_all_1.gst-plugins-base
      gst_all_1.gst-plugins-good
      gst_all_1.gst-plugins-bad
      gst_all_1.gst-plugins-ugly
      gst_all_1.gst-libav
      p7zip
      unrar
      unzip
      zip

      # Curated, redistributable wallpaper collection.
      gnome-backgrounds

      # Terminal and development creature comforts.
      kitty
      tmux
      neovim
      tree
      btop
      htop
      cmatrix
      git
      git-lfs
      github-cli
      lazygit
      lazydocker
      ripgrep
      fd
      fzf
      jq
      yq-go
      bat
      eza
      zoxide
      just
      nh
      fastfetch
      cliamp
      localsend
      pinta
      btrfs-assistant
      snapper
      gcc
      gnumake
      pkg-config
      nodejs_22
      python3
      rustup
      go
      appimage-run
    ])
    ++ (
      with pkgs;
      lib.optionals fullAppSuite [
        google-chrome
        vscode
        spotify
        libreoffice
        obsidian
        github-desktop
        gnome-software
        gearlever
        orca-slicer
        curaAppImage
        openrgb
        docker-compose
        distrobox
        ddcutil
        i2c-tools
        mission-center
        kooha
        snapshot
        cameractrls
        satty
        wf-recorder
        protonup-qt
        protontricks
        mangohud
        codex
        opencode
        claude-code
      ]
    );

  fonts.packages = with pkgs; [
    noto-fonts
    noto-fonts-color-emoji
    nerd-fonts.jetbrains-mono
    nerd-fonts.symbols-only
  ];

  environment.etc."xdg/mimeapps.list".text = ''
    [Default Applications]
    inode/directory=org.gnome.Nautilus.desktop
    text/plain=obsidian.desktop
    text/html=google-chrome.desktop
    x-scheme-handler/http=google-chrome.desktop
    x-scheme-handler/https=google-chrome.desktop
    application/pdf=org.gnome.Papers.desktop
    image/png=org.gnome.Loupe.desktop
    image/jpeg=org.gnome.Loupe.desktop
    video/mp4=io.github.celluloid_player.Celluloid.desktop
    video/x-matroska=io.github.celluloid_player.Celluloid.desktop
    audio/mpeg=io.github.celluloid_player.Celluloid.desktop
  '';

  environment.etc."xdg/gtk-3.0/settings.ini".text = ''
    [Settings]
    gtk-theme-name=adw-gtk3-dark
    gtk-icon-theme-name=Papirus-Dark
    gtk-font-name=JetBrainsMono Nerd Font 11
    gtk-cursor-theme-name=Bibata-Modern-Ice
    gtk-cursor-theme-size=24
    gtk-application-prefer-dark-theme=1
    gtk-decoration-layout=menu:
  '';
  environment.etc."xdg/gtk-4.0/settings.ini".text = ''
    [Settings]
    gtk-theme-name=adw-gtk3-dark
    gtk-icon-theme-name=Papirus-Dark
    gtk-font-name=JetBrainsMono Nerd Font 11
    gtk-cursor-theme-name=Bibata-Modern-Ice
    gtk-cursor-theme-size=24
    gtk-application-prefer-dark-theme=1
    gtk-decoration-layout=menu:
  '';
  environment.etc."xdg/fastfetch/config.jsonc".source = ../../config/desktop/fastfetch.jsonc;
  environment.etc."arcos-desktop/fastfetch-logo.txt".source = ../../config/desktop/fastfetch-logo.txt;

  services.accounts-daemon.enable = true;
  system.activationScripts.arcosUserAvatar = lib.stringAfter [ "users" ] ''
    install -d -m 0755 /var/lib/AccountsService/icons /var/lib/AccountsService/users
    install -m 0644 ${../../assets/icons/arc-logo.png} /var/lib/AccountsService/icons/${arcUser}
    cat > /var/lib/AccountsService/users/${arcUser} <<EOF
    [User]
    Icon=/var/lib/AccountsService/icons/${arcUser}
    SystemAccount=false
    EOF
  '';

  nix = {
    settings = {
      auto-optimise-store = true;
      experimental-features = [
        "nix-command"
        "flakes"
      ];
    };
    gc = {
      automatic = true;
      dates = "weekly";
      options = "--delete-older-than 14d";
    };
  };
}
