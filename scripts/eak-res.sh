#!/usr/bin/env bash
# eak-res.sh - run one command and record what it cost (peak RAM, swap, duration, exit code).
#
# Development tool for small machines (EAK targets an 8 GB laptop). Linux only: it reads /proc.
# It never changes system configuration and never kills anything except, on Ctrl-C/TERM,
# the command it started itself.
#
# Usage:
#   scripts/eak-res.sh [options] -- COMMAND [ARGS...]
#
# Options:
#   --label NAME        short name for the run in the log (default: first word of the command)
#   --wait-mem MB       before starting, wait until MemAvailable >= MB (see --wait-timeout)
#   --wait-timeout SEC  give up waiting after SEC seconds and run anyway, with a warning
#                       (default 600; a stuck wait must not hide a real result)
#   --lock FILE         run under flock(1) on FILE so heavy jobs on one host never overlap
#   --log FILE          append one TSV line here (default: target/eak-res/runs.tsv)
#   --interval SEC      sampling period (default 1)
#
# Log columns (tab separated, header written once):
#   timestamp_utc label command exit_code duration_s peak_tree_rss_mb min_mem_avail_mb
#   peak_swap_used_mb swap_delta_mb lock_wait_s mem_wait_s
# peak_tree_rss_mb is the largest sum of RSS over the command and all its descendants at any
# one sample, so a short spike between two samples can be missed (documented limitation).
# duration_s covers the command only, not the lock wait or the memory wait.
#
# Exit code: the wrapped command's exit code (so it composes in scripts and CI).
set -u

label=""
wait_mem=0
wait_timeout=600
lock=""
interval=1
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
log="${EAK_RES_LOG:-$root/target/eak-res/runs.tsv}"

while [ $# -gt 0 ]; do
  case "$1" in
    --label) label="$2"; shift 2 ;;
    --wait-mem) wait_mem="$2"; shift 2 ;;
    --wait-timeout) wait_timeout="$2"; shift 2 ;;
    --lock) lock="$2"; shift 2 ;;
    --log) log="$2"; shift 2 ;;
    --interval) interval="$2"; shift 2 ;;
    --) shift; break ;;
    -h|--help) sed -n '2,28p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "eak-res: unknown option $1 (use -- before the command)" >&2; exit 64 ;;
  esac
done
[ $# -gt 0 ] || { echo "eak-res: no command given" >&2; exit 64; }
[ -r /proc/meminfo ] || { echo "eak-res: needs /proc (Linux only)" >&2; exit 69; }
[ -n "$label" ] || label="$(basename "$1")"

now() { date +%s.%N; }
mem_available_mb() { awk '/^MemAvailable:/ {printf "%d", $2/1024}' /proc/meminfo; }
swap_used_mb() { awk '/^SwapTotal:/ {t=$2} /^SwapFree:/ {f=$2} END {printf "%d", (t-f)/1024}' /proc/meminfo; }
# Sum of RSS (MB) of process $1 and every descendant.
tree_rss_mb() {
  ps -eo pid=,ppid=,rss= 2>/dev/null | awk -v root="$1" '
    { par[$1]=$2; rss[$1]=$3 }
    END {
      sum=0
      for (p in rss) { q=p; while (q != "" && q != 0 && q != root) q=par[q]; if (q == root) sum+=rss[p] }
      printf "%d", sum/1024
    }'
}

# 1. Serialise heavy work on the host. The lock fd stays open for the whole run.
lock_wait=0
if [ -n "$lock" ]; then
  mkdir -p "$(dirname "$lock")"
  exec 9>>"$lock"
  t0=$(now)
  flock 9 || { echo "eak-res: could not take lock $lock" >&2; exit 70; }
  lock_wait=$(awk -v a="$t0" -v b="$(now)" 'BEGIN {printf "%d", b-a}')
fi

# 2. Wait for enough free memory (never silently: print progress and the outcome).
mem_wait=0
if [ "$wait_mem" -gt 0 ]; then
  t0=$(now)
  while [ "$(mem_available_mb)" -lt "$wait_mem" ]; do
    waited=$(awk -v a="$t0" -v b="$(now)" 'BEGIN {printf "%d", b-a}')
    if [ "$waited" -ge "$wait_timeout" ]; then
      echo "eak-res: WARNING MemAvailable $(mem_available_mb) MB < $wait_mem MB after ${waited}s; running anyway" >&2
      break
    fi
    [ $((waited % 30)) -eq 0 ] && echo "eak-res: waiting for MemAvailable >= $wait_mem MB (now $(mem_available_mb) MB, ${waited}s)" >&2
    sleep 2
  done
  mem_wait=$(awk -v a="$t0" -v b="$(now)" 'BEGIN {printf "%d", b-a}')
fi

# 3. Run and sample.
swap_start=$(swap_used_mb)
peak_rss=0
min_avail=$(mem_available_mb)
peak_swap=$swap_start
t_start=$(now)
"$@" &
pid=$!
trap 'kill -TERM "$pid" 2>/dev/null' INT TERM
while kill -0 "$pid" 2>/dev/null; do
  rss=$(tree_rss_mb "$pid"); avail=$(mem_available_mb); sw=$(swap_used_mb)
  [ "${rss:-0}" -gt "$peak_rss" ] && peak_rss=$rss
  [ "$avail" -lt "$min_avail" ] && min_avail=$avail
  [ "$sw" -gt "$peak_swap" ] && peak_swap=$sw
  sleep "$interval"
done
wait "$pid"; code=$?
trap - INT TERM
t_end=$(now)
swap_end=$(swap_used_mb)
avail=$(mem_available_mb); [ "$avail" -lt "$min_avail" ] && min_avail=$avail
[ "$swap_end" -gt "$peak_swap" ] && peak_swap=$swap_end
duration=$(awk -v a="$t_start" -v b="$t_end" 'BEGIN {printf "%.1f", b-a}')
swap_delta=$((swap_end - swap_start))

# 4. Record.
mkdir -p "$(dirname "$log")"
[ -s "$log" ] || printf 'timestamp_utc\tlabel\tcommand\texit_code\tduration_s\tpeak_tree_rss_mb\tmin_mem_avail_mb\tpeak_swap_used_mb\tswap_delta_mb\tlock_wait_s\tmem_wait_s\n' >> "$log"
cmdline=$(printf '%s ' "$@" | tr '\t\n' '  ')
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$label" "${cmdline% }" \
  "$code" "$duration" "$peak_rss" "$min_avail" "$peak_swap" "$swap_delta" "$lock_wait" "$mem_wait" >> "$log"

status=PASS; [ "$code" -eq 0 ] || status="FAIL($code)"
echo "eak-res: [$label] $status ${duration}s | peak tree RSS ${peak_rss} MB | min MemAvailable ${min_avail} MB | peak swap ${peak_swap} MB (delta ${swap_delta} MB) | lock wait ${lock_wait}s" >&2
exit "$code"
