# Platform support matrix

"CI-smoke" = the release workflow installs the produced package with the real install script on a native
GitHub-hosted runner, runs the bundled `herdr --version`, launches EAK (xvfb on Linux) and checks it stays up,
then uninstalls. "Real hardware" = a person ran it on a physical machine. Nothing is code-signed.

| Platform | Package | Built in CI | CI-smoke | Real hardware | Herdr runtime |
|---|---|---|---|---|---|
| Linux x86_64 (Ubuntu 22.04 build) | AppImage, .deb | yes | yes (+ `apt install` of the .deb) | yes (Linux dev machine, earlier source builds) | bundled (upstream prebuilt v0.9.5) |
| Linux aarch64 | none | no | no | no | upstream publishes one; EAK package not built yet |
| Windows 10/11 x64 | NSIS setup.exe | yes | yes | **no** | bundled (+ ConPTY files); upstream marks Windows beta |
| macOS 14+ Apple Silicon | .dmg / .app | yes | yes | **no** | bundled |
| macOS Intel x86_64 | .dmg / .app | yes | yes | **no** | bundled |

Signing/notarization: not done (no credentials). Herdr is Apache-2.0 upstream, pinned and SHA-256-verified in
`packaging/herdr.pin`; it is fetched as a release binary rather than compiled from source.
See `docs/INSTALL.md` for SmartScreen / Gatekeeper behaviour.
