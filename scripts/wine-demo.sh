#!/bin/bash
# Rung 4 for --demo: scripts/wine-run.sh with --demo. Same usage, inside
# xvfb-run: wine-demo.sh RAMLORD_EXE SHOT_PNG
exec "$(dirname "$0")/wine-run.sh" "${1:?usage: wine-demo.sh RAMLORD_EXE SHOT_PNG}" "${2:?}" --demo
