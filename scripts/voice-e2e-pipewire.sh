#!/usr/bin/env bash
set -euo pipefail

wav="${1:-/tmp/arc-voice-generated.wav}"
if [[ ! -s "$wav" ]]; then
  echo "usage: $0 <16 kHz mono test wav>" >&2
  exit 2
fi
original_source="$(pactl get-default-source)"
sink_module="$(pactl load-module module-null-sink sink_name=arc_e2e sink_properties=device.description=Arc_E2E)"
source_module="$(pactl load-module module-remap-source source_name=arc_e2e_mic master=arc_e2e.monitor source_properties=device.description=Arc_E2E_Microphone)"

cleanup() {
  pactl set-default-source "$original_source" || true
  pactl unload-module "$source_module" || true
  pactl unload-module "$sink_module" || true
}
trap cleanup EXIT

pactl set-default-source arc_e2e_mic
started="$(arcctl voice toggle)"
grep -q 'utteranceId' <<<"$started"
paplay --device=arc_e2e "$wav"
finished="$(arcctl voice toggle)"
grep -q '"stable": true' <<<"$finished"
grep -q '"utteranceId"' <<<"$finished"
printf '%s\n' "$finished"
