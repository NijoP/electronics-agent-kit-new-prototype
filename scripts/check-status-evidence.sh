#!/usr/bin/env bash
# Keeps the evidence claims in STATUS.md, FEATURE_MATRIX.md and README.md honest against the
# real-window UI evidence workflow (DoD slice F11).
#  1. Every ui-evidence scenario file these docs cite (`<x><n>-....json`) exists under
#     apps/desktop/e2e/scenarios/ and is run by a UI_SCENARIO step of .github/workflows/ui-evidence.yml.
#  2. Every STATUS.md row labelled "CI + real-window verified" cites at least one scenario file.
#  3. README.md's "<N> scenarios drive the app in a real window" equals the number of UI_SCENARIO steps.
# Usage: bash scripts/check-status-evidence.sh
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wf="$repo/.github/workflows/ui-evidence.yml"
dir="$repo/apps/desktop/e2e/scenarios"
fail=0
err() { echo "check-status-evidence: $*" >&2; fail=1; }

run="$(sed -nE 's#^ +UI_SCENARIO: scenarios/([a-z0-9-]+\.json) *$#\1#p' "$wf" | sort -u)"
[ -n "$run" ] || err "no UI_SCENARIO step found in $wf"

for doc in STATUS.md FEATURE_MATRIX.md README.md; do
  for s in $(grep -oE '\b[a-z][0-9]+-[a-z0-9-]+\.json\b' "$repo/$doc" | sort -u); do
    [ -f "$dir/$s" ] || err "$doc cites $s, which is not in apps/desktop/e2e/scenarios/"
    grep -qxF "$s" <<<"$run" || err "$doc cites $s, which no UI_SCENARIO step of ui-evidence.yml runs"
  done
done

while IFS= read -r row; do
  grep -qE '\b[a-z][0-9]+-[a-z0-9-]+\.json\b' <<<"$row" \
    || err "STATUS.md row labelled CI + real-window verified cites no scenario: ${row:0:80}"
done < <(grep -E '^\| [^|]+ \| CI \+ real-window verified' "$repo/STATUS.md" || true)

n="$(grep -c '^ *UI_SCENARIO: ' "$wf" || true)"
claim="$(tr '\n' ' ' <"$repo/README.md" | grep -oE '[0-9]+ scenarios drive +the app in a real window' | grep -oE '^[0-9]+' || true)"
[ "$claim" = "$n" ] || err "README.md says '${claim:-no count}' scenarios drive the app in a real window; ui-evidence.yml runs $n"

[ "$fail" -eq 0 ] && echo "check-status-evidence: ok ($n scenarios)"
exit "$fail"
