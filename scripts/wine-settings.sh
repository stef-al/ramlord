#!/bin/bash
# Sourced by the Wine scripts: writes RAMlord's settings.json in the Wine
# prefix's %APPDATA%\RAMlord. Usage: wine_settings '{"close_quits": true}'
# The prices are not fetched in these runs, so they do not touch the network.
wine_settings() {
  local appdata
  appdata=$(wine cmd /c 'echo %APPDATA%' 2>/dev/null | tr -d '\r')
  local dir
  dir="$(winepath -u "$appdata" 2>/dev/null)/RAMlord"
  mkdir -p "$dir"
  printf '%s\n' "$1" >"$dir/settings.json"
}
