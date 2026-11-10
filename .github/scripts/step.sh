#!/usr/bin/env bash
# Runs one CI step: `step.sh COMMAND [ARGS...]`.
#
# The output streams as usual. When the command fails, the end of its output
# is repeated as a GitHub `::error::` annotation, so the failure can be read
# through `gh api repos/OWNER/REPO/commits/SHA/check-runs` and the run's
# annotations without opening the log (SPEC.md, CI and releases).
set -uo pipefail

log=$(mktemp)
"$@" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}

if [ "$status" -ne 0 ]; then
  # Workflow commands need %, CR and LF escaped in the message, and also
  # ':' and ',' in properties such as the title.
  message=$(tail -n 30 "$log")
  message=${message//'%'/'%25'}
  message=${message//$'\r'/}
  message=${message//$'\n'/'%0A'}
  title="$* failed with exit code $status"
  title=${title//'%'/'%25'}
  title=${title//':'/'%3A'}
  title=${title//','/'%2C'}
  echo "::error title=${title}::${message}"
fi

rm -f "$log"
exit "$status"
