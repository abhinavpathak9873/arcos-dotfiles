#!/usr/bin/env bash
set -euo pipefail

if [[ $# -eq 0 ]]; then
  set -- flake check --print-build-logs
fi

exec docker compose -f compose.nix.yml run --rm nix "$@"
