theme="${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/rofi.rasi"
config="/etc/xdg/rofi/arcos-desktop.rasi"
state="${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/display-features.json"
mkdir -p "$(dirname "$state")"
[ -s "$state" ] || printf '{}\n' > "$state"

output="$(swaymsg -r -t get_outputs | jq -r '.[] | select(.active) | .name' \
  | rofi -dmenu -i -p 'Display' -no-custom -config "$config" -theme "$theme")"
[ -n "$output" ] || exit 0
[[ "$output" =~ ^[A-Za-z0-9._:-]+$ ]] || exit 2

action="$(printf '%s\n' \
  'Enable variable refresh rate' \
  'Disable variable refresh rate' \
  'Enable HDR (10-bit)' \
  'Disable HDR (8-bit SDR)' \
  | rofi -dmenu -i -p "$output" -no-custom -config "$config" -theme "$theme")"

case "$action" in
  'Enable variable refresh rate') property=vrr; value=on ;;
  'Disable variable refresh rate') property=vrr; value=off ;;
  'Enable HDR (10-bit)') property=hdr; value=on ;;
  'Disable HDR (8-bit SDR)') property=hdr; value=off ;;
  *) exit 0 ;;
esac

temporary="$state.new"
jq --arg output "$output" --arg property "$property" --arg value "$value" \
  '.[$output][$property] = $value' "$state" > "$temporary"
mv "$temporary" "$state"

if [ "$property" = vrr ]; then
  swaymsg "output $output adaptive_sync $value" >/dev/null
elif [ "$value" = on ]; then
  swaymsg "output $output render_bit_depth 10; output $output hdr on" >/dev/null
else
  swaymsg "output $output hdr off; output $output render_bit_depth 8" >/dev/null
fi

generated="${XDG_CONFIG_HOME:-$HOME/.config}/sway/config.d/90-display-features.conf"
mkdir -p "$(dirname "$generated")"
jq -r 'to_entries[] | .key as $output |
  if .value.vrr then "output \($output) adaptive_sync \(.value.vrr)" else empty end,
  if .value.hdr == "on" then "output \($output) render_bit_depth 10\noutput \($output) hdr on"
  elif .value.hdr == "off" then "output \($output) hdr off\noutput \($output) render_bit_depth 8"
  else empty end' "$state" > "$generated"
