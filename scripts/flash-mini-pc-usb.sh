#!/usr/bin/env bash
set -euo pipefail

# Compatibility name retained for the original mini-PC test instructions.
exec "$(dirname -- "$0")/flash-arcos-usb.sh" "$@"
