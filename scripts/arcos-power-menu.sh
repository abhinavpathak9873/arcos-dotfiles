power_log="${XDG_RUNTIME_DIR:-/tmp}/arcos-power-menu.log"
exec nwg-bar -t /etc/xdg/nwg-bar/bar.json \
  -s "${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/nwg-bar.css" \
  2>>"$power_log"
