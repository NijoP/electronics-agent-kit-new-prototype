# Low-memory development (8 GB laptop)

EAK must build, test and package on a 4-core / 8 GB Linux laptop. Everything here is local, free and
open source: no GitHub Actions, no paid services. Linux only (the monitor reads `/proc`).

## What is in the repo

| File | Purpose |
|---|---|
| `scripts/eak-res.sh` | Wraps one command. Samples `/proc/meminfo` and the command's process-tree RSS every 1 s and appends a TSV line to `target/eak-res/runs.tsv` (timestamp, label, command, exit code, duration, peak tree RSS, min MemAvailable, peak swap used, swap delta, lock wait, memory wait). Options: `--wait-mem MB` (wait for MemAvailable; proceeds with a printed warning after `--wait-timeout`, default 600 s), `--lock FILE` (flock), `--label`, `--log`. Returns the command's own exit code. |
| `scripts/test-batches.sh` | Ordered batches, each run through `eak-res.sh`. A failing batch never stops later ones; exit code is non-zero if any batch failed; the summary lists passed and failed batches by name and says what was not run. |
| `scripts/ci-local.sh` | Documented local-CI entry point (calls `test-batches.sh`). |
| `.cargo/config.toml` | `build.jobs = 2`; dev/test `debug = "line-tables-only"`; dependencies without debug info. |
| `apps/desktop/vite.config.ts` | vitest: forked workers, `maxWorkers: 2`; vite build: no source maps, no gzip-size pass. |

## Batch order (`scripts/test-batches.sh`)

1. **light** (`light:fmt`, `light:no-python`, `light:tsc`, `light:vitest`): formatting, Python-free policy, frontend types and unit tests.
2. **per-crate Rust tests**, one crate at a time with `--release`: `eak-core`, `eak-engineering`, `eak-validation`, `eak-manufacturing`, `eak-intelligence` (includes the LCSC/JLCPCB provider tests), `eak-runtime` (`--test-threads=2`), `eak-desktop`.
3. **clippy**: `cargo clippy --workspace --all-targets -- -D warnings`.
4. **package:tauri** (`npx tauri build --no-bundle`): only with `--package`; otherwise the summary says it was not run.

No test is removed or skipped to save memory; the split only changes the order and the concurrency.

## How to run

```bash
scripts/ci-local.sh                 # everything except packaging
scripts/ci-local.sh --package       # plus the Tauri packaging smoke build
scripts/test-batches.sh --only light            # one group (light, test, clippy) or one batch name
scripts/test-batches.sh --no-lock               # private machine / private target dir
scripts/test-batches.sh --lock-once             # one flock for the whole run
EAK_BUILD_LOCK=/path/to/lock EAK_WAIT_MEM=500 EAK_VITEST_WORKERS=2 EAK_NODE_HEAP_MB=1536 scripts/test-batches.sh
column -t -s$'\t' target/eak-res/runs.tsv       # read the results
```

Defaults: `CARGO_BUILD_JOBS=2`, `NODE_OPTIONS=--max-old-space-size=1536`, wait for 500 MB MemAvailable
(the host has 12 GB of swap/zram to absorb the rest). On a shared host every heavy job should go
through one lock file (`EAK_BUILD_LOCK`); per-batch locking on a busy host can queue for a long time
(the first batch of the first attempt waited 3199 s behind other workers), so prefer a private
target dir (`CARGO_TARGET_DIR`) when the machine is shared.

## Measured on this host (PARTIAL)

Host: 4 cores, 7.8 GB RAM, 12 GB swap (file + zram), shared with other build jobs, so swap figures are
whole-host numbers and include other processes. Wrapped with `eak-res.sh`, 1 s sampling, commit
based on `ea1efeb`, 2026-11-14.

| Batch | Result | Duration | Peak tree RSS | Min MemAvailable | Peak swap used (host) |
|---|---|---|---|---|---|
| light:fmt | PASS | 7.4 s | 45 MB | 1766 MB | 4895 MB |
| light:no-python | PASS | 1.2 s | 0 MB (finished between samples) | 1770 MB | 4895 MB |
| light:tsc | PASS | 52.1 s | 367 MB | 1614 MB | 4913 MB |
| light:vitest (10 files, 65 tests) | PASS | 118.2 s | 552 MB | 1311 MB | 4913 MB |
| test:* (7 crates), clippy, package:tauri | **not measured yet** | | | | |

The Rust test, clippy and packaging batches have not been run through the monitor yet: a first
attempt queued for the shared build lock for more than 50 minutes. They are to be run on the frozen
main commit with a private target dir and recorded in `TEST_RESULTS.md`.

## Decisions and what was not measured

- `build.jobs = 2` and `line-tables-only` are configured from the known memory behaviour of rustc/linker; their effect on peak RAM was **not** A/B measured yet.
- Linker: neither `lld` nor `mold` is installed; rustc 1.97 on x86_64 Linux already links with its bundled `rust-lld` by default, so nothing was added.
- `split-debuginfo` was not set: Cargo cannot make it per-OS in a profile and `line-tables-only` leaves little to split.
- Release profile is unchanged (`lto = "thin"`, `codegen-units = 4`, `strip`): it is the packaging profile and no measurement showed a lower peak with another setting.
- The monitor samples once per second, so a spike shorter than that can be missed.
