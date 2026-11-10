#!/bin/bash
# Rung 4 for M5: the settings screen (F3), the rent roll card (F2) saved
# to Pictures\RAMlord and copied to the clipboard, close to the tray, and a
# second start bringing the first window back. Runs inside xvfb-run:
#
#   cargo build --release --target x86_64-pc-windows-gnu
#   WINEPREFIX=$PWD/scripts/prefix WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=" \
#     xvfb-run -a -s "-screen 0 1920x1080x24" \
#     scripts/wine-m5.sh target/x86_64-pc-windows-gnu/release/ramlord.exe OUT_DIR
#
# Needs xdotool, xclip and ImageMagick. Prints one line per check, then
# "m5 checks: N failed".
set -u
EXE=${1:?usage: wine-m5.sh RAMLORD_EXE OUT_DIR}
OUT=${2:?usage: wine-m5.sh RAMLORD_EXE OUT_DIR}
mkdir -p "$OUT"
failed=0
check() { if eval "$2"; then echo "ok: $1"; else echo "FAILED: $1"; failed=$((failed + 1)); fi; }
shot() { import -window root "$OUT/$1.png" 2>/dev/null; }
visible() { xdotool search --onlyvisible --name '^RAMlord$' 2>/dev/null | head -1; }
running() { pgrep -f 'ramlord\.exe' | wc -l; }

wineboot -i >/dev/null 2>&1
. "$(dirname "$0")/wine-settings.sh"
# Defaults, but no network: the close button hides to the tray.
wine_settings '{"fetch_prices": false}'
profile=$(winepath -u "$(wine cmd /c 'echo %USERPROFILE%' 2>/dev/null | tr -d '\r')")
rm -f "$profile"/Pictures/RAMlord/rent-roll-*.png "$HOME"/Pictures/RAMlord/rent-roll-*.png

wine "$EXE" &
sleep 8
WID=$(visible)
check "the window opens" '[ -n "$WID" ]'
shot 1-tower
xdotool windowfocus "$WID" 2>/dev/null; xdotool mousemove 300 10 click 1
xdotool key F3
sleep 1
shot 2-settings
xdotool key Escape
xdotool key F2
sleep 3
shot 3-card-saved
card=$(ls "$profile"/Pictures/RAMlord/rent-roll-*.png "$HOME"/Pictures/RAMlord/rent-roll-*.png 2>/dev/null | head -1)
check "F2 writes Pictures\\RAMlord\\rent-roll-*.png" '[ -n "$card" ]'
[ -n "$card" ] && cp "$card" "$OUT/card.png"
check "the card is 960 by 1200" '[ "$(identify -format %wx%h "$OUT/card.png" 2>/dev/null)" = 960x1200 ]'
targets=$(timeout 5 xclip -selection clipboard -t TARGETS -o 2>/dev/null | tr '\n' ' ')
echo "clipboard offers: $targets"
check "the card is on the clipboard as an image" 'echo "$targets" | grep -qi "image/"'

# A second start exits at once and brings nothing new.
start=$(date +%s)
wine "$EXE"
code=$?
check "a second start exits with 0 (got $code)" '[ "$code" = 0 ]'
check "a second start takes under 10 s" '[ $(( $(date +%s) - start )) -lt 10 ]'
check "one window, one copy" '[ "$(xdotool search --name "^RAMlord$" 2>/dev/null | wc -l)" = 1 ]'

# WM_CLOSE hides the window to the tray; the process stays.
wine taskkill /im ramlord.exe >/dev/null 2>&1
sleep 2
check "closing hides the window" '[ -z "$(visible)" ]'
check "closing keeps RAMlord running" '[ "$(running)" -ge 1 ]'
# A second start brings the hidden window back.
wine "$EXE"
sleep 2
check "a second start shows the first window again" '[ -n "$(visible)" ]'
shot 4-back

wine taskkill /f /im ramlord.exe >/dev/null 2>&1
wineserver -k >/dev/null 2>&1
echo "m5 checks: $failed failed"
