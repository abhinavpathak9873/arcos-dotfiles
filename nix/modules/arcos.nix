{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.arcos;
  wallpaper = ../../assets/wallpapers/arcos-default.png;
  workspace1 = if cfg.enableAi then "1:Arc" else "1";
  workspace2 = if cfg.enableAi then "2:Web" else "2";
  workspace3 = if cfg.enableAi then "3:Files" else "3";
  workspace4 = if cfg.enableAi then "4:Code" else "4";
  workspace5 = if cfg.enableAi then "5:Media" else "5";
  workspace6 = if cfg.enableAi then "6:Work" else "6";
  workspace7 = if cfg.enableAi then "7:Research" else "7";
  workspace8 = if cfg.enableAi then "8:Comms" else "8";
  workspace9 = if cfg.enableAi then "9:VMs" else "9";
  workspace10 = if cfg.enableAi then "10:Extra" else "10";
  firstWorkspace = workspace1;
  swayPackage = if cfg.enableAi then pkgs.sway else pkgs.swayfx;
  herdr = pkgs.callPackage ../packages/herdr.nix { };
  aether = pkgs.callPackage ../packages/aether.nix { };
  omawrite = pkgs.callPackage ../packages/omawrite.nix { };
  omacut = pkgs.callPackage ../packages/omacut.nix { };
  kokoro82m = pkgs.callPackage ../packages/kokoro-82m.nix { };
  kokoroPython = pkgs.python3.withPackages (pythonPackages: [
    pythonPackages.kokoro
    pythonPackages.numpy
    pythonPackages.soundfile
  ]);
  kokoroTts = pkgs.writeShellApplication {
    name = "kokoro-tts";
    runtimeInputs = [
      kokoroPython
      pkgs.pipewire
    ];
    text = ''
      export KOKORO_82M_DIR=${kokoro82m}/share/kokoro-82m
      exec ${kokoroPython}/bin/python ${../../scripts/kokoro-tts.py} "$@"
    '';
  };
  walkerWithWindowFocus = pkgs.walker.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ../patches/walker-focus-windows.patch ];
  });
  elephantWithSwayWorkspaces = pkgs.elephant.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ../patches/elephant-sway-workspaces.patch ];
  });
  voxtypeWithOsd = pkgs.voxtype.overrideAttrs (old: {
    buildFeatures = (old.buildFeatures or [ ]) ++ [ "osd-gtk4" ];
    cargoBuildFlags = (old.cargoBuildFlags or [ ]) ++ [ "--features=osd-gtk4" ];
    cargoTestFlags = (old.cargoTestFlags or [ ]) ++ [ "--features=osd-gtk4" ];
    patches = (old.patches or [ ]) ++ [ ../patches/voxtype-material-osd.patch ];
    nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ pkgs.wrapGAppsHook4 ];
    buildInputs = (old.buildInputs or [ ]) ++ [
      pkgs.gtk4
      pkgs.gtk4-layer-shell
    ];
  });
  overviewPython = pkgs.python3.withPackages (pythonPackages: [
    pythonPackages.pygobject3
    pythonPackages.pycairo
  ]);
  rofiThemeArgs = lib.optionalString (
    !cfg.enableAi
  ) "-config /etc/xdg/rofi/arcos-desktop.rasi -theme $HOME/.config/arcos-desktop/rofi.rasi";
  whisperModel = pkgs.fetchurl {
    url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small.en-q8_0.bin";
    hash = "sha256-Z6F59gjqYRS9P9uQYOditYij+zvQDEOHlxvk0XeVgGc=";
  };
  arcHardStop = pkgs.writeShellApplication {
    name = "arc-hard-stop";
    runtimeInputs = [
      cfg.package
      pkgs.libnotify
    ];
    text = ''
      arcctl stop >/dev/null 2>&1 || true
      notify-send --urgency=critical "Arc stopped" "Microphone, speech, agent work, and foreground control were stopped." 2>/dev/null || true
    '';
  };
  arcRecover = pkgs.writeShellApplication {
    name = "arc-recover";
    runtimeInputs = [
      pkgs.systemd
      pkgs.libnotify
    ];
    text = ''
      systemctl --user restart arc-core.service arc-speech.service arc-shell.service hermes.service 2>/dev/null || true
      notify-send "Arc recovery" "The persistent Arc services were restarted." 2>/dev/null || true
    '';
  };
  arcVoiceToggle = pkgs.writeShellApplication {
    name = "arc-voice-toggle";
    runtimeInputs = [ cfg.package ];
    text = ''
      arcctl voice toggle >/dev/null
    '';
  };
  arcTextPrompt = pkgs.writeShellApplication {
    name = "arc-text-prompt";
    runtimeInputs = [ cfg.package ];
    text = ''
      arcctl prompt >/dev/null
    '';
  };
  arcShow = pkgs.writeShellApplication {
    name = "arc-show";
    runtimeInputs = [ cfg.package ];
    text = ''
      arcctl toggle >/dev/null
    '';
  };
  arcosBrowser = pkgs.writeShellApplication {
    name = "arcos-browser";
    runtimeInputs =
      if !cfg.enableAi && cfg.fullAppSuite then [ pkgs.google-chrome ] else [ pkgs.chromium ];
    text = ''
      ${
        if !cfg.enableAi && cfg.fullAppSuite then
          "exec ${pkgs.google-chrome}/bin/google-chrome-stable \"$@\""
        else
          "exec ${pkgs.chromium}/bin/chromium \"$@\""
      }
    '';
  };
  arcosTheme = pkgs.writeShellApplication {
    name = "arcos-theme";
    runtimeInputs =
      (with pkgs; [
        imagemagick
        python3
        swayPackage
        systemd
        glib
        dconf
      ])
      ++ lib.optional cfg.enableAi pkgs.mako;
    text = ''
      export ARCOS_THEME_TEMPLATE_DIR=/etc/arcos-desktop/templates
      export ARCOS_DEFAULT_WALLPAPER=${wallpaper}
      exec python3 ${../../scripts/arcos-theme.py} "$@"
    '';
  };
  arcosWallpaperPicker = pkgs.writeShellApplication {
    name = "arcos-wallpaper-picker";
    runtimeInputs = with pkgs; [
      coreutils
      findutils
      gnused
      rofi
      arcosTheme
    ];
    text = builtins.readFile ../../scripts/arcos-wallpaper-picker.sh;
  };
  arcosSoftware = pkgs.writeShellApplication {
    name = "arcos-software";
    runtimeInputs = with pkgs; [
      flatpak
      gnome-software
    ];
    text = builtins.readFile ../../scripts/arcos-software.sh;
  };
  arcosBrew = pkgs.writeShellApplication {
    name = "brew";
    runtimeInputs = with pkgs; [
      coreutils
      git
      polkit
    ];
    text = builtins.readFile ../../scripts/arcos-brew.sh;
  };
  arcosConfigBackup = pkgs.writeShellApplication {
    name = "arcos-config-backup";
    runtimeInputs = with pkgs; [
      coreutils
      kitty
    ];
    text = builtins.readFile ../../scripts/arcos-config-backup.sh;
  };
  arcosDisplayFeatures = pkgs.writeShellApplication {
    name = "arcos-display-features";
    runtimeInputs = with pkgs; [
      coreutils
      jq
      rofi
      swayPackage
    ];
    text = builtins.readFile ../../scripts/arcos-display-features.sh;
  };
  arcosMenu = pkgs.writeShellApplication {
    name = "arcos-menu";
    runtimeInputs = with pkgs; [
      rofi
    ];
    text = builtins.readFile ../../scripts/arcos-menu.sh;
  };
  arcosApps = pkgs.writeShellApplication {
    name = "arcos-apps";
    runtimeInputs = [
      overviewPython
      pkgs.gdk-pixbuf
      pkgs.glib
      pkgs.gobject-introspection
      pkgs.graphene
      pkgs.gtk4
      pkgs.gtk4-layer-shell
      pkgs.harfbuzz
      pkgs.pango
    ];
    text = ''
      export GI_TYPELIB_PATH=${
        lib.makeSearchPath "lib/girepository-1.0" [
          pkgs.gdk-pixbuf.out
          pkgs.glib.out
          pkgs.gobject-introspection.out
          pkgs.graphene.out
          pkgs.gtk4.out
          pkgs.gtk4-layer-shell.out
          pkgs.harfbuzz.out
          pkgs.pango.out
        ]
      }
      export LD_PRELOAD=${pkgs.gtk4-layer-shell}/lib/libgtk4-layer-shell.so
      export GSK_RENDERER=cairo
      apps_log="''${XDG_RUNTIME_DIR:-/tmp}/arcos-apps.log"
      exec ${overviewPython}/bin/python ${../../scripts/arcos-apps.py} "$@" 2>>"$apps_log"
    '';
  };
  arcosOverview = pkgs.writeShellApplication {
    name = "arcos-overview";
    runtimeInputs = [
      overviewPython
      pkgs.gdk-pixbuf
      pkgs.glib
      pkgs.gobject-introspection
      pkgs.graphene
      pkgs.gtk4
      pkgs.gtk4-layer-shell
      pkgs.harfbuzz
      pkgs.pango
      swayPackage
    ];
    text = ''
      export GI_TYPELIB_PATH=${
        lib.makeSearchPath "lib/girepository-1.0" [
          pkgs.gdk-pixbuf.out
          pkgs.glib.out
          pkgs.gobject-introspection.out
          pkgs.graphene.out
          pkgs.gtk4.out
          pkgs.gtk4-layer-shell.out
          pkgs.harfbuzz.out
          pkgs.pango.out
        ]
      }
      export LD_PRELOAD=${pkgs.gtk4-layer-shell}/lib/libgtk4-layer-shell.so
      # GTK's Vulkan probe can stall for several seconds on software-rendered
      # live/VM sessions. These small native overlays render instantly with
      # Cairo while the compositor itself remains GPU accelerated on hardware.
      export GSK_RENDERER=cairo
      overview_log="''${XDG_RUNTIME_DIR:-/tmp}/arcos-overview.log"
      exec ${overviewPython}/bin/python ${../../scripts/arcos-overview.py} "$@" 2>>"$overview_log"
    '';
  };
  arcosLock = pkgs.writeShellApplication {
    name = "arcos-lock";
    runtimeInputs = with pkgs; [
      gtklock
      jq
    ];
    text = ''
      export ARCOS_DEFAULT_WALLPAPER=${wallpaper}
      export ARCOS_GTKLOCK_USERINFO_MODULE=${pkgs.gtklock-userinfo-module}/lib/gtklock/userinfo-module.so
      export ARCOS_GTKLOCK_PLAYERCTL_MODULE=${pkgs.gtklock-playerctl-module}/lib/gtklock/playerctl-module.so
      export ARCOS_GTKLOCK_POWERBAR_MODULE=${pkgs.gtklock-powerbar-module}/lib/gtklock/powerbar-module.so
      ${builtins.readFile ../../scripts/arcos-lock.sh}
    '';
  };
  arcosCapture = pkgs.writeShellApplication {
    name = "arcos-capture";
    runtimeInputs = with pkgs; [
      coreutils
      grim
      kooha
      libnotify
      rofi
      satty
      slurp
      snapshot
      wl-clipboard
    ];
    text = builtins.readFile ../../scripts/arcos-capture.sh;
  };
  arcosUpdate = pkgs.writeShellApplication {
    name = "arcos-update";
    runtimeInputs = with pkgs; [
      flatpak
      gum
      kitty
      nh
      nix
      nixos-rebuild
      sudo
    ];
    text = builtins.readFile ../../scripts/arcos-update.sh;
  };
  arcosRgbSync = pkgs.writeShellApplication {
    name = "arcos-rgb-sync";
    runtimeInputs = with pkgs; [
      coreutils
      jq
      openrgb
    ];
    text = builtins.readFile ../../scripts/arcos-rgb-sync.sh;
  };
  arcosGpuUsage = pkgs.writeShellApplication {
    name = "arcos-gpu-usage";
    runtimeInputs = with pkgs; [
      coreutils
      gnugrep
    ];
    text = builtins.readFile ../../scripts/arcos-gpu-usage.sh;
  };
  arcosRecord = pkgs.writeShellApplication {
    name = "arcos-record";
    runtimeInputs = with pkgs; [
      coreutils
      libnotify
      slurp
      systemd
      wf-recorder
    ];
    text = builtins.readFile ../../scripts/arcos-record.sh;
  };
  arcosMonitorBrightness = pkgs.writeShellApplication {
    name = "arcos-monitor-brightness";
    runtimeInputs = with pkgs; [
      ddcutil
      gawk
      gnugrep
      libnotify
      rofi
      zenity
    ];
    text = builtins.readFile ../../scripts/arcos-monitor-brightness.sh;
  };
  arcosPowerMenu = pkgs.writeShellApplication {
    name = "arcos-power-menu";
    runtimeInputs = [
      overviewPython
      pkgs.gdk-pixbuf
      pkgs.glib
      pkgs.gobject-introspection
      pkgs.graphene
      pkgs.gtk4
      pkgs.gtk4-layer-shell
      pkgs.harfbuzz
      pkgs.pango
      swayPackage
      pkgs.systemd
    ];
    text = ''
      export GI_TYPELIB_PATH=${
        lib.makeSearchPath "lib/girepository-1.0" [
          pkgs.gdk-pixbuf.out
          pkgs.glib.out
          pkgs.gobject-introspection.out
          pkgs.graphene.out
          pkgs.gtk4.out
          pkgs.gtk4-layer-shell.out
          pkgs.harfbuzz.out
          pkgs.pango.out
        ]
      }
      export LD_PRELOAD=${pkgs.gtk4-layer-shell}/lib/libgtk4-layer-shell.so
      export GSK_RENDERER=cairo
      power_log="''${XDG_RUNTIME_DIR:-/tmp}/arcos-power-menu.log"
      rm -f "''${XDG_RUNTIME_DIR:-/tmp}/arcos-power-menu.ready"
      exec ${overviewPython}/bin/python ${../../scripts/arcos-power-menu.py} "$@" 2>>"$power_log"
    '';
  };
  arcosCaffeine = pkgs.writeShellApplication {
    name = "arcos-caffeine";
    runtimeInputs = with pkgs; [
      libnotify
      systemd
    ];
    text = builtins.readFile ../../scripts/arcos-caffeine.sh;
  };
  arcosClipboard = pkgs.writeShellApplication {
    name = "arcos-clipboard";
    runtimeInputs = [
      pkgs.cliphist
      pkgs.libnotify
      walkerWithWindowFocus
      pkgs.wl-clipboard
    ];
    text = builtins.readFile ../../scripts/arcos-clipboard.sh;
  };
  arcosShortcuts = pkgs.writeShellApplication {
    name = "arcos-shortcuts";
    runtimeInputs = [
      overviewPython
      pkgs.gdk-pixbuf
      pkgs.glib
      pkgs.gobject-introspection
      pkgs.graphene
      pkgs.gtk4
      pkgs.gtk4-layer-shell
      pkgs.harfbuzz
      pkgs.pango
      swayPackage
    ];
    text = ''
      export GI_TYPELIB_PATH=${
        lib.makeSearchPath "lib/girepository-1.0" [
          pkgs.gdk-pixbuf.out
          pkgs.glib.out
          pkgs.gobject-introspection.out
          pkgs.graphene.out
          pkgs.gtk4.out
          pkgs.gtk4-layer-shell.out
          pkgs.harfbuzz.out
          pkgs.pango.out
        ]
      }
      export LD_PRELOAD=${pkgs.gtk4-layer-shell}/lib/libgtk4-layer-shell.so
      export GSK_RENDERER=cairo
      shortcuts_log="''${XDG_RUNTIME_DIR:-/tmp}/arcos-shortcuts.log"
      rm -f "''${XDG_RUNTIME_DIR:-/tmp}/arcos-shortcuts.ready"
      exec ${overviewPython}/bin/python ${../../scripts/arcos-shortcuts.py} "$@" 2>>"$shortcuts_log"
    '';
  };
  arcosSettings = pkgs.writeShellApplication {
    name = "arcos-settings";
    runtimeInputs =
      (with pkgs; [
        arcosConfigBackup
        arcosCapture
        arcosDisplayFeatures
        arcosMenu
        arcosSoftware
        arcosWallpaperPicker
        arcosUpdate
        bash
        blueman
        gnome-disk-utility
        kitty
        networkmanager
        networkmanagerapplet
        pavucontrol
        rofi
        snapshot
        tailscale
        wdisplays
        zenity
      ])
      ++ lib.optionals cfg.fullAppSuite (
        with pkgs;
        [
          ddcutil
          mission-center
          openrgb
        ]
      );
    text = builtins.readFile ../../scripts/arcos-settings.sh;
  };
  arcOutputLayout = pkgs.writeShellApplication {
    name = "arc-output-layout";
    runtimeInputs = [
      pkgs.jq
      swayPackage
    ];
    text = ''
      mapfile -t outputs < <(swaymsg -r -t get_outputs | jq -r \
        '[.[] | select(.active and (.name | startswith("HEADLESS-") | not))] | sort_by(.rect.x, .rect.y) | .[].name')
      [ "''${#outputs[@]}" -gt 0 ] || exit 0

      # Prefer VRR on capable displays. Unsupported outputs simply reject the
      # runtime request; they remain at their normal fixed refresh rate.
      for output in "''${outputs[@]}"; do
        swaymsg "output $output adaptive_sync on" >/dev/null 2>&1 || true
      done

      ${lib.optionalString cfg.enableAi ''
        first="''${outputs[0]}"
        second="''${outputs[1]:-$first}"
        for workspace in "${workspace1}" "${workspace2}" "${workspace3}" "${workspace4}" "${workspace5}"; do
          swaymsg "workspace $workspace output $first" >/dev/null
        done
        for workspace in "${workspace6}" "${workspace7}" "${workspace8}" "${workspace9}" "${workspace10}"; do
          swaymsg "workspace $workspace output $second" >/dev/null
        done
      ''}
    '';
  };
  arcOutputWatch = pkgs.writeShellApplication {
    name = "arc-output-watch";
    runtimeInputs = [
      arcOutputLayout
      pkgs.jq
      swayPackage
    ];
    text = ''
      arc-output-layout
      swaymsg -m -t subscribe '["output"]' | while read -r _event; do
        arc-output-layout
      done
    '';
  };
  arcSwayStart = pkgs.writeShellApplication {
    name = "arc-sway-start";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.gnugrep
      swayPackage
      pkgs.systemd
    ];
    text = ''
      udevadm settle --timeout=10 2>/dev/null || true
      nvidia_primary=false
      card_count=0
      nvidia_count=0
      for device in /sys/class/drm/card[0-9]*/device; do
        [ -e "$device/vendor" ] || continue
        card_count=$((card_count + 1))
        vendor="$(cat "$device/vendor")"
        [ "$vendor" = "0x10de" ] && nvidia_count=$((nvidia_count + 1))
        if [ "$vendor" = "0x10de" ] && [ "$(cat "$device/boot_vga" 2>/dev/null || true)" = "1" ]; then
          nvidia_primary=true
        fi
      done
      if [ "$card_count" -gt 0 ] && [ "$card_count" -eq "$nvidia_count" ]; then
        nvidia_primary=true
      fi

      sway_args=(--config /etc/sway/config)
      if $nvidia_primary; then
        export GBM_BACKEND=nvidia-drm
        export __GLX_VENDOR_LIBRARY_NAME=nvidia
        export WLR_NO_HARDWARE_CURSORS=1
        sway_args=(--unsupported-gpu "''${sway_args[@]}")
      fi

      ${lib.optionalString cfg.softwareRendering ''
        exec sway "''${sway_args[@]}"
      ''}

      # The live workstation uses the GPU by default. If DRM/GBM setup fails
      # immediately on unusual hardware, fall back to Pixman instead of
      # leaving the user at a black screen. A normal logout never triggers it.
      started="$(date +%s)"
      set +e
      sway "''${sway_args[@]}"
      status=$?
      set -e
      elapsed=$(( $(date +%s) - started ))
      if [ "$status" -eq 0 ] || [ "$elapsed" -ge 10 ]; then
        exit "$status"
      fi
      export LIBGL_ALWAYS_SOFTWARE=1
      export WLR_RENDERER=pixman
      exec sway "''${sway_args[@]}"
    '';
  };
  arcSessionStart = pkgs.writeShellApplication {
    name = "arc-session-start";
    runtimeInputs = [
      arcOutputLayout
      pkgs.dbus
      pkgs.jq
      swayPackage
      pkgs.systemd
    ];
    text = ''
      export XDG_CURRENT_DESKTOP=sway
      systemctl --user import-environment WAYLAND_DISPLAY SWAYSOCK XDG_CURRENT_DESKTOP
      dbus-update-activation-environment --systemd WAYLAND_DISPLAY SWAYSOCK XDG_CURRENT_DESKTOP=sway

      arc-output-layout
      ${lib.optionalString (!cfg.enableAi) "${arcosTheme}/bin/arcos-theme ensure"}
      systemctl --user start arcos-desktop.target

      # wlroots may focus a newly-created headless output. Always establish the
      # visible Arc workspace on the first real display before launching UI.
      physical_output="$(${swayPackage}/bin/swaymsg -r -t get_outputs \
        | ${pkgs.jq}/bin/jq -r '[.[] | select(.active and (.name | startswith("HEADLESS-") | not))][0].name // empty')"
      if [ -n "$physical_output" ]; then
        ${swayPackage}/bin/swaymsg "workspace ${firstWorkspace} output $physical_output"
        ${swayPackage}/bin/swaymsg "focus output $physical_output; workspace ${firstWorkspace}"
      fi

    '';
  };
  swayConfig = ''
    set $mod Mod4
    set $ws1 "${workspace1}"
    set $ws2 "${workspace2}"
    set $ws3 "${workspace3}"
    set $ws4 "${workspace4}"
    set $ws5 "${workspace5}"
    set $ws6 "${workspace6}"
    set $ws7 "${workspace7}"
    set $ws8 "${workspace8}"
    set $ws9 "${workspace9}"
    set $ws10 "${workspace10}"
    font pango:JetBrainsMono Nerd Font 11
    floating_modifier $mod normal
    default_border pixel 2
    default_floating_border pixel 1
    client.focused #8f7aa8 #1b1e2a #d9dcec #8f7aa8 #8f7aa8
    client.focused_inactive #414559 #1b1e2a #b5bfe2 #414559 #414559
    client.unfocused #292c3c #171923 #949cbb #292c3c #292c3c
    gaps inner 8
    gaps outer 8
    smart_gaps off
    smart_borders off
    focus_wrapping yes
    focus_follows_mouse yes
    mouse_warping container

    ${lib.optionalString (!cfg.enableAi) ''
      # SwayFX keeps the tiling model intact while adding the restrained depth
      # cues that make both one-window and multi-window layouts feel finished.
      corner_radius 10
      shadows enable
      shadows_on_csd enable
      shadow_blur_radius 18
      shadow_color #00000066
      shadow_inactive_color #00000038
      shadow_offset 0 4
      default_dim_inactive 0.015
    ''}

    input type:keyboard {
      repeat_delay 280
      repeat_rate 35
    }

    output * bg ${wallpaper} fill
    output * adaptive_sync on
    output * max_render_time off
    ${lib.optionalString cfg.enableHeadlessOutput ''
      output HEADLESS-1 mode 2560x1440@60Hz position 2560 0 scale 1
      workspace 90:arc-background output HEADLESS-1
    ''}

    seat seat0 xcursor_theme Bibata-Modern-Ice 24
    seat agent-seat fallback false
    seat agent-seat hide_cursor 1

    bindsym --to-code --no-repeat $mod+Return exec ${pkgs.kitty}/bin/kitty
    bindsym --to-code --no-repeat $mod+Shift+Return exec ${pkgs.kitty}/bin/kitty --class arcos-tmux --title tmux ${pkgs.tmux}/bin/tmux new-session -A -s main
    ${lib.optionalString cfg.enableAi "bindsym --to-code --no-repeat $mod+d exec ${pkgs.rofi}/bin/rofi -show drun ${rofiThemeArgs}"}
    bindsym --to-code --no-repeat $mod+e exec ${pkgs.nautilus}/bin/nautilus --new-window
    bindsym --to-code --no-repeat $mod+b exec ${arcosBrowser}/bin/arcos-browser
    bindsym --to-code --no-repeat $mod+Ctrl+l exec ${arcosLock}/bin/arcos-lock
    bindsym --to-code --no-repeat Print exec ${arcosCapture}/bin/arcos-capture
    bindsym --to-code --no-repeat Shift+Print exec ${pkgs.grimblast}/bin/grimblast --notify save screen
    bindsym --to-code --no-repeat Ctrl+Print exec ${pkgs.kooha}/bin/kooha

    bindsym --locked XF86AudioRaiseVolume exec ${pkgs.swayosd}/bin/swayosd-client --output-volume raise --max-volume 120
    bindsym --locked XF86AudioLowerVolume exec ${pkgs.swayosd}/bin/swayosd-client --output-volume lower --max-volume 120
    bindsym --locked XF86AudioMute exec ${pkgs.swayosd}/bin/swayosd-client --output-volume mute-toggle
    bindsym --locked XF86AudioMicMute exec ${pkgs.swayosd}/bin/swayosd-client --input-volume mute-toggle
    bindsym --locked XF86MonBrightnessUp exec ${pkgs.swayosd}/bin/swayosd-client --brightness raise
    bindsym --locked XF86MonBrightnessDown exec ${pkgs.swayosd}/bin/swayosd-client --brightness lower
    bindsym --locked XF86AudioPlay exec ${pkgs.swayosd}/bin/swayosd-client --playerctl play-pause
    bindsym --locked XF86AudioNext exec ${pkgs.swayosd}/bin/swayosd-client --playerctl next
    bindsym --locked XF86AudioPrev exec ${pkgs.swayosd}/bin/swayosd-client --playerctl previous

    bindsym --to-code --no-repeat $mod+1 workspace number 1
    bindsym --to-code --no-repeat $mod+2 workspace number 2
    bindsym --to-code --no-repeat $mod+3 workspace number 3
    bindsym --to-code --no-repeat $mod+4 workspace number 4
    bindsym --to-code --no-repeat $mod+5 workspace number 5
    bindsym --to-code --no-repeat $mod+6 workspace number 6
    bindsym --to-code --no-repeat $mod+7 workspace number 7
    bindsym --to-code --no-repeat $mod+8 workspace number 8
    bindsym --to-code --no-repeat $mod+9 workspace number 9
    bindsym --to-code --no-repeat $mod+0 workspace number 10
    bindsym --to-code --no-repeat $mod+Shift+1 move container to workspace number 1
    bindsym --to-code --no-repeat $mod+Shift+2 move container to workspace number 2
    bindsym --to-code --no-repeat $mod+Shift+3 move container to workspace number 3
    bindsym --to-code --no-repeat $mod+Shift+4 move container to workspace number 4
    bindsym --to-code --no-repeat $mod+Shift+5 move container to workspace number 5
    bindsym --to-code --no-repeat $mod+Shift+6 move container to workspace number 6
    bindsym --to-code --no-repeat $mod+Shift+7 move container to workspace number 7
    bindsym --to-code --no-repeat $mod+Shift+8 move container to workspace number 8
    bindsym --to-code --no-repeat $mod+Shift+9 move container to workspace number 9
    bindsym --to-code --no-repeat $mod+Shift+0 move container to workspace number 10

    # Complete directional window management. --to-code makes the compositor
    # consume these before Kitty or another focused client can see them.
    bindsym --to-code $mod+Left focus left
    bindsym --to-code $mod+Down focus down
    bindsym --to-code $mod+Up focus up
    bindsym --to-code $mod+Right focus right
    bindsym --to-code $mod+h focus left
    bindsym --to-code $mod+j focus down
    bindsym --to-code $mod+k focus up
    bindsym --to-code $mod+l focus right
    bindsym --to-code $mod+Shift+Left move left
    bindsym --to-code $mod+Shift+Down move down
    bindsym --to-code $mod+Shift+Up move up
    bindsym --to-code $mod+Shift+Right move right
    bindsym --to-code $mod+Shift+h move left
    bindsym --to-code $mod+Shift+j move down
    bindsym --to-code $mod+Shift+l move right

    # Window switching works with the familiar Alt+Tab and Meta+Tab patterns.
    bindsym --to-code --no-repeat Mod1+Tab focus next
    bindsym --to-code --no-repeat Mod1+Shift+Tab focus prev
    bindsym --to-code --no-repeat $mod+Tab focus next
    bindsym --to-code --no-repeat $mod+Shift+Tab focus prev
    bindsym --to-code --no-repeat $mod+Ctrl+Left workspace prev
    bindsym --to-code --no-repeat $mod+Ctrl+Right workspace next
    bindsym --to-code --no-repeat $mod+backslash workspace back_and_forth

    bindsym --to-code --no-repeat $mod+f fullscreen toggle
    bindsym --to-code --no-repeat $mod+Shift+space floating toggle
    bindsym --to-code --no-repeat $mod+s layout stacking
    bindsym --to-code --no-repeat $mod+t layout tabbed
    bindsym --to-code --no-repeat $mod+x split toggle
    bindsym --to-code --no-repeat $mod+r mode "resize"
    bindsym --to-code --no-repeat $mod+q kill

    mode "resize" {
      bindsym h resize shrink width 20 px
      bindsym j resize grow height 20 px
      bindsym k resize shrink height 20 px
      bindsym l resize grow width 20 px
      bindsym Left resize shrink width 20 px
      bindsym Down resize grow height 20 px
      bindsym Up resize shrink height 20 px
      bindsym Right resize grow width 20 px
      bindsym Return mode "default"
      bindsym Escape mode "default"
    }

    ${lib.optionalString cfg.enableAi ''
      # keyd turns a Caps tap into F23 and Caps+Space into F24. This preserves
      # a one-handed voice toggle without stealing focused application input.
      bindsym F23 exec ${arcVoiceToggle}/bin/arc-voice-toggle
      bindsym F24 exec ${arcTextPrompt}/bin/arc-text-prompt
      bindsym $mod+a exec ${arcShow}/bin/arc-show
      bindsym $mod+Escape exec ${arcHardStop}/bin/arc-hard-stop
      bindsym $mod+Shift+r exec ${arcRecover}/bin/arc-recover
    ''}
    ${lib.optionalString (!cfg.enableAi) ''
      bindsym --to-code --no-repeat $mod+space exec ${walkerWithWindowFocus}/bin/walker
      bindsym --to-code --no-repeat $mod+a exec ${arcosApps}/bin/arcos-apps
      bindsym --to-code --no-repeat $mod+w exec ${arcosOverview}/bin/arcos-overview
      bindsym --to-code --no-repeat $mod+d exec ${walkerWithWindowFocus}/bin/walker
      bindsym --to-code --no-repeat $mod+v exec ${arcosClipboard}/bin/arcos-clipboard
      bindsym --to-code --no-repeat $mod+Shift+k exec ${arcosShortcuts}/bin/arcos-shortcuts
      bindsym --to-code --no-repeat $mod+Escape exec ${arcosPowerMenu}/bin/arcos-power-menu
      bindsym --to-code --no-repeat $mod+comma exec ${arcosSettings}/bin/arcos-settings
      bindsym --to-code --no-repeat $mod+n exec ${pkgs.swaynotificationcenter}/bin/swaync-client -t -sw
      bindsym --to-code --no-repeat F23 exec ${voxtypeWithOsd}/bin/voxtype record start
      bindsym --to-code --release F23 exec ${voxtypeWithOsd}/bin/voxtype record stop
      ${lib.optionalString cfg.fullAppSuite "bindsym --to-code --no-repeat $mod+Shift+r exec ${arcosCapture}/bin/arcos-capture"}
    ''}
    bindsym $mod+Shift+e exec swaynag -t warning -m 'Exit ArcOS?' -b 'Exit' 'swaymsg exit'

    exec_always ${arcSessionStart}/bin/arc-session-start

    include /etc/sway/config.d/*
    include $HOME/.config/sway/config.d/*
  '';
in
{
  options.services.arcos = {
    enable = lib.mkEnableOption "ArcOS desktop and host-control foundation";
    enableAi = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Enable persistent Arc AI, speech, Codex, and native overlay services.";
    };
    fullAppSuite = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Include the heavier gaming, container, creator, and workstation application suite.";
    };
    preloadAiModels = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Preload optional Hermes and Kokoro AI payloads into the system closure.";
    };
    preloadLargeApps = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Preload large duplicate-purpose applications that remain available on demand.";
    };
    user = lib.mkOption {
      type = lib.types.str;
      default = "arc";
      description = "User that owns the Arc graphical session.";
    };
    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ../packages/arc-core.nix { };
      description = "Arc core package used by the desktop session.";
    };
    hermesPackage = lib.mkOption {
      type = lib.types.nullOr lib.types.package;
      default = null;
      description = "Pinned Hermes agent kernel package. Null leaves Hermes integration unavailable.";
    };
    autoLogin = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Automatically enter the Sway session; intended only for disposable VMs.";
    };
    enableHeadlessOutput = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Create a wlroots headless output for background agent workspaces.";
    };
    softwareRendering = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Use software-compatible rendering defaults for recovery or installer sessions.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = builtins.hasAttr cfg.user config.users.users;
        message = "services.arcos.user must name a declared NixOS user";
      }
      {
        assertion = cfg.enableAi || !cfg.enableHeadlessOutput;
        message = "The non-AI ArcOS desktop does not use an agent headless output";
      }
    ];

    programs.sway = {
      enable = true;
      package = swayPackage;
      wrapperFeatures.gtk = true;
      extraPackages =
        (with pkgs; [
          blueman
          brightnessctl
          grimblast
          grim
          kitty
          networkmanagerapplet
          papirus-icon-theme
          playerctl
          polkit_gnome
          rofi
          slurp
          swayidle
          swaylock
          nautilus
          waybar
          wl-clipboard
        ])
        ++ [
          elephantWithSwayWorkspaces
          walkerWithWindowFocus
        ]
        ++ lib.optional cfg.enableAi pkgs.mako;
    };

    programs.dconf = lib.mkIf (!cfg.enableAi) {
      enable = true;
      profiles.user.databases = [
        {
          settings."org/gnome/desktop/interface" = {
            color-scheme = "prefer-dark";
            gtk-theme = "adw-gtk3-dark";
            icon-theme = "Papirus-Dark";
            cursor-theme = "Bibata-Modern-Ice";
            font-name = "JetBrainsMono Nerd Font 11";
          };
        }
      ];
    };

    # Keep native shell and native inspector typography identical.
    fonts = {
      fontconfig.enable = true;
      packages = [
        pkgs.inter
        pkgs.dejavu_fonts
        pkgs.noto-fonts
        pkgs.noto-fonts-color-emoji
        pkgs.nerd-fonts.jetbrains-mono
        pkgs.nerd-fonts.symbols-only
      ];
    };

    environment.etc."sway/config".text = swayConfig;
    environment.etc."xdg/kitty/kitty.conf".source =
      if cfg.enableAi then ../../config/kitty/kitty.conf else ../../config/desktop/kitty-main.conf;
    environment.etc."xdg/kitty/abhi-look.conf".source = ../../config/kitty/abhi-look.conf;
    environment.etc."xdg/kitty/catppuccin-frappe.conf".source =
      ../../config/kitty/catppuccin-frappe.conf;
    environment.etc."xdg/waybar/config.jsonc".source =
      if cfg.enableAi then ../../config/waybar/config.jsonc else ../../config/desktop/waybar.jsonc;
    environment.etc."xdg/waybar/style.css".source = ../../config/waybar/style.css;
    environment.etc."xdg/rofi/config.rasi".source =
      if cfg.enableAi then ../../config/rofi/config.rasi else ../../config/desktop/rofi-config.rasi;
    environment.etc."xdg/rofi/arcos-desktop.rasi".source = ../../config/desktop/rofi-config.rasi;
    environment.etc."arcos-desktop/templates/waybar.css".source = ../../config/desktop/waybar.css;
    environment.etc."arcos-desktop/templates/rofi.rasi".source = ../../config/desktop/rofi.rasi;
    environment.etc."arcos-desktop/templates/mako.conf".source = ../../config/desktop/mako.conf;
    environment.etc."arcos-desktop/templates/kitty.conf".source = ../../config/desktop/kitty.conf;
    environment.etc."arcos-desktop/templates/gtk.css".source = ../../config/desktop/gtk.css;
    environment.etc."arcos-desktop/templates/apps.css".source = ../../config/desktop/apps.css;
    environment.etc."arcos-desktop/templates/overview.css".source = ../../config/desktop/overview.css;
    environment.etc."arcos-desktop/templates/shortcuts.css".source = ../../config/desktop/shortcuts.css;
    environment.etc."arcos-desktop/templates/tmux-theme.conf".source =
      ../../config/desktop/tmux-theme.conf;
    environment.etc."arcos-desktop/templates/nvim-theme.lua".source =
      ../../config/desktop/nvim-theme.lua;
    environment.etc."arcos-desktop/templates/gtklock.css".source = ../../config/desktop/gtklock.css;
    environment.etc."arcos-desktop/templates/swaync.css".source = ../../config/desktop/swaync.css;
    environment.etc."arcos-desktop/templates/swayosd.css".source = ../../config/desktop/swayosd.css;
    environment.etc."arcos-desktop/templates/nwg-bar.css".source = ../../config/desktop/nwg-bar.css;
    environment.etc."arcos-desktop/templates/walker.toml".source = ../../config/desktop/walker.toml;
    environment.etc."arcos-desktop/templates/walker.css".source = ../../config/desktop/walker.css;
    environment.etc."xdg/swaync/config.json".source = ../../config/desktop/swaync.json;
    environment.etc."tmux.conf".source = ../../config/desktop/tmux.conf;
    environment.etc."xdg/nvim/init.lua".source = ../../config/desktop/nvim.lua;
    environment.etc."xdg/nwg-bar/bar.json".source = ../../config/desktop/nwg-bar.json;
    environment.etc."xdg/swayosd/config.toml".text = ''
      [server]
      top_margin = 0.86
      max_volume = 120
      min_brightness = 5
      show_percentage = true
      keyboard_backlight = true

      [client]
    '';
    environment.etc."xdg/mako/config".text = ''
      font=JetBrainsMono Nerd Font 11
      background-color=#171923f5
      text-color=#d9dcec
      border-color=#51576d
      border-size=1
      border-radius=11
      default-timeout=5500
      width=420
      margin=12
      padding=14
    '';
    environment.etc."sway/config.d/10-arcos.conf".text = ''
      for_window [app_id="ai.arcos.inspector"] floating enable, resize set 1000 680
      for_window [app_id="arc-recovery"] floating enable, sticky enable
    '';
    environment.etc."xdg/voxtype/config.toml".text = ''
      state_file = "auto"

      [hotkey]
      enabled = false

      [audio]
      device = "default"
      sample_rate = 16000
      max_duration_secs = 180

      [whisper]
      mode = "local"
      model = "${whisperModel}"
      language = "en"
      translate = false
      on_demand_loading = false
      context_window_optimization = true

      [output]
      mode = "type"
      fallback_to_clipboard = true
      type_delay_ms = 0
      pre_type_delay_ms = 80

      [output.notification]
      on_recording_start = false
      on_recording_stop = false
      on_transcription = true

      [text]
      spoken_punctuation = true

      [status]
      icon_theme = "nerd-font"

      [osd]
      enabled = true
      frontend = "gtk4"
      position = "top-center"
      width_px = 420
      height_px = 52
      top_margin = 0.05
      opacity = 0.96
      waveform_window_secs = 3.0
      waveform_gain = 10.0
    '';

    environment.systemPackages =
      (with pkgs; [
        arcOutputLayout
        arcOutputWatch
        arcSwayStart
        arcosBrowser
        at-spi2-core
        dbus
        jq
        libei
        libnotify
        pavucontrol
        pipewire
        wireplumber
        xdg-utils
      ])
      ++ lib.optional (cfg.enableAi || !cfg.fullAppSuite) pkgs.chromium
      ++ lib.optionals cfg.enableAi [
        cfg.package
        arcHardStop
        arcRecover
        arcVoiceToggle
        arcTextPrompt
        arcShow
      ]
      ++ lib.optionals (!cfg.enableAi) [
        arcosBrew
        arcosApps
        arcosConfigBackup
        arcosCaffeine
        arcosClipboard
        arcosCapture
        arcosDisplayFeatures
        arcosLock
        arcosTheme
        arcosMenu
        arcosOverview
        arcosGpuUsage
        arcosPowerMenu
        arcosRgbSync
        arcosShortcuts
        arcosSettings
        arcosSoftware
        arcosWallpaperPicker
        arcosUpdate
        pkgs.nwg-bar
        pkgs.swaynotificationcenter
        pkgs.swayosd
        pkgs.gtklock
        voxtypeWithOsd
        aether
        herdr
        omacut
        omawrite
      ]
      ++ lib.optionals (!cfg.enableAi && cfg.fullAppSuite) [
        arcosMonitorBrightness
        arcosRecord
      ]
      ++ lib.optionals (!cfg.enableAi && cfg.fullAppSuite && cfg.preloadAiModels) [
        kokoro82m
        kokoroTts
      ]
      ++ lib.optional (
        cfg.hermesPackage != null && (cfg.enableAi || cfg.preloadAiModels)
      ) cfg.hermesPackage;

    environment.sessionVariables = lib.mkMerge [
      (lib.mkIf cfg.enableHeadlessOutput {
        WLR_BACKENDS = "drm,libinput,headless";
        WLR_HEADLESS_OUTPUTS = "1";
      })
      (lib.mkIf cfg.softwareRendering {
        LIBGL_ALWAYS_SOFTWARE = "1";
        WGPU_BACKEND = "gl";
        WLR_RENDERER = "pixman";
      })
    ];

    services.dbus.enable = true;
    services.dbus.packages = lib.optionals (!cfg.enableAi) [ pkgs.swayosd ];
    services.udev.packages = lib.optionals (!cfg.enableAi) [ pkgs.swayosd ];
    services.keyd = {
      enable = true;
      keyboards.default = {
        ids = [ "*" ];
        settings =
          if cfg.enableAi then
            {
              main.capslock = "overload(arcos, f23)";
              arcos.space = "f24";
            }
          else
            {
              main.capslock = "f23";
            };
      };
    };
    services.pipewire = {
      enable = true;
      alsa.enable = true;
      alsa.support32Bit = true;
      pulse.enable = true;
      wireplumber.enable = true;
    };
    security.polkit.enable = true;
    security.pam.services.gtklock = { };
    security.rtkit.enable = true;
    xdg.portal = {
      enable = true;
      wlr.enable = true;
      extraPortals = [ pkgs.xdg-desktop-portal-gtk ];
      config.sway.default = lib.mkForce [
        "wlr"
        "gtk"
      ];
    };

    services.greetd = lib.mkIf cfg.autoLogin {
      enable = true;
      settings.default_session = {
        command = "${arcSwayStart}/bin/arc-sway-start";
        user = cfg.user;
      };
    };

    systemd.user.targets.arcos-agent = {
      description = "Arc cancellable agent activity";
    };

    systemd.user.targets.arcos-desktop = {
      description = "ArcOS desktop session services";
      wants = [
        "arc-waybar.service"
        "arc-output-watch.service"
        "arc-polkit-agent.service"
        "arc-swayidle.service"
        "arc-network-applet.service"
        "arc-bluetooth-applet.service"
      ]
      ++ lib.optionals (!cfg.enableAi) [
        "arc-clipboard-images.service"
        "arc-clipboard-text.service"
        "arc-elephant.service"
        "arc-walker.service"
        "arc-voxtype.service"
        "swaync.service"
        "arc-swayosd.service"
      ]
      ++ lib.optionals (!cfg.enableAi && cfg.fullAppSuite) [
        "arc-openrgb.service"
        "arc-openrgb-sync.service"
      ]
      ++ lib.optionals cfg.enableAi [
        "arc-mako.service"
        "arc-core.service"
        "arc-speech.service"
        "arc-shell.service"
        "hermes.service"
      ];
    };

    systemd.user.services.arc-core = lib.mkIf cfg.enableAi {
      description = "Arc persistent orchestration and desktop control";
      partOf = [ "graphical-session.target" ];
      before = [
        "arc-shell.service"
        "arc-speech.service"
      ];
      path = [
        pkgs.sway
        pkgs.wl-clipboard
        pkgs.grim
        pkgs.curl
        pkgs.systemd
      ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/arc-core serve";
        Restart = "always";
        RestartSec = 1;
        RuntimeDirectory = "arc";
        RuntimeDirectoryMode = "0700";
        UMask = "0077";
      };
      environment = {
        ARC_RUNTIME_DIR = "%t/arc";
        ARC_HERMES_TOKEN_FILE = "%t/arc/hermes.token";
        ARC_HERMES_HTTP = "http://127.0.0.1:43826";
        ARC_HERMES_WS = "ws://127.0.0.1:43826/api/ws";
      };
    };

    systemd.user.services.hermes = lib.mkIf (cfg.enableAi && cfg.hermesPackage != null) {
      description = "Hermes sessions, memory, skills, tools, and scheduling";
      partOf = [ "graphical-session.target" ];
      after = [ "arc-core.service" ];
      serviceConfig = {
        ExecStartPre = "${pkgs.writeShellScript "arc-hermes-token" ''umask 077; test -s "$XDG_RUNTIME_DIR/arc/hermes.token" || ${pkgs.openssl}/bin/openssl rand -hex 32 > "$XDG_RUNTIME_DIR/arc/hermes.token"''}";
        ExecStart = "${pkgs.writeShellScript "arc-hermes-start" ''export HERMES_DASHBOARD_SESSION_TOKEN="$(cat "$XDG_RUNTIME_DIR/arc/hermes.token")"; exec ${cfg.hermesPackage}/bin/hermes serve --host 127.0.0.1 --port 43826''}";
        Restart = "always";
        RestartSec = 2;
        UMask = "0077";
      };
      environment = {
        ARC_RUNTIME_DIR = "%t/arc";
        HERMES_DESKTOP = "1";
      };
    };

    systemd.user.services.arc-speech = lib.mkIf cfg.enableAi {
      description = "Arc local streaming speech service";
      partOf = [ "graphical-session.target" ];
      after = [
        "pipewire.service"
        "arc-core.service"
      ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/arc-speech serve";
        Restart = "always";
        RestartSec = 1;
        UMask = "0077";
      };
      environment = {
        ARC_RUNTIME_DIR = "%t/arc";
        ARC_PIPEWIRE_RECORD = "${pkgs.pipewire}/bin/pw-record";
        ARC_WHISPER_CPP = "${pkgs.whisper-cpp}/bin/whisper-cli";
        ARC_WHISPER_MODEL = "${whisperModel}";
        ARC_TTS_COMMAND = "${pkgs.espeak-ng}/bin/espeak-ng";
      };
    };

    systemd.user.services.arc-codex = lib.mkIf cfg.enableAi {
      description = "On-demand Arc Codex app-server bridge";
      partOf = [ "arcos-agent.target" ];
      after = [ "arc-core.service" ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/arc-codex serve";
        Restart = "on-failure";
        RestartSec = 1;
        UMask = "0077";
      };
      environment = {
        ARC_RUNTIME_DIR = "%t/arc";
        ARC_CODEX_PATH = "${pkgs.codex}/bin/codex";
      };
    };

    systemd.user.services.arc-shell = lib.mkIf cfg.enableAi {
      description = "Arc native Wayland layer shell";
      partOf = [ "graphical-session.target" ];
      after = [ "arc-core.service" ];
      path = [ cfg.package ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/arc-shell";
        Restart = "always";
        RestartSec = 1;
      };
      environment = {
        ARC_RUNTIME_DIR = "%t/arc";
      }
      // lib.optionalAttrs cfg.softwareRendering {
        LIBGL_ALWAYS_SOFTWARE = "1";
        WGPU_BACKEND = "gl";
      };
    };

    systemd.user.services.arc-waybar = {
      description = "ArcOS Waybar";
      partOf = [ "arcos-desktop.target" ];
      path =
        with pkgs;
        [
          blueman
          btop
          kitty
          networkmanagerapplet
          pavucontrol
          playerctl
          nautilus
        ]
        ++ lib.optionals cfg.enableAi [
          cfg.package
          arcHardStop
        ]
        ++ lib.optionals (!cfg.enableAi) [
          arcosApps
          arcosCaffeine
          arcosMenu
          arcosGpuUsage
          arcosPowerMenu
          pkgs.swaynotificationcenter
        ];
      serviceConfig = {
        ExecStart =
          if cfg.enableAi then
            "${pkgs.waybar}/bin/waybar"
          else
            "${pkgs.waybar}/bin/waybar --config /etc/xdg/waybar/config.jsonc --style %h/.config/arcos-desktop/waybar.css";
        Restart = "on-failure";
        RestartSec = 1;
      };
    };
    systemd.user.services.arc-caffeine = lib.mkIf (!cfg.enableAi) {
      description = "ArcOS sleep blocker";
      serviceConfig = {
        ExecStart = "${pkgs.systemd}/bin/systemd-inhibit --what=idle:sleep:handle-lid-switch --who=ArcOS --why=ArcOS-sleep-blocker --mode=block ${pkgs.coreutils}/bin/sleep infinity";
        Restart = "on-failure";
        RestartSec = 2;
      };
    };
    systemd.user.services.arc-clipboard-text = lib.mkIf (!cfg.enableAi) {
      description = "ArcOS text clipboard history";
      partOf = [ "arcos-desktop.target" ];
      after = [ "graphical-session.target" ];
      serviceConfig = {
        ExecStart = "${pkgs.wl-clipboard}/bin/wl-paste --type text --watch ${pkgs.cliphist}/bin/cliphist store";
        Restart = "on-failure";
        RestartSec = 2;
      };
    };
    systemd.user.services.arc-clipboard-images = lib.mkIf (!cfg.enableAi) {
      description = "ArcOS image clipboard history";
      partOf = [ "arcos-desktop.target" ];
      after = [ "graphical-session.target" ];
      serviceConfig = {
        ExecStart = "${pkgs.wl-clipboard}/bin/wl-paste --type image --watch ${pkgs.cliphist}/bin/cliphist store";
        Restart = "on-failure";
        RestartSec = 2;
      };
    };
    systemd.user.services.arc-voxtype = lib.mkIf (!cfg.enableAi) {
      description = "ArcOS local hold-to-dictate service";
      partOf = [ "arcos-desktop.target" ];
      after = [
        "graphical-session.target"
        "pipewire.service"
        "pipewire-pulse.service"
      ];
      path = with pkgs; [
        libnotify
        playerctl
        voxtypeWithOsd
        wl-clipboard
        wtype
      ];
      serviceConfig = {
        ExecStart = "${voxtypeWithOsd}/bin/voxtype --config /etc/xdg/voxtype/config.toml --no-hotkey --model ${whisperModel} --language en --eager-processing --spoken-punctuation --filter-fillers --vad --pause-media --driver wtype,clipboard --fallback-to-clipboard --wait-for-modifier-release daemon";
        Restart = "on-failure";
        RestartSec = 3;
      };
      environment = {
        VOXTYPE_CONFIG = "/etc/xdg/voxtype/config.toml";
        VOXTYPE_OSD_FRONTEND = "gtk4";
      };
    };
    systemd.user.services.arc-walker = lib.mkIf (!cfg.enableAi) {
      description = "ArcOS universal search service";
      partOf = [ "arcos-desktop.target" ];
      after = [
        "graphical-session.target"
        "arc-elephant.service"
      ];
      requires = [ "arc-elephant.service" ];
      path = [
        elephantWithSwayWorkspaces
        pkgs.playerctl
        swayPackage
      ];
      serviceConfig = {
        ExecStart = "${walkerWithWindowFocus}/bin/walker --gapplication-service";
        Restart = "on-failure";
        RestartSec = 1;
      };
    };
    systemd.user.services.arc-elephant = lib.mkIf (!cfg.enableAi) {
      description = "ArcOS search providers and index";
      partOf = [ "arcos-desktop.target" ];
      after = [ "graphical-session.target" ];
      serviceConfig = {
        ExecStart = "${elephantWithSwayWorkspaces}/bin/elephant";
        # Desktop files intentionally use ordinary commands (for example
        # `Exec=kitty`).  Keep the complete system profile visible here so
        # Elephant can launch every installed application, not just binaries
        # from its own derivation closure.
        Environment = "PATH=/run/wrappers/bin:/etc/profiles/per-user/%u/bin:/run/current-system/sw/bin";
        Restart = "on-failure";
        RestartSec = 1;
      };
    };
    systemd.user.services.arc-mako = lib.mkIf cfg.enableAi {
      description = "ArcOS notification daemon";
      partOf = [ "arcos-desktop.target" ];
      serviceConfig = {
        ExecStart = "${pkgs.mako}/bin/mako";
        Restart = "on-failure";
        RestartSec = 1;
      };
    };
    # Use SwayNC's canonical D-Bus unit name.  Clients activate this same unit,
    # so there can never be a second notification daemon racing our service.
    systemd.user.services.swaync = lib.mkIf (!cfg.enableAi) {
      description = "ArcOS Control Center and notifications";
      partOf = [ "arcos-desktop.target" ];
      path = with pkgs; [
        arcosCaffeine
        arcosCapture
        arcosLock
        arcosPowerMenu
        arcosSettings
        arcosUpdate
        bluez
        gnugrep
        networkmanager
        swaynotificationcenter
        systemd
        wdisplays
      ];
      serviceConfig = {
        ExecStart = "${pkgs.swaynotificationcenter}/bin/swaync --config /etc/xdg/swaync/config.json --style %h/.config/swaync/style.css";
        Restart = "on-failure";
        RestartSec = 1;
      };
    };
    systemd.user.services.arc-swayosd = lib.mkIf (!cfg.enableAi) {
      description = "ArcOS volume and brightness on-screen display";
      partOf = [ "arcos-desktop.target" ];
      serviceConfig = {
        ExecStart = "${pkgs.swayosd}/bin/swayosd-server";
        Restart = "on-failure";
        RestartSec = 1;
      };
    };
    systemd.user.services.arc-openrgb = lib.mkIf (!cfg.enableAi && cfg.fullAppSuite) {
      description = "ArcOS OpenRGB background server";
      partOf = [ "arcos-desktop.target" ];
      serviceConfig = {
        ExecStart = "${pkgs.openrgb}/bin/openrgb --server --server-host 127.0.0.1 --server-port 6742 --noautoconnect";
        Restart = "on-failure";
        RestartSec = 2;
      };
    };
    systemd.user.services.arc-openrgb-sync = lib.mkIf (!cfg.enableAi && cfg.fullAppSuite) {
      description = "Apply the wallpaper-derived ArcOS RGB profile";
      partOf = [ "arcos-desktop.target" ];
      after = [ "arc-openrgb.service" ];
      requires = [ "arc-openrgb.service" ];
      serviceConfig = {
        Type = "oneshot";
        ExecStart = "${pkgs.writeShellScript "arc-openrgb-sync-start" "sleep 2; exec ${arcosRgbSync}/bin/arcos-rgb-sync"}";
      };
    };
    systemd.user.services.arc-output-watch = {
      description = "ArcOS hotplug-aware output layout";
      partOf = [ "arcos-desktop.target" ];
      serviceConfig = {
        ExecStart = "${arcOutputWatch}/bin/arc-output-watch";
        Restart = "on-failure";
        RestartSec = 1;
      };
    };
    systemd.user.services.arc-swayidle = lib.mkIf (!cfg.enableAi) {
      description = "ArcOS idle and session lock integration";
      partOf = [ "arcos-desktop.target" ];
      serviceConfig = {
        ExecStart = "${pkgs.swayidle}/bin/swayidle -w timeout 600 '${arcosLock}/bin/arcos-lock' before-sleep '${arcosLock}/bin/arcos-lock' lock '${arcosLock}/bin/arcos-lock'";
        Restart = "on-failure";
        RestartSec = 2;
      };
    };
    systemd.user.services.arc-network-applet = {
      description = "ArcOS NetworkManager tray applet";
      partOf = [ "arcos-desktop.target" ];
      serviceConfig = {
        ExecStart = "${pkgs.networkmanagerapplet}/bin/nm-applet --indicator";
        Restart = "on-failure";
        RestartSec = 2;
      };
    };
    systemd.user.services.arc-bluetooth-applet = {
      description = "ArcOS Bluetooth tray applet";
      partOf = [ "arcos-desktop.target" ];
      serviceConfig = {
        ExecStart = "${pkgs.blueman}/bin/blueman-applet";
        Restart = "on-failure";
        RestartSec = 2;
      };
    };
    systemd.user.services.arc-polkit-agent = {
      description = "ArcOS graphical polkit agent";
      partOf = [ "arcos-desktop.target" ];
      serviceConfig = {
        ExecStart = "${pkgs.polkit_gnome}/libexec/polkit-gnome-authentication-agent-1";
        Restart = "on-failure";
        RestartSec = 2;
      };
    };

    systemd.services.swayosd-libinput-backend = lib.mkIf (!cfg.enableAi) {
      description = "SwayOSD hardware-key listener";
      wantedBy = [ "multi-user.target" ];
      after = [ "dbus.service" ];
      serviceConfig = {
        Type = "dbus";
        BusName = "org.erikreider.swayosd";
        ExecStart = "${pkgs.swayosd}/bin/swayosd-libinput-backend";
        Restart = "on-failure";
      };
    };

  };
}
