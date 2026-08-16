if [ ! -t 0 ]; then
  exec kitty --class arcos-update --title 'ArcOS Update' arcos-update
fi

choice="$(printf '%s\n' \
  'Update ArcOS system' \
  'Update Flatpak applications' \
  'Update Homebrew packages' \
  'Update everything' \
  'Show system generations' \
  | gum choose --header 'ArcOS Update')"

update_system() {
  if [ ! -e /etc/nixos/configuration.nix ]; then
    printf 'No installed NixOS configuration was found. The live image cannot update itself.\n'
    return 1
  fi
  sudo nh os switch /etc/nixos
}

update_flatpak() {
  flatpak update --user --assumeyes
}

update_brew() {
  if [ ! -x /home/linuxbrew/.linuxbrew/bin/brew ]; then
    printf 'Homebrew has not been initialized yet. Run brew once to set it up.\n'
    return 0
  fi
  /home/linuxbrew/.linuxbrew/bin/brew update
  /home/linuxbrew/.linuxbrew/bin/brew upgrade
}

case "$choice" in
  'Update ArcOS system') update_system ;;
  'Update Flatpak applications') update_flatpak ;;
  'Update Homebrew packages') update_brew ;;
  'Update everything')
    update_system
    update_flatpak
    update_brew
    ;;
  'Show system generations') sudo nixos-rebuild list-generations ;;
  *) exit 0 ;;
esac

printf '\nDone. Press Enter to close.\n'
read -r _
