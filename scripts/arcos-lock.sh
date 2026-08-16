state="${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/theme.json"
wallpaper="${ARCOS_DEFAULT_WALLPAPER:-}"
state_wallpaper="$(jq -r '.wallpaper // empty' "$state" 2>/dev/null || true)"
[ -f "$state_wallpaper" ] && wallpaper="$state_wallpaper"
args=(
  --style "${XDG_CONFIG_HOME:-$HOME/.config}/arcos-desktop/gtklock.css"
  --time-format '%H:%M'
  --date-format '%A, %d %B'
  --start-hidden
  --idle-hide
  --idle-timeout 8
  --modules "${ARCOS_GTKLOCK_USERINFO_MODULE:?}"
  --modules "${ARCOS_GTKLOCK_PLAYERCTL_MODULE:?}"
  --modules "${ARCOS_GTKLOCK_POWERBAR_MODULE:?}"
)
[ -f "$wallpaper" ] && args+=(--background "$wallpaper")
lock_log="${XDG_RUNTIME_DIR:-/tmp}/arcos-lock.log"
exec gtklock "${args[@]}" 2>>"$lock_log"
