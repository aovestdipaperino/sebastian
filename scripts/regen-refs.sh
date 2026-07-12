#!/usr/bin/env bash
# Regenerates the reference .svg next to every .mmd in the given directories
# using the pinned harness mmdc (mermaid 11.15.0). Run scripts/setup-harness.sh
# first. References are byte-compared by the corpus tests, so regenerate only
# when bumping the mermaid version or adding fixtures — and expect randomness
# (rough.js shapes, random ids) to churn some files; the corpus tests mask
# those cases explicitly.
#
#   scripts/regen-refs.sh sebastian/tests/flowchart_cases [more dirs...]
#   HARNESS=/path/to/harness scripts/regen-refs.sh ...
#
# Gantt references depend on the render-time timezone: generate with
# TZ=America/Los_Angeles (CI runs the tests pinned to it).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
harness="${HARNESS:-$root/harness}"
mmdc="$harness/node/node_modules/.bin/mmdc"
if [ ! -x "$mmdc" ]; then
  echo "harness mmdc not found at $mmdc — run scripts/setup-harness.sh first" >&2
  exit 1
fi

[ $# -gt 0 ] || { echo "usage: $0 <dir-with-.mmd-files> [more dirs...]" >&2; exit 1; }

for dir in "$@"; do
  for mmd in "$dir"/*.mmd; do
    [ -e "$mmd" ] || continue
    svg="${mmd%.mmd}.svg"
    "$mmdc" -i "$mmd" -o "$svg" -q -p "$harness/pup.json"
    echo "$svg"
  done
done
