# Platform support

Windows, Linux and macOS are equal targets by design (no platform assumptions outside `cfg` blocks).
In practice, evidence is thin. Nothing has been built or run since the testing freeze.

| Platform | Published package | Core crates | Desktop app | Terminal workspace / harness |
|---|---|---|---|---|
| Linux x86_64 | v0.1.4 `.deb` and AppImage (built before this refactor) | built and tested before the freeze | package never verified on real hardware | engine PTY tests passed on Linux before the freeze; UI and harness code-reviewed only |
| Linux aarch64 | none | unverified | unverified | unverified |
| Windows 10/11 x64 | **none yet** | built and tested locally earlier (GNU toolchain) | unverified | unverified (ConPTY path never run; harness uses loopback TCP) |
| macOS arm64 / Intel | **none yet** | unverified | unverified | unverified |

Installers are unsigned; an installer stops with a clear message when a release lacks the asset for
the OS/CPU. See `docs/INSTALL.md`.

Portable by design: `std::path`; tree-kill via `taskkill /T` (Windows) or process-group kill (Unix)
in `cfg` blocks; integer-nanometre geometry; bundled SQLite; `portable-pty` (Unix PTY / ConPTY).
Per-OS build prerequisites: `docs/DEVELOPMENT.md`. Limitation: the optional research browser has
no native Windows build.
