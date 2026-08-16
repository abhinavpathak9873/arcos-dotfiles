theme="${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/rofi.rasi"
config="/etc/xdg/rofi/arcos-desktop.rasi"

exec rofi -show drun -config "$config" -theme "$theme"
