theme="${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/rofi.rasi"
config="/etc/xdg/rofi/arcos-desktop.rasi"
actions="$(mktemp)"
labels="$(mktemp)"
trap 'rm -f "$actions" "$labels"' EXIT

swaymsg -r -t get_workspaces | jq -r '.[] | [.num, .name] | @tsv' | while IFS=$'\t' read -r number name; do
  printf 'workspace\t%s\n' "$number" >> "$actions"
  printf '󰖲  Workspace %s\n' "$name" >> "$labels"
done

swaymsg -r -t get_tree | jq -r '
  def windows($workspace):
    (if .type == "workspace" then .name else $workspace end) as $current
    | if ((.pid // null) != null and (.name // "") != "") then
        [.id, $current, (.app_id // .window_properties.class // "Application"), .name] | @tsv
      else empty end,
      ((.nodes // [])[] | windows($current)),
      ((.floating_nodes // [])[] | windows($current));
  windows("")
' | while IFS=$'\t' read -r id workspace app title; do
  printf 'window\t%s\n' "$id" >> "$actions"
  printf '󰖯  %s  ·  %s — %s\n' "$workspace" "$app" "$title" >> "$labels"
done

[ -s "$labels" ] || exit 0
index="$(rofi -dmenu -i -format i -p 'Overview' -no-custom -config "$config" -theme "$theme" < "$labels")"
[ -n "$index" ] || exit 0
action="$(sed -n "$((index + 1))p" "$actions")"
kind="${action%%$'\t'*}"
target="${action#*$'\t'}"
case "$kind" in
  workspace) swaymsg "workspace number $target" >/dev/null ;;
  window) swaymsg "[con_id=$target] focus" >/dev/null ;;
esac
