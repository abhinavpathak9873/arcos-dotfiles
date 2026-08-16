usage=""
tooltip="GPU utilization"

if command -v nvidia-smi >/dev/null 2>&1; then
  usage="$(nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits 2>/dev/null | head -n 1 | tr -d ' ')"
  name="$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -n 1)"
  [ -n "$name" ] && tooltip="$name"
fi

if [ -z "$usage" ]; then
  for busy in /sys/class/drm/card[0-9]*/device/gpu_busy_percent; do
    [ -r "$busy" ] || continue
    usage="$(tr -dc '0-9' < "$busy")"
    [ -n "$usage" ] && break
  done
fi

if [ -n "$usage" ]; then
  printf '{"text":"GPU %s%%","tooltip":"%s","class":"active"}\n' "$usage" "$tooltip"
else
  printf '{"text":"GPU —","tooltip":"GPU usage is unavailable from this driver","class":"unavailable"}\n'
fi
