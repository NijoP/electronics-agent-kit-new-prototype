# Platform support matrix

Status: **nothing here has been built, run or tested yet** (workflows are disabled by the project owner's
testing freeze). The installers and packaging config are code-reviewed only; the matrix below is the intended
support, and an installer stops with a clear message when a release lacks the asset for your OS/CPU.

| Platform | Package | Built in CI | CI-smoke | Real hardware | Herdr runtime |
|---|---|---|---|---|---|
| Linux x86_64 (Ubuntu 22.04 build) | AppImage, .deb | no | no | **no** (package never installed) | bundled (upstream prebuilt v0.9.5) |
| Linux aarch64 | none | no | no | no | upstream publishes one; EAK package not built yet |
| Windows 10/11 x64 | NSIS setup.exe | no | no | **no** | bundled (+ ConPTY files); upstream marks Windows beta |
| macOS 14+ Apple Silicon | .dmg / .app | no | no | **no** | bundled |
| macOS Intel x86_64 | .dmg / .app | no | no | **no** | bundled |

Signing/notarization: not done (no credentials). Herdr is Apache-2.0 upstream, pinned and SHA-256-verified in
`packaging/herdr.pin`; it is fetched as a release binary rather than compiled from source.
See `docs/INSTALL.md` for SmartScreen / Gatekeeper behaviour.
