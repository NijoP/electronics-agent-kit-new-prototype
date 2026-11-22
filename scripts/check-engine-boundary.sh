#!/usr/bin/env bash
# Engine-boundary scan (AGENTS.md "no code writes project files directly"; ADR-0037).
# The desktop IPC layer may write a project folder only through `Engine::execute`,
# `Engine::save_evidence` or `Engine::write_project_record`. This scan fails when:
#   1. a desktop or harness source writes a sourcing/assembly record directly
#      (`sourcing::write_record` / `assembly::write_record` are for non-project folders);
#   2. a desktop source file that is not listed below uses a filesystem write primitive.
# Listed files write outside project folders (app config, the library store, caches) or are
# the file explorer, which refuses every engine-owned path. Adding a file here needs a reason.
# Run with `--self-test` to check that the scan itself still detects a violation.
set -euo pipefail
cd "$(dirname "$0")/.."

desktop=apps/desktop/src-tauri/src
primitives='fs::write|atomic_write|File::create|OpenOptions|fs::rename|fs::copy|remove_file|remove_dir_all|create_dir_all|create_dir\('
# file : reason
allowed="
catalogue.rs:distributor catalogue cache in the app config folder
kicad.rs:KiCad import into the user's library store (outside any project)
libcatalog.rs:enabled/disabled state beside the library store
libstore.rs:personal and imported library store (outside any project)
models.rs:model-provider settings in the app config folder
resources.rs:resource-policy settings in the app config folder
terminal.rs:test fixtures only
workspace.rs:project file explorer; engine-owned names are refused (ENGINE_OWNED)
"

scan() {
  local root=$1 fail=0
  local direct
  direct=$(grep -rnE '(sourcing|assembly)::write_record\(' "$root" crates/eak-harness/src 2>/dev/null || true)
  if [ -n "$direct" ]; then
    echo "Direct sourcing/assembly record writes (use Engine::write_project_record):"
    echo "$direct"
    fail=1
  fi
  local f name
  while IFS= read -r f; do
    name=${f#"$root"/}
    if ! grep -q "^${name}:" <<<"$allowed"; then
      echo "Filesystem write outside the engine in $f (not on the allow list):"
      grep -nE "$primitives" "$f"
      fail=1
    fi
  done < <(grep -rlE "$primitives" "$root" --include='*.rs' | sort)
  return $fail
}

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  printf 'fn f(d:&Path){ std::fs::write(d.join("sourcing/x.json"), "{}").unwrap(); }\n' >"$tmp/parts.rs"
  if scan "$tmp" >/dev/null; then echo "self-test: an unlisted direct write was not detected"; exit 1; fi
  printf 'fn f(d:&Path,r:&R){ sourcing::write_record(d, r).unwrap(); }\n' >"$tmp/parts.rs"
  if scan "$tmp" >/dev/null; then echo "self-test: a direct record write was not detected"; exit 1; fi
  echo "Engine-boundary self-test: OK"
  exit 0
fi

if ! scan "$desktop"; then exit 1; fi
echo "Engine boundary: OK"
