destination="$HOME/ArcOS-Config"
if [ ! -d "$destination" ]; then
  cp -a /etc/arcos "$destination"
  chmod -R u+w "$destination"
fi

if command -v code >/dev/null 2>&1; then
  exec code "$destination"
fi
exec kitty --directory "$destination"
