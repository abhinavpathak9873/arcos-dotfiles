#!/usr/bin/env bash
set -euo pipefail

release_dir="${1:-.}"
output_path="${2:-$release_dir/arcos-universal-26.05-dev.iso}"
checksum_file="$release_dir/arcos-universal-26.05-dev.iso.sha256"

shopt -s nullglob
parts=("$release_dir"/arcos-universal-26.05-dev.iso.part-*)
shopt -u nullglob
if [[ "${#parts[@]}" -eq 0 ]]; then
  printf 'No ISO parts found in %s.\n' "$release_dir" >&2
  exit 1
fi
if [[ ! -f "$checksum_file" ]]; then
  printf 'Missing checksum file: %s\n' "$checksum_file" >&2
  exit 1
fi

cat "${parts[@]}" >"$output_path"
expected="$(awk '{ print $1; exit }' "$checksum_file")"
actual="$(sha256sum "$output_path" | cut -d' ' -f1)"
if [[ "$actual" != "$expected" ]]; then
  printf 'Reassembled ISO checksum mismatch.\n' >&2
  exit 1
fi
printf 'Reassembled and verified: %s\n' "$output_path"
