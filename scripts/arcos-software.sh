set +e
flatpak remote-add --user --if-not-exists flathub \
  https://dl.flathub.org/repo/flathub.flatpakrepo >/dev/null 2>&1
set -e
exec gnome-software "$@"
