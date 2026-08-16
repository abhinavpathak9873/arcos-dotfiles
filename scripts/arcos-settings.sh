theme="${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/rofi.rasi"
config="/etc/xdg/rofi/arcos-desktop.rasi"

entries='Wallpaper and colors
Displays
Display performance
External monitor brightness
Network
Bluetooth
Sound
Software
Updates
Capture and camera
Tailscale
System monitor
NVIDIA settings
OpenRGB
Syncthing
Disks
ArcOS configuration'

choice="$(printf '%s\n' "$entries" | rofi -dmenu -i -p 'Settings' -no-custom -config "$config" -theme "$theme")"

case "$choice" in
  'Wallpaper and colors')
    action="$(printf '%s\n' 'Browse included wallpapers' 'Match current wallpaper automatically' 'Choose wallpaper file' 'Choose accent color' 'Reapply current theme' | rofi -dmenu -p 'Appearance' -no-custom -config "$config" -theme "$theme")"
    case "$action" in
      'Browse included wallpapers') exec arcos-wallpaper-picker ;;
      'Match current wallpaper automatically') arcos-theme auto ;;
      'Choose wallpaper file')
        wallpaper="$(zenity --file-selection --title='Choose wallpaper' --file-filter='Images | *.png *.jpg *.jpeg *.webp *.avif' 2>/dev/null || true)"
        [ -n "$wallpaper" ] && arcos-theme wallpaper "$wallpaper"
        ;;
      'Choose accent color')
        color="$(zenity --color-selection --show-palette --title='Choose desktop accent' 2>/dev/null || true)"
        [ -n "$color" ] && arcos-theme color "$color"
        ;;
      'Reapply current theme') arcos-theme apply ;;
    esac
    ;;
  Displays) exec wdisplays ;;
  'Display performance') exec arcos-display-features ;;
  'External monitor brightness')
    command -v arcos-monitor-brightness >/dev/null && exec arcos-monitor-brightness
    ;;
  Network) exec nm-connection-editor ;;
  Bluetooth) exec blueman-manager ;;
  Sound) exec pavucontrol ;;
  Software) exec arcos-software ;;
  Updates) exec arcos-update ;;
  'Capture and camera') exec arcos-capture ;;
  Tailscale) exec kitty --class arcos-tailscale --title Tailscale sh -lc 'sudo tailscale up; printf "\nPress Enter to close.\n"; read -r _' ;;
  'System monitor') exec kitty --class arcos-btop --title 'System Monitor' btop ;;
  'NVIDIA settings') command -v nvidia-settings >/dev/null && exec nvidia-settings ;;
  OpenRGB) command -v openrgb >/dev/null && exec openrgb ;;
  Syncthing) exec xdg-open http://127.0.0.1:8384 ;;
  Disks) exec gnome-disks ;;
  'ArcOS configuration') exec arcos-config-backup ;;
esac
