{
  config,
  lib,
  pkgs,
  ...
}:

let
  arcHardwareReport = pkgs.writeShellApplication {
    name = "arc-hardware-report";
    runtimeInputs = with pkgs; [
      bluez
      coreutils
      ethtool
      findutils
      gawk
      gnugrep
      gnused
      gnutar
      gzip
      iproute2
      jq
      lm_sensors
      networkmanager
      pciutils
      pipewire
      procps
      sway
      systemd
      usbutils
      util-linux
      vulkan-tools
      wireplumber
    ];
    text = ''
            output_root="''${1:-''${HOME}/ArcOS-Hardware-Reports}"
            stamp="$(date -u +%Y%m%dT%H%M%SZ)"
            report_dir="$output_root/arcos-hardware-$stamp"
            mkdir -p "$report_dir"

            capture() {
              name="$1"
              shift
              {
                printf '$'
                printf ' %q' "$@"
                printf '\n\n'
                "$@"
              } >"$report_dir/$name.txt" 2>&1 || true
            }

            boot_mode="legacy"
            if [ -d /sys/firmware/efi ]; then
              boot_mode="uefi"
            fi
            cpu_model="$(lscpu | awk -F: '/Model name/ { sub(/^[[:space:]]+/, "", $2); print $2; exit }')"
            memory_kib="$(awk '/MemTotal/ { print $2 }' /proc/meminfo)"
            memory_gib="$(awk -v kib="$memory_kib" 'BEGIN { printf "%.1f", kib / 1048576 }')"
            amd_cpu=false
            if grep -qi 'AuthenticAMD' /proc/cpuinfo; then
              amd_cpu=true
            fi
            graphical_output=false
            if swaymsg -r -t get_outputs 2>/dev/null | jq -e 'any(.[]; .active == true)' >/dev/null 2>&1; then
              graphical_output=true
            fi

            cat >"$report_dir/summary.txt" <<EOF
      ArcOS portable x86_64 hardware report
      Generated (UTC): $stamp
      Hostname: $(hostname)
      Boot mode: $boot_mode
      CPU: $cpu_model
      AMD CPU detected: $amd_cpu
      Memory: $memory_gib GiB
      Kernel: $(uname -r)
      Active graphical output: $graphical_output

      This report is local and is never uploaded automatically. Review it before sharing;
      some command output can contain device identifiers.
      EOF

            jq -n \
              --arg generated "$stamp" \
              --arg bootMode "$boot_mode" \
              --arg cpu "$cpu_model" \
              --arg kernel "$(uname -r)" \
              --argjson memoryKiB "$memory_kib" \
              --argjson amdCpu "$amd_cpu" \
              --argjson graphicalOutput "$graphical_output" \
              '{generatedUtc: $generated, bootMode: $bootMode, cpu: $cpu, kernel: $kernel, memoryKiB: $memoryKiB, amdCpu: $amdCpu, graphicalOutput: $graphicalOutput}' \
              >"$report_dir/self-test.json"

            capture cpu lscpu
            capture memory free -h
            capture kernel uname -a
            capture boot systemd-analyze time
            capture storage lsblk -e 7 -o NAME,PATH,SIZE,TYPE,FSTYPE,LABEL,MODEL,TRAN,MOUNTPOINTS
            capture pci lspci -nnk
            capture usb lsusb -t
            capture network nmcli --terse --fields DEVICE,TYPE,STATE device status
            capture links ip -brief link
            capture radio rfkill list
            capture audio wpctl status
            capture displays swaymsg -r -t get_outputs
            capture graphics vulkaninfo --summary
            capture sensors sensors
            capture failed-units systemctl --failed --no-legend
            capture warnings journalctl -b -p warning..alert --no-pager

            archive="$report_dir.tar.gz"
            tar -C "$output_root" -czf "$archive" "$(basename "$report_dir")"
            printf 'REPORT_DIR=%s\nARCHIVE=%s\n' "$report_dir" "$archive"
    '';
  };
in
{
  # Broad boot coverage for common mini-PC and workstation storage, USB, and
  # SATA/NVMe controllers. A generated installed profile narrows this after the
  # first physical boot.
  boot.initrd.availableKernelModules = lib.mkAfter [
    "ahci"
    "nvme"
    "sd_mod"
    "uas"
    "usb_storage"
    "usbhid"
    "xhci_pci"
  ];
  hardware.enableRedistributableFirmware = true;
  hardware.graphics.enable = true;
  services.fstrim.enable = true;
  services.fwupd.enable = true;

  environment.systemPackages = with pkgs; [
    arcHardwareReport
    ethtool
    libva-utils
    lm_sensors
    mesa-demos
    pciutils
    usbutils
    vulkan-tools
  ];

  environment.etc."sway/config.d/20-amd-mini-pc.conf".text = ''
    bindsym $mod+Shift+h exec ${arcHardwareReport}/bin/arc-hardware-report
  '';
}
