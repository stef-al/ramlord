#!/bin/bash
# Rung 4: run ramlord under Wine, screenshot while its window is up, then
# ask it to close and report how it ended.
#
# Adapted from docs/reference/wine-probe/run.sh. Runs inside xvfb-run:
#
#   cargo build --release --target x86_64-pc-windows-gnu
#   WINEPREFIX=$PWD/scripts/prefix WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=" \
#     xvfb-run -a -s "-screen 0 1920x1080x24" \
#     scripts/wine-run.sh target/x86_64-pc-windows-gnu/release/ramlord.exe shot.png [ARG...]
#
# With no ARG it runs the live app. WINE_ALSO="notepad" starts other
# programs first, so the live tower has tenants. Prints "ramlord exit=N";
# 0 means the window closed cleanly on WM_CLOSE.
set -u
EXE=${1:?usage: wine-run.sh RAMLORD_EXE SHOT_PNG [ARG...]}
SHOT=${2:?usage: wine-run.sh RAMLORD_EXE SHOT_PNG [ARG...]}
shift 2

# Boot the prefix first; the first run takes under a minute.
wineboot -i >/dev/null 2>&1

for program in ${WINE_ALSO:-}; do
  wine "$program" >/dev/null 2>&1 &
done
[ -n "${WINE_ALSO:-}" ] && sleep 3

wine "$EXE" "$@" &
PID=$!
sleep 8
import -window root "$SHOT" 2>/dev/null || xwd -root -silent | convert xwd:- "$SHOT"

# Without /f, taskkill posts WM_CLOSE to the window: the same path as the
# close button. Give it 10 seconds before giving up.
wine taskkill /im ramlord.exe >/dev/null 2>&1
for _ in $(seq 20); do
  kill -0 "$PID" 2>/dev/null || break
  sleep 0.5
done
if kill -0 "$PID" 2>/dev/null; then
  echo "ramlord did not close on WM_CLOSE"
  wineserver -k >/dev/null 2>&1
  exit 1
fi
wait "$PID"
echo "ramlord exit=$?"
wineserver -k >/dev/null 2>&1
