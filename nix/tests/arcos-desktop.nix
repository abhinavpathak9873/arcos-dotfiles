{
  pkgs,
  arcosModule,
  hermesPackage,
}:

pkgs.testers.runNixOSTest {
  name = "arcos-standalone-desktop";

  nodes.machine = { pkgs, ... }: {
    imports = [ arcosModule ];
    boot.kernelPackages = pkgs.linuxPackages_6_12;
    networking.networkmanager.enable = true;
    users.users.arc = {
      isNormalUser = true;
      extraGroups = [
        "audio"
        "video"
        "input"
        "networkmanager"
      ];
    };
    services.arcos = {
      enable = true;
      enableAi = false;
      fullAppSuite = false;
      user = "arc";
      autoLogin = true;
      enableHeadlessOutput = false;
      softwareRendering = true;
      inherit hermesPackage;
    };
    virtualisation = {
      graphics = true;
      memorySize = 4096;
      cores = 4;
    };
    environment.systemPackages = [
      pkgs.espeak-ng
      pkgs.imagemagick
      pkgs.pulseaudio
    ];
    system.stateVersion = "26.05";
  };

  testScript = ''
    machine.start()
    machine.wait_for_unit("multi-user.target", timeout=60)
    machine.wait_until_succeeds("pgrep -u arc -x sway", timeout=60)

    user_env = (
      "runuser -u arc -- env HOME=/home/arc XDG_CONFIG_HOME=/home/arc/.config "
      "XDG_RUNTIME_DIR=/run/user/1000 "
      "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus "
    )
    sway_env = (
      user_env
      + "WAYLAND_DISPLAY=wayland-1 "
      + "SWAYSOCK=$(find /run/user/1000 -maxdepth 1 -name 'sway-ipc.*.sock' -print -quit) "
    )

    for unit in [
      "arcos-desktop.target",
      "arc-waybar.service",
      "arc-clipboard-images.service",
      "arc-clipboard-text.service",
      "swaync.service",
      "arc-swayosd.service",
      "arc-elephant.service",
      "arc-walker.service",
      "arc-voxtype.service",
    ]:
      machine.wait_until_succeeds(
        user_env + f"systemctl --user is-active {unit}",
        timeout=30,
      )
    machine.fail(user_env + "systemctl --user is-active arc-core.service")
    machine.fail(user_env + "systemctl --user is-active arc-speech.service")
    machine.fail(user_env + "systemctl --user is-active arc-shell.service")
    machine.fail(user_env + "systemctl --user is-active hermes.service")
    machine.fail("test -e /run/current-system/sw/bin/arcctl")
    machine.succeed("test -s /etc/keyd/default.conf")

    for generated in ["theme.json", "waybar.css", "rofi.rasi", "mako.conf", "kitty.conf", "gtk.css", "apps.css", "overview.css", "shortcuts.css", "gtklock.css", "swaync.css", "swayosd.css", "nwg-bar.css"]:
      machine.succeed(f"test -s /home/arc/.config/arcos-desktop/{generated}")
    machine.succeed("test -s /home/arc/.config/tmux/arcos-theme.conf")
    machine.succeed("test -s /home/arc/.config/nvim/arcos-theme.lua")
    machine.succeed("test -s /home/arc/.config/walker/config.toml")
    machine.succeed("test -s /home/arc/.config/walker/themes/arcos/style.css")
    machine.succeed("test -s /home/arc/.config/omarchy/current/theme/colors.toml")
    machine.succeed("test -s /home/arc/.config/gtk-3.0/gtk.css")
    machine.succeed("test -s /home/arc/.config/gtk-4.0/gtk.css")
    machine.succeed("test -s /home/arc/.config/swaync/style.css")
    machine.succeed("test -s /home/arc/.config/swayosd/style.css")
    machine.succeed("grep -Fq 'bindsym --to-code --no-repeat $mod+space' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code --no-repeat $mod+a' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code --no-repeat $mod+w' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code --no-repeat $mod+comma' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code --no-repeat $mod+n' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code --no-repeat $mod+v' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code --no-repeat $mod+Shift+k' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code --no-repeat F23' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code --release F23' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code $mod+Left focus left' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code $mod+Shift+Right move right' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code --no-repeat Mod1+Tab focus next' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym --to-code --no-repeat $mod+Tab focus next' /etc/sway/config")
    machine.succeed("grep -Fq 'output * adaptive_sync on' /etc/sway/config")
    machine.succeed("grep -Fq 'gaps outer 8' /etc/sway/config")
    machine.succeed("grep -Fq 'smart_gaps off' /etc/sway/config")
    machine.succeed("grep -Fq 'corner_radius 10' /etc/sway/config")
    machine.succeed("grep -Fq 'shadows enable' /etc/sway/config")
    machine.succeed("grep -Fq 'include $HOME/.config/sway/config.d/*' /etc/sway/config")
    machine.succeed("grep -q '^force_keyboard_focus = true$' /home/arc/.config/walker/config.toml")
    machine.succeed("grep -Fq 'empty = [\"windows\"]' /home/arc/.config/walker/config.toml")
    machine.succeed("grep -Fq 'set $ws1 \"1\"' /etc/sway/config")
    machine.fail("grep -q '1:Main' /etc/sway/config")
    machine.fail("grep -q 'arcctl' /etc/sway/config")
    machine.succeed("grep -q '\"format\": \"󰀻  Applications\"' /etc/xdg/waybar/config.jsonc")
    machine.succeed("grep -q '\"on-click\": \"arcos-apps\"' /etc/xdg/waybar/config.jsonc")
    machine.succeed("grep -q '\"format\": \"\"' /etc/xdg/waybar/config.jsonc")
    machine.fail("grep -q 'persistent-workspaces' /etc/xdg/waybar/config.jsonc")
    machine.succeed("grep -q '\"cpu\"' /etc/xdg/waybar/config.jsonc")
    machine.succeed("grep -q '\"memory\"' /etc/xdg/waybar/config.jsonc")
    machine.succeed("grep -q '\"custom/gpu\"' /etc/xdg/waybar/config.jsonc")
    machine.succeed("grep -q '\"custom/caffeine\"' /etc/xdg/waybar/config.jsonc")
    machine.succeed("grep -q '\"group/tray-expander\"' /etc/xdg/waybar/config.jsonc")
    machine.succeed(
      "jq -e '.\"control-center-positionX\" == \"right\" and "
      ".\"control-center-positionY\" == \"top\" and .positionX == \"center\"' "
      "/etc/xdg/swaync/config.json"
    )
    machine.succeed("test -d /run/current-system/sw/share/icons/Papirus-Dark")
    machine.succeed(user_env + "test \"$(${pkgs.dconf}/bin/dconf read /org/gnome/desktop/interface/icon-theme)\" = \"'Papirus-Dark'\"")
    machine.succeed("jq -e '.source == \"wallpaper\"' /home/arc/.config/arcos-desktop/theme.json")
    machine.succeed(sway_env + "swaymsg -r -t get_outputs | jq -e '.[] | select(.active)' >/dev/null")
    machine.sleep(3)
    machine.succeed(
      user_env
      + "sh -c 'test \"$(systemctl --user show -p NRestarts --value arc-waybar.service)\" = 0'"
    )
    waybar_pid = machine.succeed(
      user_env + "systemctl --user show -p MainPID --value arc-waybar.service"
    ).strip()
    machine.screenshot("00-standalone-desktop")

    machine.succeed(user_env + "arcos-theme color '#7dcfff'")
    machine.succeed("grep -q '#7dcfff' /home/arc/.config/arcos-desktop/theme.json")
    machine.wait_until_succeeds(
      user_env + "systemctl --user is-active arc-waybar.service",
      timeout=30,
    )
    machine.wait_until_succeeds(
      user_env
      + "sh -c 'test $(systemctl --user show -p MainPID --value arc-waybar.service) -gt 0'",
      timeout=30,
    )
    machine.succeed(
      user_env
      + f"sh -c 'test \"$(systemctl --user show -p MainPID --value arc-waybar.service)\" = {waybar_pid}'"
    )
    machine.succeed(
      user_env
      + "sh -c 'test \"$(systemctl --user show -p NRestarts --value arc-waybar.service)\" = 0'"
    )

    # Return to automatic wallpaper matching before visual evidence is captured.
    machine.succeed(user_env + "arcos-theme auto")
    machine.succeed("jq -e '.source == \"wallpaper\" and .source_accent != \"#7dcfff\"' /home/arc/.config/arcos-desktop/theme.json")

    # Put a real app on another workspace before opening Spotlight. An empty
    # query must list running windows across every workspace, and activating
    # the result must take us to that existing window rather than spawn a copy.
    machine.send_key("meta_l-ret")
    machine.wait_until_succeeds(
      sway_env + "swaymsg -r -t get_tree | jq -e '.. | objects | select(.app_id? == \"kitty\")' >/dev/null",
      timeout=20,
    )
    machine.send_key("meta_l-shift-2")
    machine.wait_until_succeeds(
      sway_env + "swaymsg -r -t get_tree | jq -e '.. | objects | select(.type? == \"workspace\" and .num? == 2) | .. | objects | select(.app_id? == \"kitty\")' >/dev/null",
      timeout=10,
    )
    machine.send_key("meta_l-spc")
    machine.sleep(2)
    machine.screenshot("01-universal-search")
    machine.send_key("ret")
    machine.wait_until_succeeds(
      sway_env + "swaymsg -r -t get_workspaces | jq -e '.[] | select(.focused and .num == 2)' >/dev/null",
      timeout=10,
    )

    # Typed Spotlight results launch normal desktop applications as well.
    machine.send_key("meta_l-1")
    machine.send_key("meta_l-spc")
    machine.sleep(1)
    machine.send_chars("nautilus")
    machine.sleep(1)
    machine.send_key("ret")
    machine.wait_until_succeeds(
      sway_env + "swaymsg -r -t get_tree | jq -e '.. | objects | select(.app_id? == \"org.gnome.Nautilus\")' >/dev/null",
      timeout=20,
    )

    # Workspace shortcuts remain compositor-owned while applications have focus.
    machine.send_key("meta_l-2")
    machine.wait_until_succeeds(
      sway_env + "swaymsg -r -t get_workspaces | jq -e '.[] | select(.focused and .num == 2)' >/dev/null",
      timeout=10,
    )
    machine.send_key("meta_l-1")
    machine.wait_until_succeeds(
      sway_env + "swaymsg -r -t get_workspaces | jq -e '.[] | select(.focused and .num == 1)' >/dev/null",
      timeout=10,
    )
    machine.send_key("meta_l-a")
    machine.wait_until_succeeds("pgrep -u arc -f 'arcos-apps.py'", timeout=15)
    machine.wait_until_succeeds("test -e /run/user/1000/arcos-apps.ready", timeout=15)
    machine.sleep(1)
    machine.log(machine.succeed("cat /run/user/1000/arcos-apps.log 2>/dev/null || true"))
    machine.succeed("pgrep -u arc -f 'arcos-apps.py'")
    machine.screenshot("02-application-grid")
    machine.send_key("esc")
    machine.wait_until_fails("pgrep -u arc -f 'arcos-apps.py'", timeout=10)

    # The two real windows above let the overview prove its cross-workspace
    # layout previews, grouped app rows, icons, and direct app targeting.
    machine.send_key("meta_l-w")
    machine.wait_until_succeeds("pgrep -u arc -f 'arcos-overview.py'", timeout=15)
    machine.wait_until_succeeds("test -e /run/user/1000/arcos-overview.ready", timeout=15)
    machine.sleep(1)
    machine.log(machine.succeed("cat /run/user/1000/arcos-overview.log 2>/dev/null || true"))
    machine.succeed("pgrep -u arc -f 'arcos-overview.py'")
    machine.screenshot("03-workspace-overview")
    machine.send_key("esc")
    machine.wait_until_fails("pgrep -u arc -f 'arcos-overview.py'", timeout=10)

    machine.succeed(user_env + "notify-send 'ArcOS v1.3' 'Notifications stay below the panel and above your work'")
    machine.sleep(1)
    machine.screenshot("04-notification")

    machine.send_key("meta_l-n")
    machine.sleep(2)
    machine.screenshot("05-quick-controls")
    machine.send_key("meta_l-n")

    # Exercise the real Voxtype daemon and GTK layer-shell waveform using a
    # PipeWire virtual microphone. The screenshot is taken while speech is
    # actively flowing, and the decoded result must be written afterwards.
    machine.succeed(
      "espeak-ng -s 125 -w /tmp/voxtype-demo.wav "
      "'ArcOS dictation is listening and typing locally into the focused application'"
    )
    original_source = machine.succeed(user_env + "pactl get-default-source").strip()
    dictation_sink = machine.succeed(
      user_env
      + "pactl load-module module-null-sink sink_name=voxtype_demo "
      + "sink_properties=device.description=ArcOS_Dictation_Demo"
    ).strip()
    try:
      machine.wait_until_succeeds(
        user_env + "pactl get-source-mute voxtype_demo.monitor",
        timeout=15,
      )
      machine.succeed(user_env + "pactl set-default-source voxtype_demo.monitor")
      machine.succeed(user_env + "voxtype record start --file=/tmp/voxtype-demo.txt")
      machine.succeed(
        user_env
        + "systemd-run --user --unit=voxtype-demo-audio --collect "
        + "paplay --device=voxtype_demo /tmp/voxtype-demo.wav"
      )
      machine.sleep(1)
      machine.screenshot("10-voxtype-dictation")
      machine.sleep(4)
      machine.succeed(user_env + "voxtype record stop", timeout=120)
      machine.wait_until_succeeds("test -s /tmp/voxtype-demo.txt", timeout=120)
    finally:
      machine.succeed(user_env + f"pactl set-default-source {original_source}")
      machine.succeed(user_env + f"pactl unload-module {dictation_sink}")

    machine.send_key("meta_l-esc")
    machine.log(machine.succeed("cat /run/user/1000/arcos-power-menu.log 2>/dev/null || true"))
    machine.wait_until_succeeds("pgrep -u arc -f 'arcos-power-menu.py'", timeout=15)
    machine.wait_until_succeeds("test -e /run/user/1000/arcos-power-menu.ready", timeout=15)
    machine.sleep(1)
    machine.screenshot("06-power-menu")
    machine.send_key("esc")
    machine.wait_until_fails("pgrep -u arc -f 'arcos-power-menu.py'", timeout=10)

    machine.send_key("meta_l-shift-k")
    machine.wait_until_succeeds("pgrep -u arc -f 'arcos-shortcuts.py'", timeout=15)
    machine.wait_until_succeeds("test -e /run/user/1000/arcos-shortcuts.ready", timeout=15)
    machine.sleep(1)
    machine.screenshot("07-shortcut-guide")
    machine.send_key("esc")
    machine.wait_until_fails("pgrep -u arc -f 'arcos-shortcuts.py'", timeout=10)

    machine.succeed(sway_env + "swaymsg exec arcos-lock")
    machine.wait_until_succeeds("pgrep -u arc -f '/gtklock'", timeout=15)
    machine.log(machine.succeed("cat /run/user/1000/arcos-lock.log 2>/dev/null || true"))
    machine.sleep(2)
    machine.screenshot("08-lock-screen")
    machine.send_key("spc")
    machine.sleep(1)
    machine.screenshot("09-lock-login")
    machine.succeed("pkill -u arc -f '/gtklock'")
    machine.wait_until_fails("pgrep -u arc -f '/gtklock'", timeout=10)

    # Meta+Q is compositor-owned and closes immediately while Kitty is active.
    machine.succeed(sway_env + "swaymsg '[app_id=\"kitty\"] focus'")
    machine.send_key("meta_l-q")
    machine.wait_until_fails(
      sway_env + "swaymsg -r -t get_tree | jq -e '.. | objects | select(.app_id? == \"kitty\")' >/dev/null",
      timeout=10,
    )
  '';
}
