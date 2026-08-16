#!/usr/bin/env bash
set -uo pipefail

# Phase 4 deliberately begins with observation only. This script never invokes
# sudo, mount, btrfs subvolume create/delete, mkfs, bootctl, efibootmgr writes,
# limine-install, nixos-install, or any partitioning tool.

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

section() {
  printf '\n== %s ==\n' "$1"
}

try_run() {
  local label="$1"
  shift
  printf '\n-- %s --\n' "$label"
  if ! "$@" 2>&1; then
    printf '[unavailable without changing privileges]\n'
  fi
}

printf 'ArcOS Phase 4 preflight (READ-ONLY)\n'
printf 'Generated: %s\n' "$(date --iso-8601=seconds)"
printf 'Host: %s\n' "$(hostname)"

section "Boot mode and host"
if [[ -d /sys/firmware/efi ]]; then
  printf 'Firmware boot: UEFI\n'
else
  printf 'Firmware boot: legacy/unknown (UEFI directory absent)\n'
fi
try_run "Kernel" uname -a
try_run "CPU" lscpu
try_run "Memory" free -h

section "Block devices"
try_run "lsblk inventory" lsblk -o NAME,PATH,SIZE,TYPE,FSTYPE,FSVER,LABEL,UUID,PARTUUID,MOUNTPOINTS
for target in / /home /boot; do
  try_run "Mount for ${target}" findmnt -o TARGET,SOURCE,FSTYPE,OPTIONS --target "$target"
done
try_run "Filesystem capacity" df -h / /home /boot

section "Btrfs observation"
try_run "Root filesystem" btrfs filesystem usage /
try_run "Subvolume inventory" btrfs subvolume list -a /

section "Bootloader observation"
try_run "Readable boot tree" find /boot -maxdepth 3 -type f -printf '%p\n'
try_run "Limine-related files" find /boot -maxdepth 4 -type f \( -iname '*limine*' -o -iname 'limine.conf' \) -printf '%p\n'

section "Virtualization"
if [[ -c /dev/kvm && -r /dev/kvm && -w /dev/kvm ]]; then
  printf '/dev/kvm: usable\n'
else
  printf '/dev/kvm: unavailable to this user\n'
fi
try_run "Docker" docker info --format 'Server={{.ServerVersion}} Driver={{.Driver}} CPUs={{.NCPU}} Memory={{.MemTotal}}'

section "Graphics and displays"
try_run "PCI display devices" sh -c "lspci -nnk | grep -A4 -Ei 'VGA|3D|Display'"
if command -v nvidia-smi >/dev/null 2>&1; then
  try_run "NVIDIA runtime" nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader
else
  printf '\n-- NVIDIA runtime --\nnvidia-smi not available\n'
fi
if command -v kscreen-doctor >/dev/null 2>&1; then
  try_run "KScreen outputs" kscreen-doctor -o
elif command -v swaymsg >/dev/null 2>&1 && [[ -n "${SWAYSOCK:-}" ]]; then
  try_run "Sway outputs" swaymsg -r -t get_outputs
else
  printf '\n-- Display outputs --\nNo readable compositor output tool in this session\n'
fi

section "ArcOS artifacts"
iso_count=0
shopt -s nullglob
for image in "$repo_root"/artifacts/iso/*.iso; do
  iso_count=$((iso_count + 1))
  sha256sum "$image"
  stat -c '%n %s bytes' "$image"
done
shopt -u nullglob
if [[ "$iso_count" -eq 0 ]]; then
  printf 'No local ISO artifact found.\n'
fi

section "Hold point"
printf '%s\n' \
  'No state was changed by this preflight.' \
  'Do not create subvolumes, install NixOS, or edit EFI/Limine until the Phase 4 gates are signed off.'
