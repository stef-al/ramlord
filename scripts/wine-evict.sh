#!/bin/bash
# Rung 4 for M4: evict Notepad from the running app under Wine, and check
# that Notepad is gone. Drives the window with xdotool: a click on the
# first rent roll row opens Notepad's details card, Delete asks, Enter
# closes it politely. A screenshot is taken at each step.
#
# Runs inside xvfb-run, like wine-run.sh:
#
#   cargo build --release --target x86_64-pc-windows-gnu
#   WINEPREFIX=$PWD/scripts/prefix WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=" \
#     xvfb-run -a -s "-screen 0 1920x1080x24" \
#     scripts/wine-evict.sh target/x86_64-pc-windows-gnu/release/ramlord.exe OUT_DIR
#
# Needs xdotool and ImageMagick's import. Prints "notepad evicted" and
# exits 0 when Notepad closed, 1 otherwise.
set -u
EXE=${1:?usage: wine-evict.sh RAMLORD_EXE OUT_DIR}
OUT=${2:?usage: wine-evict.sh RAMLORD_EXE OUT_DIR}
mkdir -p "$OUT"

wineboot -i >/dev/null 2>&1

# A tenant needs half a unit to get its own row: 32 MB when Wine reports
# 16 GB. Notepad keeps a file as UTF-16, so 24 MB of text is about 48 MB.
BIG="${WINEPREFIX:?}/drive_c/ramlord-big.txt"
if [ ! -s "$BIG" ]; then
  yes "The tenant in 4B has been running since Tuesday and says it is busy." |
    head -c 24000000 >"$BIG"
fi

notepad_running() { pgrep -f 'notepad\.exe' >/dev/null; }

wine notepad 'C:\ramlord-big.txt' >/dev/null 2>&1 &
for _ in $(seq 60); do
  notepad_running && break
  sleep 0.5
done
# Give it time to read the file.
sleep 8

wine "$EXE" &
PID=$!
sleep 8

shot() { import -window root "$OUT/$1.png" 2>/dev/null || xwd -root -silent | convert xwd:- "$OUT/$1.png"; }
fail() {
  echo "$1"
  shot fail
  wineserver -k >/dev/null 2>&1
  exit 1
}

notepad_running || fail "notepad did not start"
WID=$(xdotool search --name '^RAMlord$' | head -1)
[ -n "$WID" ] || fail "no RAMlord window"
eval "$(xdotool getwindowgeometry --shell "$WID")"
SCALE=$((WIDTH / 384))
[ "$SCALE" -ge 1 ] || fail "window is $WIDTH wide"
echo "window $WID at $X,$Y, ${WIDTH}x$HEIGHT, scale $SCALE"
shot 1-tower

# Canvas pixel to screen: the middle of that pixel.
click() {
  xdotool mousemove $((X + $1 * SCALE + SCALE / 2)) $((Y + $2 * SCALE + SCALE / 2))
  xdotool click 1
}

# The first rent roll row is the largest tenant: Notepad.
click 300 54
sleep 1
shot 2-details
xdotool key Delete
sleep 1
shot 3-confirm
xdotool key Return
sleep 0.5
shot 4-asking

for _ in $(seq 20); do
  notepad_running || break
  sleep 0.5
done
notepad_running && fail "notepad is still running"
# The next sample reports what was vacated.
sleep 3
shot 5-done
echo "notepad evicted"

wine taskkill /im ramlord.exe >/dev/null 2>&1
for _ in $(seq 20); do
  kill -0 "$PID" 2>/dev/null || break
  sleep 0.5
done
wait "$PID"
echo "ramlord exit=$?"
wineserver -k >/dev/null 2>&1
