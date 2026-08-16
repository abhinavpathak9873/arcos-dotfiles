state="${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/theme.json"
accent="$(jq -r '.accent // "#cba6f7"' "$state" 2>/dev/null)"
color="${accent#\#}"
profile_dir="${XDG_CONFIG_HOME:-$HOME/.config}/OpenRGB"
profile="$profile_dir/arcos-rgb.orp"
mkdir -p "$profile_dir"

# Devices expose different names for their static mode. Apply to every
# controller that supports it, persist a real OpenRGB profile, and stay silent
# when the current machine simply has no RGB hardware.
openrgb --client 127.0.0.1:6742 --mode static --brightness 55 --color "$color" \
  --save-profile "$profile" >/dev/null 2>&1 ||
  openrgb --mode static --brightness 55 --color "$color" --save-profile "$profile" \
    >/dev/null 2>&1 || true
