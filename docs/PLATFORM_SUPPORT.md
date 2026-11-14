# Platform support matrix

Status: **nothing here has been built, run or tested yet** (workflows are disabled by the project owner's
testing freeze). The installers and packaging config are code-reviewed only; the matrix below is the intended
support, and an installer stops with a clear message when a release lacks the asset for your OS/CPU.

| Platform | Package | Built in CI | CI-smoke | Real hardware |
|---|---|---|---|---|
| Linux x86_64 (Ubuntu 22.04 build) | AppImage, .deb | no | no | **no** (package never installed) |
| Linux aarch64 | none | no | no | no |
| Windows 10/11 x64 | NSIS setup.exe | no | no | **no** |
| macOS 14+ Apple Silicon | .dmg / .app | no | no | **no** |
| macOS Intel x86_64 | .dmg / .app | no | no | **no** |

Signing/notarization: not done (no credentials).
See `docs/INSTALL.md` for SmartScreen / Gatekeeper behaviour.
