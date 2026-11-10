#!/bin/bash
# runs inside xvfb-run: boot the prefix, start the probe, screenshot while its window is up
wineboot -i >/dev/null 2>&1
wine probe.exe 12 &
PID=$!
sleep 9
import -window root shot.png 2>/dev/null || xwd -root -silent | convert xwd:- shot.png
wait $PID
echo "probe exit=$?"
wineserver -k >/dev/null 2>&1
