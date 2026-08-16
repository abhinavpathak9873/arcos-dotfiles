runtime="${XDG_RUNTIME_DIR:?A graphical user session is required}"
state="$runtime/arcos-wf-recorder.pid"
recordings="${XDG_VIDEOS_DIR:-$HOME/Videos}/Recordings"

if [ -s "$state" ]; then
  pid="$(cat "$state")"
  if kill -INT "$pid" 2>/dev/null; then
    unlink "$state"
    notify-send "Recording saved" "$recordings"
    exit 0
  fi
  unlink "$state"
fi

geometry="$(slurp 2>/dev/null || true)"
[ -n "$geometry" ] || exit 0
mkdir -p "$recordings"
file="$recordings/$(date +%Y-%m-%d_%H-%M-%S).mp4"
wf-recorder -g "$geometry" -f "$file" >/dev/null 2>&1 &
pid=$!
printf '%s\n' "$pid" > "$state"
notify-send "Recording started" "Meta+Shift+R stops and saves it"
