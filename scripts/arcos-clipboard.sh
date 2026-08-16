#!/usr/bin/env bash

selection="$(cliphist list | walker --dmenu --placeholder 'Clipboard history · text and images' || true)"
[ -n "$selection" ] || exit 0
printf '%s' "$selection" | cliphist decode | wl-copy
notify-send -a ArcOS -i edit-paste "Copied from history" "The selected item is ready to paste."
