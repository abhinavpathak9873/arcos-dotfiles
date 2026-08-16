{
  pkgs,
  arcosModule,
  hermesPackage,
}:

pkgs.testers.runNixOSTest {
  name = "arcos-universal-desktop";

  nodes.machine =
    { pkgs, ... }:
    let
      arcKvmEvidence = pkgs.writeShellApplication {
        name = "arc-kvm-evidence";
        runtimeInputs = with pkgs; [
          coreutils
          procps
          systemd
        ];
        text = ''
          clear
          printf '\033[1;35mArcOS universal desktop KVM acceptance\033[0m\n\n'
          printf 'Profile:  4 vCPU / 4 GiB RAM\n'
          printf 'Kernel:   %s\n' "$(uname -r)"
          printf 'Memory:   %s\n' "$(free -h | awk '/Mem:/ { print $2 }')"
          printf 'Boot:     %s\n' "$(systemd-analyze time | head -n1)"
          printf '\n\033[1;32mPASS\033[0m Arc, Sway, native input and tiling are working.\n'
          printf 'The native shell and inspector test follows next.\n'
          sleep 60
        '';
      };
    in
    {
      imports = [
        arcosModule
        ../hardware/amd-mini-pc.nix
      ];
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
        user = "arc";
        inherit hermesPackage;
        autoLogin = true;
        enableHeadlessOutput = true;
        softwareRendering = true;
      };
      systemd.user.services.arc-speech.environment.ARC_PIPEWIRE_TARGET = "arc_e2e.monitor";
      virtualisation = {
        graphics = true;
        memorySize = 4096;
        cores = 4;
      };
      environment.systemPackages = with pkgs; [
        arcKvmEvidence
        espeak-ng
        ffmpeg
        imagemagick
        pulseaudio
        wf-recorder
      ];
      system.stateVersion = "26.05";
    };

  testScript = ''
    import json
    import os

    machine.start()
    machine.wait_for_unit("multi-user.target", timeout=60)
    machine.succeed(
      "runuser -u arc -- env XDG_RUNTIME_DIR=/run/user/1000 "
      "WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 "
      "sway --validate --config /etc/sway/config"
    )
    machine.wait_until_succeeds("pgrep -u arc -x sway", timeout=60)
    machine.sleep(2)
    machine.fail("pgrep -u arc -f '[s]waynag'")

    user_env = (
      "runuser -u arc -- env XDG_RUNTIME_DIR=/run/user/1000 "
      "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus "
    )
    sway_env = (
      user_env
      + "WAYLAND_DISPLAY=wayland-1 "
      + "SWAYSOCK=$(find /run/user/1000 -maxdepth 1 -name 'sway-ipc.*.sock' -print -quit) "
    )

    for unit in ["arc-core.service", "arc-speech.service", "arc-shell.service", "hermes.service"]:
      machine.wait_until_succeeds(user_env + f"systemctl --user is-active {unit}", timeout=60)
    machine.wait_until_succeeds("systemctl is-active keyd.service", timeout=30)
    machine.wait_until_succeeds(
      user_env + "systemctl --user is-active arcos-desktop.target",
      timeout=30,
    )
    machine.wait_until_succeeds(
      user_env + "systemctl --user is-active arc-waybar.service",
      timeout=30,
    )
    machine.wait_until_succeeds(
      user_env
      + "sh -c 'test $(systemctl --user show -p MainPID --value arc-waybar.service) -gt 0'",
      timeout=30,
    )
    machine.wait_until_succeeds(
      "pgrep -u arc -f 'arcctl waybar --watch'",
      timeout=30,
    )
    machine.wait_until_succeeds(
      user_env + "systemctl --user is-active arc-mako.service",
      timeout=30,
    )
    machine.wait_until_succeeds(
      user_env
      + "sh -c 'test $(systemctl --user show -p MainPID --value arc-mako.service) -gt 0'",
      timeout=30,
    )
    machine.succeed("grep -q 'overload(arcos, f23)' /etc/keyd/default.conf")
    machine.succeed("grep -q 'space=f24' /etc/keyd/default.conf")
    machine.wait_until_succeeds("test -S /run/user/1000/arc/arc-core.sock", timeout=30)
    machine.succeed(user_env + "arcctl status >/dev/null")
    machine.succeed("awk '/MemTotal/ { exit !($2 >= 3500000 && $2 <= 5000000) }' /proc/meminfo")
    machine.succeed("test -e /dev/dri/card0")
    machine.succeed("test -x /run/current-system/sw/bin/arc-hardware-report")

    machine.succeed(
      sway_env + "arc-hardware-report /tmp/arc-hardware-reports | tee /tmp/report-location"
    )
    report_dir = machine.succeed(
      "find /tmp/arc-hardware-reports -mindepth 1 -maxdepth 1 -type d -print -quit"
    ).strip()
    report = json.loads(machine.succeed(f"cat {report_dir}/self-test.json"))
    assert report["memoryKiB"] >= 3500000
    assert report["graphicalOutput"] is True
    machine.succeed(f"test -s {report_dir}/pci.txt")
    machine.succeed(f"test -s {report_dir}/displays.txt")

    # Exercise the complete local capture -> VAD -> whisper.cpp path through a
    # PipeWire-Pulse monitor source. Only the decoded text reaches arc-core.
    machine.succeed(
      "espeak-ng -w /tmp/arc-voice-raw.wav "
      "'hello arc this is the local voice capture acceptance test'"
    )
    machine.succeed(
      "ffmpeg -v error -y -i /tmp/arc-voice-raw.wav -ar 16000 -ac 1 "
      "/tmp/arc-voice-test.wav"
    )
    original_source = machine.succeed(user_env + "pactl get-default-source").strip()
    sink_module = machine.succeed(
      user_env
      + "pactl load-module module-null-sink sink_name=arc_e2e "
      + "sink_properties=device.description=Arc_E2E"
    ).strip()
    try:
      machine.wait_until_succeeds(
        user_env + "pactl get-source-mute arc_e2e.monitor",
        timeout=15,
      )
      machine.succeed(user_env + "pactl set-default-source arc_e2e.monitor")
      started = json.loads(machine.succeed(user_env + "arcctl voice toggle"))
      machine.sleep(1)
      machine.succeed(user_env + "paplay --device=arc_e2e /tmp/arc-voice-test.wav")
      machine.sleep(1)
      finished = json.loads(
        machine.succeed(user_env + "arcctl voice toggle", timeout=120)
      )
      assert finished["stable"] is True
      assert finished["utteranceId"] == started["utteranceId"]
      assert finished["text"].strip()
    finally:
      machine.succeed(user_env + f"pactl set-default-source {original_source}")
      machine.succeed(user_env + f"pactl unload-module {sink_module}")

    machine.sleep(2)
    machine.screenshot("00-arcos-desktop")

    # Record a deliberate, paced showcase from the guest's real Wayland
    # output. Keep a high-quality H.264 master as the acceptance artifact. The
    # sequence begins with the desktop itself, then shows
    # listening, interruption, the activity sheet, native inspector, normal
    # application coexistence, the text prompt, and hard-stop feedback.
    machine.succeed(user_env + "arcctl collapse")
    machine.succeed(
      user_env
      + "systemd-run --user --unit=arc-kvm-recording --collect "
      + "wf-recorder -D -o Virtual-1 -c libx264 -p preset=veryfast -p crf=18 "
      + "-r 30 -f /tmp/arcos-ui-showcase.mp4"
    )
    machine.sleep(3)

    showcase_source = machine.succeed(user_env + "pactl get-default-source").strip()
    showcase_sink = machine.succeed(
      user_env
      + "pactl load-module module-null-sink sink_name=arc_e2e "
      + "sink_properties=device.description=Arc_Showcase"
    ).strip()
    try:
      machine.wait_until_succeeds(
        user_env + "pactl get-source-mute arc_e2e.monitor",
        timeout=15,
      )
      machine.succeed(user_env + "pactl set-default-source arc_e2e.monitor")
      machine.succeed(user_env + "arcctl voice toggle")
      machine.sleep(4)
      machine.succeed(user_env + "arcctl stop >/dev/null")
      machine.sleep(3)
    finally:
      machine.succeed(user_env + f"pactl set-default-source {showcase_source}")
      machine.succeed(user_env + f"pactl unload-module {showcase_sink}")

    # Arc has no primary application window. Open the layer-shell sheet only
    # when deliberately requested, then verify it did not change workspace.
    workspace_before = machine.succeed(sway_env + "swaymsg -r -t get_workspaces | jq -r '.[] | select(.focused).name'").strip()
    machine.succeed(user_env + "arcctl toggle")
    machine.sleep(5)
    workspace_after = machine.succeed(sway_env + "swaymsg -r -t get_workspaces | jq -r '.[] | select(.focused).name'").strip()
    assert workspace_after == workspace_before
    machine.screenshot("01-arc-expanded-sheet")

    # The dense inspector is a native GTK window and appears only when asked.
    machine.succeed(user_env + "arcctl collapse")
    machine.sleep(3)
    machine.succeed(
      sway_env
      + "swaymsg exec \"sh -c '/run/current-system/sw/bin/arc-inspector >/tmp/arc-inspector.log 2>&1'\""
    )
    machine.sleep(2)
    machine.wait_until_succeeds("pgrep -u arc -f '[a]rc-inspector'", timeout=15)
    machine.wait_until_succeeds(
      sway_env + "sh -c \"swaymsg -r -t get_tree | jq -e '.. | objects | select(.app_id == \\\"ai.arcos.inspector\\\")' >/dev/null\"",
      timeout=60,
    )
    machine.sleep(5)
    machine.screenshot("02-native-inspector")
    machine.succeed(
      sway_env + "swaymsg exec 'kitty --title ArcOS-KVM-Test arc-kvm-evidence'"
    )
    machine.wait_until_succeeds(
      sway_env
      + "sh -c \"swaymsg -r -t get_tree | jq -e "
      + "'.. | objects | select(.name == \\\"ArcOS-KVM-Test\\\")' >/dev/null\"",
      timeout=15,
    )
    machine.sleep(5)
    machine.screenshot("03-terminal-and-inspector")
    machine.send_key("alt-f4")
    machine.sleep(3)

    machine.send_key("alt-f4")
    machine.sleep(3)
    machine.wait_until_succeeds(
      user_env
      + "sh -c 'systemctl --user is-active arc-core.service && systemctl --user is-active arc-speech.service && systemctl --user is-active hermes.service'",
      timeout=30,
    )

    machine.succeed(user_env + "arc-text-prompt")
    machine.sleep(1)
    machine.send_chars("Summarize this project and show me what changed")
    machine.sleep(5)
    machine.screenshot("04-caps-text-prompt")
    machine.send_key("esc")
    machine.sleep(3)

    machine.succeed(
      user_env
      + "systemd-run --user --unit=arc-test-agent --property=PartOf=arcos-agent.target sleep infinity"
    )
    machine.succeed(user_env + "systemctl --user start arcos-agent.target")
    machine.succeed(
      user_env
      + "sh -c 'started=$(date +%s%N); arcctl stop >/dev/null; "
      + "elapsed=$(( $(date +%s%N) - started )); test $elapsed -lt 100000000'"
    )
    machine.wait_until_succeeds(
      user_env + "sh -c '! systemctl --user is-active --quiet arc-test-agent.service'",
      timeout=5,
    )
    machine.sleep(4)
    machine.succeed(user_env + "systemctl --user stop arc-kvm-recording.service")
    machine.wait_until_succeeds("test -s /tmp/arcos-ui-showcase.mp4", timeout=15)
    machine.succeed(
      "ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 "
      "/tmp/arcos-ui-showcase.mp4 | awk '{ exit !($1 >= 35) }'"
    )
    machine.succeed(
      "ffprobe -v error -select_streams v:0 -show_entries stream=width,height,codec_name "
      "-of json /tmp/arcos-ui-showcase.mp4 "
      "| jq -e '.streams[0] | .codec_name == \"h264\" and .width == 1280 and .height == 800'"
    )
    machine.fail("pgrep -u arc -f '[e]lectron'")
    machine.fail("pgrep -u arc -f '[W]ebKitWebProcess'")
    machine.fail("pgrep -u arc -f '[t]auri'")

    out_dir = os.environ.get("out", os.getcwd())
    machine.copy_from_machine("/tmp/arcos-ui-showcase.mp4", out_dir)
    machine.copy_from_machine("/tmp/arc-hardware-reports", out_dir)
  '';
}
