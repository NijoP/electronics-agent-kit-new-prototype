# Platform support

Windows, Linux and macOS are equal targets by design (no platform assumptions outside `cfg` blocks).
In practice, evidence is thin. Nothing has been built or run since the testing freeze.

| Platform | One-command source install (default) | Prebuilt package | Core crates | Desktop app | Terminal workspace / harness |
|---|---|---|---|---|---|
| Linux x86_64 (Debian/Ubuntu) | `install.sh`: written, **code-reviewed only, never run**; output: executable + `.desktop` launcher | v0.1.4 `.deb` and AppImage (built before this refactor; never verified on hardware) | built and tested before the freeze | **current `main` never built** | engine PTY tests passed on Linux before the freeze; UI and harness code-reviewed only |
| Linux x86_64 (Fedora, Arch, openSUSE) | `install.sh` can install packages with names from the Tauri v2 page (**unrun, unverified**) | none | as above | never built | as above |
| Linux x86_64 (other distributions) | stops with guidance unless the Tauri libraries are already present | none | as above | never built | as above |
| Linux aarch64 | allowed with a warning, **unverified** (WebKitGTK/Tauri dependencies not checked on ARM) | none | unverified | unverified | unverified |
| Windows 10/11 x64 | `install.ps1`: written, **code-reviewed only, never run** (not even parsed: no PowerShell was available); needs MSVC Build Tools + Windows SDK + WebView2; output: exe + Start Menu shortcut | **none** | built and tested locally earlier (GNU toolchain; the installer needs MSVC) | never built | unverified (ConPTY path never run; harness uses loopback TCP) |
| Windows ARM64 | **not supported**: the installer stops | none | unverified | unverified | unverified |
| macOS Apple Silicon / Intel | `install.sh`: written, **code-reviewed only, never run**; output: real `EAK.app` in `~/Applications`, ad-hoc signed locally, **not notarized** | **none** | unverified | never built | unverified |

"Supported" here means an installer path exists and was reviewed, not that anything works. Nothing in the
source-install path has been executed on any platform; the Rust workspace has not been compiled since Phase 1
and `Cargo.lock` was hand-merged, so the first real `--locked` build may fail on the lockfile or on compile errors.
Packages and installed binaries are unsigned. Not checked per platform: native library availability on other
distributions, DMG packaging (the installer builds only the `.app`), cross-compilation (never attempted; builds are native).
The installer stops with a clear message when a prebuilt release lacks the asset for the OS/CPU. See `docs/INSTALL.md`.

Portable by design: `std::path`; tree-kill via `taskkill /T` (Windows) or process-group kill (Unix)
in `cfg` blocks; integer-nanometre geometry; bundled SQLite; `portable-pty` (Unix PTY / ConPTY).
Per-OS build prerequisites: `docs/DEVELOPMENT.md`. Limitation: the optional research browser has
no native Windows build.
