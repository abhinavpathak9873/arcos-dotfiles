theme="${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/rofi.rasi"
config="/etc/xdg/rofi/arcos-desktop.rasi"

mapfile -t displays < <(ddcutil detect --brief 2>/dev/null | awk '/^Display [0-9]+/{print $2}')
if [ "${#displays[@]}" -eq 0 ]; then
  notify-send "Monitor brightness" "No DDC/CI-capable external displays were detected. Check that DDC/CI is enabled in the monitor menu."
  exit 1
fi

display="${displays[0]}"
if [ "${#displays[@]}" -gt 1 ]; then
  display="$(printf 'Display %s\n' "${displays[@]}" | rofi -dmenu -p 'Monitor' -no-custom -config "$config" -theme "$theme" | awk '{print $2}')"
  [ -n "$display" ] || exit 0
fi

values="$(ddcutil getvcp 10 --display "$display" --terse 2>/dev/null || true)"
current="$(printf '%s\n' "$values" | awk '$1 == "VCP" && $2 == "10" {for (i=1;i<=NF;i++) if ($i == "C") {print $(i+1); exit}}')"
current="${current:-50}"
level="$(zenity --scale --title="Display $display brightness" --text='Set external monitor brightness' --min-value=1 --max-value=100 --value="$current" --step=1 2>/dev/null || true)"
[ -n "$level" ] || exit 0
ddcutil setvcp 10 "$level" --display "$display"
notify-send "Display $display" "Brightness set to $level%"
