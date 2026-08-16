theme="${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/rofi.rasi"
config="/etc/xdg/rofi/arcos-desktop.rasi"
choices="$(mktemp)"
paths="$(mktemp)"
trap 'rm -f "$choices" "$paths"' EXIT

for root in \
  "$HOME/Pictures/Wallpapers" \
  /run/current-system/sw/share/backgrounds \
  /etc/arcos-wallpapers; do
  [ -d "$root" ] || continue
  find "$root" -type f \( -iname '*.png' -o -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.webp' -o -iname '*.avif' \) -print0
done | sort -zu | while IFS= read -r -d '' path; do
  label="$(basename "$path")"
  printf '%s\n' "$path" >> "$paths"
  printf '%s\0icon\x1f%s\n' "$label" "$path" >> "$choices"
done

[ -s "$paths" ] || exit 0
index="$(rofi -dmenu -i -show-icons -format i -p 'Wallpaper' -no-custom -config "$config" -theme "$theme" < "$choices")"
[ -n "$index" ] || exit 0
wallpaper="$(sed -n "$((index + 1))p" "$paths")"
[ -f "$wallpaper" ] && exec arcos-theme wallpaper "$wallpaper"
