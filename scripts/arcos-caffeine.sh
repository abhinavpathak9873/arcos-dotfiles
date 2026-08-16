#!/usr/bin/env bash

unit="arc-caffeine.service"
command="${1:-status}"

active=false
if systemctl --user --quiet is-active "$unit"; then
  active=true
fi

case "$command" in
  status)
    if $active; then
      printf '{"text":"󰅶","class":"active","tooltip":"Sleep blocker is on · click to allow suspend"}\n'
    else
      printf '{"text":"󰾪","class":"inactive","tooltip":"Sleep blocker is off · click to keep this computer awake"}\n'
    fi
    ;;
  toggle)
    if $active; then
      systemctl --user stop "$unit"
      notify-send -a ArcOS -i caffeine-cup-empty "Sleep blocker off" "Automatic lock and suspend are available again."
    else
      systemctl --user start "$unit"
      notify-send -a ArcOS -i caffeine "Sleep blocker on" "ArcOS will stay awake until you turn this off."
    fi
    ;;
  *)
    printf 'Usage: arcos-caffeine [status|toggle]\n' >&2
    exit 2
    ;;
esac
