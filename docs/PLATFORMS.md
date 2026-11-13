# Platform support

| Platform | Status | Toolchain notes |
|---|---|---|
| Windows 10/11 x64 | Core crates built and tested locally (M1) | Rust GNU or MSVC toolchain. GNU needs MinGW-w64 GCC (`scripts/env.sh` finds WinLibs). Tauri needs WebView2 (preinstalled on Windows 11). |
| Ubuntu 22.04+ x64 | CI configured, not yet executed | `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev` for Tauri |
| macOS arm64 / x64 | CI configured (macos-latest, macos-13), not yet executed | Xcode command-line tools |

Portable by design:
- Paths: `std::path`.
- Process supervision: tree kill uses `taskkill /T` on Windows and process-group `kill` on Unix (`cfg` blocks in `eak-runtime/src/process.rs`).
- Integer geometry: no floating-point drift between platforms.
- SQLite: bundled, so no system library is needed.

Known limitations:
- Lightpanda (optional research browser) has no native Windows build.
- Installers (MSI/NSIS, .deb/AppImage, .dmg) are not produced yet.
