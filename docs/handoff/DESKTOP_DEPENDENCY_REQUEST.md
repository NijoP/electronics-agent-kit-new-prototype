# EAK desktop: dependency approval request

Task: eak-desktop-shell. PR https://github.com/NijoP/electronics-agent-kit-new-prototype/pull/12 (branch `fm/eak-desktop-shell`, head 599bf91). The branch is pushed for CI and stays unmerged until approval.
Versions are as resolved in the branch's `Cargo.lock` and `apps/desktop/package-lock.json`.

## Summary

- **New Rust crates (direct):** 2, `tauri` and `tauri-build`. No other new direct crates.
- **SQLite:** not new. The recent-project index reuses `eak_runtime::index`, which already depends on `rusqlite 0.37` (bundled SQLite) in main. It is a rebuildable cache, never the canonical project format.
- **Transitive Rust impact:** the workspace `Cargo.lock` grows from 57 to 433 packages, almost entirely Tauri's own tree (wry, tao, webview bindings, serde, tokio, etc.).
  A full per-crate licence audit of that tree (for example with `cargo deny check licenses`) has **not** been run. That is an open item before approval if a transitive-licence guarantee is required.
- **npm:** 3 runtime packages are shipped in the app bundle. 9 direct dev-only packages are used only to build and test, and are never shipped. The lockfile resolves 4 runtime and 127 dev packages in total, counting transitive ones.
- **No Python, no backend services, no Node runtime in the shipped app.** Node/npm is build-time only. The shipped app is the Rust binary plus static frontend assets rendered by the OS webview.

## Rust (apps/desktop/src-tauri/Cargo.toml)

| Crate | Range / resolved | Kind | Purpose | Licence | Windows / Linux / macOS | Why not std or an existing dependency |
|---|---|---|---|---|---|---|
| `tauri` | `2` / 2.12.5 | runtime | The Tauri 2 desktop framework: window, OS webview, typed IPC commands from the React UI into Rust, bundling | Apache-2.0 OR MIT | Windows: WebView2 runtime (preinstalled on Windows 11, installer-bootstrapped on 10). Linux: webkit2gtk-4.1, gtk-3, libayatana-appindicator3, librsvg2 (CI installs the `-dev` packages via apt). macOS: system WKWebView, no extra libs. | Tauri 2 is the frozen-stack desktop shell. No std or existing crate provides a webview window or IPC. |
| `tauri-build` | `2` / 2.7.4 | build-dependency | Tauri's `build.rs` helper: generates the app context, permissions/capabilities and Windows resources at compile time | Apache-2.0 OR MIT | All three | Required by `tauri`; no alternative. |

Existing workspace crates used (not new): `eak-core`, `eak-runtime`, `eak-validation`, `serde`, `serde_json`.
The only root `Cargo.toml` change is enabling the `apps/desktop/src-tauri` workspace member line.

## npm runtime (shipped in the app bundle)

| Package | Range / resolved | Purpose | Licence | Platforms | Justification |
|---|---|---|---|---|---|
| `@tauri-apps/api` | `^2.12.4` / 2.12.4 | Frontend bindings that call the Rust `#[tauri::command]` functions (`invoke`) | Apache-2.0 OR MIT | All (pure JS) | Tauri's own IPC client. The alternative would be hand-rolling Tauri's internal IPC protocol, which is not a supported interface. |
| `react` | `^19.3.1` / 19.3.1 | UI component model | MIT | All (pure JS) | Frozen-stack frontend. |
| `react-dom` | `^19.3.1` / 19.3.1 | React renderer for the webview DOM | MIT | All (pure JS) | Required by React in a browser or webview. |

## npm dev-only (build and test, never shipped)

| Package | Range / resolved | Purpose | Licence | Platforms | Justification |
|---|---|---|---|---|---|
| `@tauri-apps/cli` | `^2.12.4` / 2.12.4 | `tauri build` / `tauri dev`: compiles and bundles the desktop app (installers) | Apache-2.0 OR MIT | Prebuilt native binaries for Win/Linux/macOS x64 and arm64 | The Tauri 2 build tool; needed for installable bundles. |
| `typescript` | `^5.9.0` / 5.9.3 | Type-checks the frontend (`tsc --noEmit`) | Apache-2.0 | All | Frozen-stack language. |
| `vite` | `^8.3.8` / 8.3.8 | Frontend dev server and production build | MIT | All | Frozen-stack build tool. |
| `@vitejs/plugin-react` | `^6.3.0` / 6.3.0 | Vite's official React JSX/TSX transform and fast refresh | MIT | All | Vite needs it to compile React components; maintained by the Vite team. |
| `vitest` | `^5.1.0` / 5.1.0 | Frontend unit-test runner (named in the routed task) | MIT | All | Vite-native: reuses the same config and transform, so no second build pipeline (Jest would need Babel/ts-jest). |
| `jsdom` | `^31.0.1` / 31.0.1 | In-process DOM so UI tests run headless in CI without a display or browser | MIT | All (pure JS) | Needed for component tests on CI runners that have no display. Not shipped. |
| `@testing-library/react` | `^16.3.3` / 16.3.3 | Renders components and queries them as a user would in tests | MIT | All | The standard React test utility. The alternative is hand-written render and act helpers duplicating it. |
| `@types/react` | `^19` / 19.3.0 | TypeScript type definitions for React | MIT | All | Types only, zero runtime. |
| `@types/react-dom` | `^19` / 19.3.0 | TypeScript type definitions for react-dom | MIT | All | Types only, zero runtime. |

## Proposed for the queued 3D viewport (not in this PR)

| Package | Current version | Kind | Purpose | Licence | Platforms | Justification |
|---|---|---|---|---|---|---|
| `three` | 0.187.0 | runtime, loaded on demand | WebGL 3D rendering of the board, components and copper from `eak-engineering` scene3d output | MIT | All (WebGL in WebView2, webkit2gtk and WKWebView) | Named in the frozen stack. Writing raw WebGL would duplicate a renderer. |
| `@react-three/fiber` | 9.8.2 (peer: react >=19 <19.4, three >=0.156; compatible with react 19.3.1) | runtime, loaded on demand | Declarative React renderer for three.js, so the 3D view is a React component sharing app state | MIT | All | Named in the frozen stack. Its Expo and React Native peers are optional and not installed. |

Both would be code-split (dynamic `import()`), so the 3D bundle loads only when the 3D view opens.

## Requested decision

Approve all of the following:
- the 2 Rust crates
- the 3 npm runtime packages
- the 9 npm dev-only packages
- the 2 3D packages, for the follow-on task

Or name any to reject. Open caveat: the transitive Rust licence audit has not been run.
