#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s ISO_PATH WHOLE_DISK_DEVICE\n' "$0"
  printf 'Example: %s artifacts/iso/arcos-universal-26.05-dev.iso /dev/sdb\n' "$0"
}

if [[ "$#" -ne 2 ]]; then
  usage >&2
  exit 2
fi

iso_path="$(readlink -f -- "$1")"
target_device="$(readlink -f -- "$2")"

if [[ ! -f "$iso_path" ]]; then
  printf 'ISO not found: %s\n' "$iso_path" >&2
  exit 1
fi
if [[ ! -b "$target_device" ]]; then
  printf 'Target is not a block device: %s\n' "$target_device" >&2
  exit 1
fi
if [[ "$(lsblk -dnro TYPE "$target_device")" != "disk" ]]; then
  printf 'Target must be a whole disk, not a partition: %s\n' "$target_device" >&2
  exit 1
fi

root_source="$(findmnt -nro SOURCE --target /)"
root_parent="$(lsblk -nro PKNAME "$root_source" 2>/dev/null | head -n1 || true)"
target_name="$(basename "$target_device")"
if [[ "$target_name" == "$root_parent" || "$target_device" == "$root_source" ]]; then
  printf 'Refusing to overwrite the disk that contains the running root filesystem.\n' >&2
  exit 1
fi

if lsblk -nro MOUNTPOINTS "$target_device" | grep -q '[^[:space:]]'; then
  printf 'Refusing to write: the target or one of its partitions is mounted.\n' >&2
  lsblk -o NAME,SIZE,TYPE,FSTYPE,LABEL,MOUNTPOINTS "$target_device" >&2
  exit 1
fi

iso_size="$(stat -c %s "$iso_path")"
device_size="$(blockdev --getsize64 "$target_device")"
if (( device_size < iso_size )); then
  printf 'Target is smaller than the ISO.\n' >&2
  exit 1
fi

printf '\nDESTRUCTIVE USB WRITE\n'
printf 'ISO:    %s\n' "$iso_path"
printf 'SHA256: %s\n' "$(sha256sum "$iso_path" | cut -d' ' -f1)"
printf 'Target: %s\n' "$target_device"
lsblk -d -o NAME,SIZE,MODEL,SERIAL,TRAN "$target_device"
printf '\nEvery existing byte on %s will be overwritten.\n' "$target_device"
printf 'Type exactly: FLASH %s\n> ' "$target_device"
read -r confirmation
if [[ "$confirmation" != "FLASH $target_device" ]]; then
  printf 'Confirmation did not match; nothing was written.\n' >&2
  exit 1
fi

sudo dd if="$iso_path" of="$target_device" bs=16M status=progress conv=fsync
sync

expected="$(sha256sum "$iso_path" | cut -d' ' -f1)"
actual="$(sudo head -c "$iso_size" "$target_device" | sha256sum | cut -d' ' -f1)"
if [[ "$actual" != "$expected" ]]; then
  printf 'USB verification failed. Expected %s but read %s.\n' "$expected" "$actual" >&2
  exit 1
fi
printf 'USB write and byte-for-byte verification completed successfully.\n'
