#!/usr/bin/env bash
# test-batches.sh - run the full EAK verification in small, ordered, memory-aware batches.
#
# Nothing is skipped silently: every workspace crate's tests run, each batch is wrapped by
# scripts/eak-res.sh (peak RAM / swap / duration go to target/eak-res/runs.tsv), a failing batch
# does not stop later batches, and the exit code is non-zero if ANY batch failed.
#
# Usage: scripts/test-batches.sh [--package] [--only NAME[,NAME...]] [--no-lock] [--summary FILE]
#   --package   also run the packaging smoke batch (npm run tauri build --no-bundle ...). Off by
#               default because it is the heaviest step (~2.5 GB).
#   --only      run only the named batches (see names printed below), e.g. --only light,clippy
#   --no-lock   do not take the per-batch host build lock (single-user machines, or when the caller already holds it)
#   --lock-once re-run this script under ONE flock for the whole run instead of queueing per batch
#               (use on a shared host: per-batch queueing can wait for each batch separately)
# Environment:
#   EAK_BUILD_LOCK   lock file shared by every heavy job on the host
#                    (default: $HOME/.cache/eak/build.lock)
#   EAK_WAIT_MEM     MB of MemAvailable required before each batch (default 500; swap absorbs the rest)
#   EAK_NODE_HEAP_MB node --max-old-space-size for tsc/vite/vitest (default 1536)
#   EAK_VITEST_WORKERS  vitest worker processes (default 2)
set -uo pipefail
cd "$(dirname "$0")/.."
root="$PWD"

do_package=0; only=""; use_lock=1; summary=""; lock_once=0
while [ $# -gt 0 ]; do
  case "$1" in
    --package) do_package=1; shift ;;
    --only) only="$2"; shift 2 ;;
    --no-lock) use_lock=0; shift ;;
    --lock-once) lock_once=1; shift ;;
    --summary) summary="$2"; shift 2 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "test-batches: unknown option $1" >&2; exit 64 ;;
  esac
done

lock_file="${EAK_BUILD_LOCK:-$HOME/.cache/eak/build.lock}"
if [ "$lock_once" -eq 1 ]; then
  mkdir -p "$(dirname "$lock_file")"
  args=(--no-lock); [ "$do_package" -eq 1 ] && args+=(--package); [ -n "$only" ] && args+=(--only "$only"); [ -n "$summary" ] && args+=(--summary "$summary")
  exec flock "$lock_file" "$0" "${args[@]}"
fi
wait_mem="${EAK_WAIT_MEM:-500}"
export NODE_OPTIONS="--max-old-space-size=${EAK_NODE_HEAP_MB:-1536}"
workers="${EAK_VITEST_WORKERS:-2}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"

res=(scripts/eak-res.sh --wait-mem "$wait_mem")
[ "$use_lock" -eq 1 ] && res+=(--lock "$lock_file")

failed=(); passed=(); lines=()
selected() { [ -z "$only" ] || [[ ",$only," == *",$1,"* ]]; }

# batch NAME COMMAND... : run one batch, record the result, carry on.
batch() {
  local name="$1"; shift
  selected "$name" || selected "${name%%:*}" || return 0
  echo "=== batch: $name ==="
  if "${res[@]}" --label "$name" -- "$@"; then passed+=("$name"); else failed+=("$name"); fi
}
DESK=(bash -c 'cd "$0" && exec "$@"' "$root/apps/desktop")

echo "EAK batches | lock=$([ "$use_lock" -eq 1 ] && echo "$lock_file" || echo none) | wait-mem=${wait_mem}MB | $NODE_OPTIONS | cargo jobs=$CARGO_BUILD_JOBS"
[ -d apps/desktop/node_modules ] || { echo "installing frontend deps (npm ci)"; "${DESK[@]}" npm ci --no-audit --no-fund; }

# (a) light: formatting, policy, frontend types and unit tests.
batch light:fmt cargo fmt --all -- --check
batch light:no-python scripts/check-no-python.sh
batch light:tsc "${DESK[@]}" npx tsc --noEmit
batch light:vitest "${DESK[@]}" npx vitest run --pool=forks --maxWorkers="$workers"

# (b) per-crate Rust tests, one crate at a time, smallest first. eak-intelligence includes the
# LCSC/JLCPCB provider tests; eak-runtime runs with 2 test threads (process tests are timing-
# sensitive under load). eak-desktop is the Tauri IPC crate (links the WebView libraries).
for crate in eak-core eak-engineering eak-validation eak-manufacturing eak-intelligence eak-runtime eak-desktop; do
  extra=()
  [ "$crate" = eak-runtime ] && extra=(-- --test-threads=2)
  batch "test:$crate" cargo test -p "$crate" --release "${extra[@]}"
done

# (c) lints last: clippy over the whole workspace, warnings are errors.
batch clippy cargo clippy --workspace --all-targets -- -D warnings

# (d) packaging smoke (opt-in).
if [ "$do_package" -eq 1 ]; then
  batch package:tauri "${DESK[@]}" npx tauri build --no-bundle
else
  echo "=== batch: package:tauri NOT RUN (pass --package to run it) ==="
fi

echo
echo "================ summary ================"
echo "passed (${#passed[@]}): ${passed[*]:-none}"
echo "failed (${#failed[@]}): ${failed[*]:-none}"
[ "$do_package" -eq 1 ] || echo "not run: package:tauri (flag --package)"
if [ -n "$summary" ]; then
  { echo "passed: ${passed[*]:-none}"; echo "failed: ${failed[*]:-none}"; } > "$summary"
fi
[ ${#failed[@]} -eq 0 ]
