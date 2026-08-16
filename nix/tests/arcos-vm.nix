{
  pkgs,
  arcosModule,
  hermesPackage,
}:

pkgs.testers.runNixOSTest {
  name = "arcos-host-control";

  nodes.machine = { pkgs, ... }: {
    imports = [ arcosModule ];
    # The 6.18 test kernel currently trips a QEMU/KVM CPUID dependency bug on
    # Zen 3 hosts. Keep the acceptance guest on the long-term 6.12 kernel so a
    # host-kernel/QEMU quirk cannot masquerade as an ArcOS service failure.
    boot.kernelPackages = pkgs.linuxPackages_6_12;
    users.users.arc = {
      isNormalUser = true;
      extraGroups = [
        "audio"
        "video"
        "input"
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
    virtualisation.graphics = true;
    environment.systemPackages = [ pkgs.imagemagick ];
    system.stateVersion = "26.05";
  };

  testScript = ''
    import json

    machine.start(allow_reboot=True)
    machine.wait_for_unit("multi-user.target")
    machine.succeed("test -x /run/current-system/sw/bin/arc-core")
    machine.succeed("test -x /run/current-system/sw/bin/arcctl")
    machine.succeed("test -x /run/current-system/sw/bin/arc-shell")
    machine.succeed("test -x /run/current-system/sw/bin/arc-speech")
    machine.succeed("test -x /run/current-system/sw/bin/arc-codex")
    machine.succeed("test -x /run/current-system/sw/bin/arc-inspector")
    machine.succeed("test -x /run/current-system/sw/bin/hermes")
    machine.succeed("test -r /etc/fonts/fonts.conf")
    machine.succeed("hermes version | grep -q 'Hermes Agent'")
    machine.succeed("grep -q 'seat agent-seat fallback false' /etc/sway/config")
    machine.succeed("grep -Fq 'bindsym $mod+Escape' /etc/sway/config")
    machine.succeed("grep -q 'workspace 90:arc-background output HEADLESS-1' /etc/sway/config")
    machine.succeed("grep -Fq 'for_window [app_id=\"ai.arcos.inspector\"] floating enable' /etc/sway/config.d/10-arcos.conf")
    machine.succeed("grep -Fq 'bindsym $mod+a' /etc/sway/config")
    machine.fail("test -e /run/current-system/sw/bin/arc-workbench")
    machine.succeed(
      "runuser -u arc -- env XDG_RUNTIME_DIR=/run/user/1000 "
      "WLR_BACKENDS=headless WLR_RENDERER=pixman sway --validate --config /etc/sway/config"
    )

    machine.wait_until_succeeds("pgrep -u arc -x sway", timeout=60)
    user_env = (
      "runuser -u arc -- env XDG_RUNTIME_DIR=/run/user/1000 "
      "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus "
    )
    sway_env = (
      user_env +
      "SWAYSOCK=$(find /run/user/1000 -maxdepth 1 -name 'sway-ipc.*.sock' -print -quit) "
    )
    for unit in ["arc-core.service", "arc-speech.service", "arc-shell.service", "hermes.service"]:
      machine.wait_until_succeeds(user_env + f"systemctl --user is-active {unit}", timeout=60)
    machine.wait_until_succeeds("test -S /run/user/1000/arc/arc-core.sock", timeout=30)
    machine.succeed(user_env + "arcctl status | grep -q '\"status\": \"ready\"'")

    response = machine.succeed(
      "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}' | arc-core serve-stdio"
    )
    frame = json.loads(response)
    assert frame["result"]["protocolVersion"] == 3
    assert frame["result"]["identity"] == "arc"
    assert frame["result"]["kernel"] == "hermes"

    machine.succeed(
      "state=$(mktemp -d); "
      "printf '%s\\n' "
      "'{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"rooms/create\",\"params\":{\"name\":\"VM acceptance\"}}' "
      "'{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"actions/record\",\"params\":{\"actor\":\"arc\",\"action\":\"vm.verify\",\"target\":\"arcos-vm\",\"outcome\":\"succeeded\",\"reversible\":true,\"permission\":\"allowed\",\"detail\":\"NixOS VM test\"}}' "
      "'{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"audit/verify\",\"params\":{}}' "
      "| ARC_STATE_DIR=$state arc-core serve-stdio | tee /tmp/arc-responses; "
      "grep -q '\"valid\":true' /tmp/arc-responses"
    )

    old_pid = machine.succeed(user_env + "systemctl --user show -p MainPID --value arc-core.service").strip()
    machine.succeed(f"kill -9 {old_pid}")
    machine.wait_until_succeeds(user_env + f"sh -c 'pid=$(systemctl --user show -p MainPID --value arc-core.service); test \"$pid\" != 0 && test \"$pid\" != {old_pid}'", timeout=30)
    machine.wait_until_succeeds(user_env + "arcctl status >/dev/null", timeout=30)

    machine.succeed(
      user_env
      + "arcctl rooms/create '{\"name\":\"Reboot persistence\"}' "
      + ">/tmp/arc-persistent-room.json"
    )
    old_hermes_pid = machine.succeed(
      user_env + "systemctl --user show -p MainPID --value hermes.service"
    ).strip()
    machine.succeed(f"kill -9 {old_hermes_pid}")
    machine.wait_until_succeeds(
      user_env
      + "sh -c 'pid=$(systemctl --user show -p MainPID --value hermes.service); "
      + f"test \"$pid\" != 0 && test \"$pid\" != {old_hermes_pid}'",
      timeout=30,
    )
    machine.succeed(user_env + "systemctl --user is-active arc-core.service")

    # A graphical session may consume the synthetic Ctrl+Alt+Delete used by the
    # driver. Ask PID 1 to reboot first, then let reboot() prepare reconnection.
    machine.succeed(
      "systemd-run --unit=arc-test-reboot --on-active=2s systemctl reboot"
    )
    machine.reboot()
    machine.wait_for_unit("multi-user.target")
    machine.wait_until_succeeds("pgrep -u arc -x sway", timeout=60)
    for unit in ["arc-core.service", "arc-speech.service", "arc-shell.service", "hermes.service"]:
      machine.wait_until_succeeds(user_env + f"systemctl --user is-active {unit}", timeout=60)
    machine.wait_until_succeeds("test -S /run/user/1000/arc/arc-core.sock", timeout=30)
    machine.succeed(user_env + "arcctl rooms/list | grep -q 'Reboot persistence'")
  '';
}
