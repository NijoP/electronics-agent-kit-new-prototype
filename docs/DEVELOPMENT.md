# Development

Rules: `AGENTS.md`. Tests by level: `TESTING.md`. Decisions: `docs/decisions/`.

## Prerequisites
- Stable Rust 1.80+, Node.js 22+ with npm.
- Linux: `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev` (plus `libdbus-1-dev pkg-config` for the keyring).
- macOS: Xcode command-line tools. Windows: WebView2, MSVC build tools or MinGW-w64 (`source scripts/env.sh` in Git Bash).
- No Python anywhere (`scripts/check-no-python.sh`).

## Run and check
```bash
cd apps/desktop && npm ci && npm run tauri dev
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --release
bash scripts/check-no-python.sh && (cd apps/desktop && npm test)
```
Low-memory machines: `docs/dev/LOW_MEMORY_DEVELOPMENT.md`, `scripts/ci-local.sh`. The only GitHub
workflow, `.github/workflows/ci.yml`, is disabled on GitHub; add no workflows.

## Adding a workspace
Drop `apps/desktop/src/workspaces/<name>.workspace.tsx` that calls `registerWorkspace`
(`apps/desktop/src/workspaces/README.md`). The Terminal workspace is registered this way.

## Third-party code
Only under `third_party/licenses/` with notices; see `docs/LICENSES_AND_ATTRIBUTION.md`.
