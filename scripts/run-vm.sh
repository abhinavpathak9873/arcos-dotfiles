#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
disk_dir="$repo_root/artifacts/vm"
disk_path="$disk_dir/arcos-vm.qcow2"

mkdir -p "$disk_dir"
cd "$repo_root"

printf 'Starting ArcOS VM with KVM.\n'
printf 'Persistent guest disk: %s\n' "$disk_path"
printf 'Display: VNC on 127.0.0.1:5900 (for example, open vnc://127.0.0.1:5900).\n'

exec docker compose -f compose.nix.yml run --rm \
  --publish 127.0.0.1:5900:5900 \
  --env NIX_DISK_IMAGE=/workspace/artifacts/vm/arcos-vm.qcow2 \
  --env 'QEMU_OPTS=-display vnc=0.0.0.0:0' \
  --entrypoint sh nix -lc \
  'vm_path=$(nix build .#vm --no-link --print-out-paths); exec "$vm_path/bin/run-arcos-vm-vm"'
