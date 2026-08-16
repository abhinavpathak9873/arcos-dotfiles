capture_dir="${XDG_PICTURES_DIR:-$HOME/Pictures}/Captures"
mkdir -p "$capture_dir"
stamp="$(date +%Y-%m-%d_%H-%M-%S)"

choice="$(printf '%s\n' \
  '󰹑  Screenshot an area' \
  '󰍹  Screenshot the display' \
  '󰑋  Record screen with sound' \
  '󰄀  Open camera' \
  | rofi -dmenu -i -p 'Capture' -no-custom -config /etc/xdg/rofi/arcos-desktop.rasi \
      -theme "${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/rofi.rasi")"

case "$choice" in
  *'Screenshot an area')
    geometry="$(slurp)" || exit 0
    target="$capture_dir/Screenshot_$stamp.png"
    grim -g "$geometry" "$target"
    satty --filename "$target" --output-filename "$target"
    wl-copy < "$target"
    notify-send -i "$target" 'Screenshot saved' "$target"
    ;;
  *'Screenshot the display')
    target="$capture_dir/Screenshot_$stamp.png"
    grim "$target"
    satty --filename "$target" --output-filename "$target"
    wl-copy < "$target"
    notify-send -i "$target" 'Screenshot saved' "$target"
    ;;
  *'Record screen with sound')
    exec kooha
    ;;
  *'Open camera')
    exec snapshot
    ;;
esac
