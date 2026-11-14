#!/usr/bin/env bash
# ci-local.sh - the project's local CI entry point. GitHub Actions is NOT used (billing); this
# runs the same checks on any developer machine for free. Linux (uses /proc for the monitor).
#
#   scripts/ci-local.sh              # light + per-crate tests + clippy, sequential, memory-aware
#   scripts/ci-local.sh --package    # additionally the Tauri packaging smoke build
#   scripts/ci-local.sh --only light # any test-batches.sh option is passed through
#
# Results (peak RAM, swap, duration, exit code per batch) accumulate in target/eak-res/runs.tsv.
# Exit status is non-zero if any batch failed.
set -uo pipefail
cd "$(dirname "$0")/.."
exec scripts/test-batches.sh "$@"
