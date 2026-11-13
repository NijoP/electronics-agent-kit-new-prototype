#!/usr/bin/env bash
# Python-free policy scan (Execution Instructions §6.2). Exits non-zero on violation.
set -euo pipefail
cd "$(dirname "$0")/.."
bad=$(git ls-files | grep -E '\.py$|(^|/)requirements[^/]*\.txt$|(^|/)pyproject\.toml$|(^|/)setup\.py$|(^|/)Pipfile|(^|/)poetry\.lock$|(^|/)environment\.ya?ml$' || true)
if [ -n "$bad" ]; then echo "Python files are not allowed in EAK:"; echo "$bad"; exit 1; fi
echo "Python-free policy: OK"
