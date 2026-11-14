# Security notes

Prototype software; no security review has been done. Report issues privately to the maintainers.

- **Credentials:** model API keys live in the OS credential store only, never in project or profile files. External agent CLIs keep their own authentication; EAK does not read it.
- **Design writes:** every mutation is a typed `Command` through `Engine::execute` with a revision check, an approval policy (agent requests need an approval covering the risk class) and a journal naming the actor (`agent:<name>@<session>` for harness calls). Destructive harness commands need an explicit opt-in (`EAK_HARNESS_ALLOW_DESTRUCTIVE=1`). Code-reviewed only for the harness.
- **App endpoint:** the harness endpoint is token-authenticated and local only (Unix socket; loopback TCP on Windows), advertised in `<project>/.harness.json`; calls for a project the app does not have open are refused. Code-reviewed only; no penetration test.
- **Terminal control API:** token-authenticated loopback JSON API (`docs/terminal-api.md`). Terminal sessions are real OS processes with the user's privileges; closing a session signals only processes EAK spawned.
- **Network:** model providers, LCSC lookup (read-only) over HTTPS with rustls. Nothing is uploaded or ordered.
- **Installers:** unsigned; SHA-256 verified against the release `SHA256SUMS` (`docs/INSTALL.md`).
- Never commit secrets, copyrighted datasheets or API keys.
