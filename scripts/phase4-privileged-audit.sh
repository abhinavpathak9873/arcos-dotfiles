#!/usr/bin/env bash
set -euo pipefail

# This performs privileged observation and backup only. It contains no mount,
# partition, mkfs, btrfs create/delete, bootloader write, or install command.

if [[ "$EUID" -ne 0 ]]; then
  printf 'Run with sudo and an already-mounted EXTERNAL backup directory:\n' >&2
  printf '  sudo %s /path/on/external/drive\n' "$0" >&2
  exit 1
fi
if [[ "$#" -ne 1 || ! -d "$1" ]]; then
  printf 'An existing external backup directory is required.\n' >&2
  exit 2
fi

backup_root="$(readlink -f -- "$1")"
root_device_id="$(findmnt -nro MAJ:MIN --target /)"
backup_device_id="$(findmnt -nro MAJ:MIN --target "$backup_root")"
if [[ "$root_device_id" == "$backup_device_id" ]]; then
  printf 'Refusing backup destination on the same mounted filesystem as /.\n' >&2
  exit 1
fi

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
capture_dir="$backup_root/arcos-phase4-$stamp"
mkdir -p "$capture_dir/inventory"

lsblk -O -J >"$capture_dir/inventory/lsblk.json"
blkid >"$capture_dir/inventory/blkid.txt"
findmnt --json >"$capture_dir/inventory/findmnt.json"
btrfs filesystem usage / >"$capture_dir/inventory/btrfs-filesystem.txt"
btrfs subvolume list -a / >"$capture_dir/inventory/btrfs-subvolumes.txt"
find /boot -xdev -printf '%M %u %g %s %TY-%Tm-%TdT%TH:%TM:%TS %p\n' \
  >"$capture_dir/inventory/boot-tree.txt"
find /boot -xdev -type f -exec sha256sum {} + >"$capture_dir/inventory/boot-sha256.txt"
tar --acls --xattrs --one-file-system -C / -czf "$capture_dir/boot-backup.tar.gz" boot

sha256sum "$capture_dir/boot-backup.tar.gz" >"$capture_dir/SHA256SUMS"
tar -C "$backup_root" -czf "$capture_dir.tar.gz" "$(basename "$capture_dir")"
sha256sum "$capture_dir.tar.gz" >"$capture_dir.tar.gz.sha256"
printf 'Privileged read-only inventory and /boot backup saved to:\n%s\n' "$capture_dir"
