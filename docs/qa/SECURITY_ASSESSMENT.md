# Security assessment (installed f48158d + source at the same revision)

Scope: local, authorized, disposable instance. Source read plus a few loopback probes of the test instance's own endpoints. No exploit was developed. **No High or Critical finding was confirmed**, so no private exploit note exists. Dependency advisories were **not checked** (no `cargo audit` installed, no network use allowed): unknown, not "clean".

| ID | Area | Finding | Class | Severity | Confidence |
|---|---|---|---|---|---|
| S1 | Terminal env | PTY children inherit the whole GUI-process environment. A stub printed (names only) variables from the launching shell such as `*_TOKEN`, `*_API_KEY`, `GIT_CONFIG_KEY_0`, plus EAK's own `EAK_HARNESS_TOKEN_FILE`, `EAK_TERMINAL_API_FILE`, `EAK_TERMINAL_SESSION_ID`. Normal terminal behaviour, but any agent started from EAK sees the secrets of whatever launched EAK. Fix: scrub with an allow/deny list for agent launchers. | Confirmed behaviour; design weakness | Medium (no CVSS) | High |
| S2 | CSP | `style-src 'unsafe-inline'`; script-src is `'self' 'wasm-unsafe-eval'`, no remote origins, `connect-src ipc:` only. | Configuration weakness | Low | High |
| S3 | Capabilities | `core:default` + `dialog:allow-open` only; no shell/fs plugin; project access only via EAK IPC. | Informational (good) | Info | High |
| S4 | Harness endpoint | Linux: unix socket + token file 0600 in a 0700 dir, constant-time compare. Stale `eak-<pid>.sock`/`token-<pid>` remained after the process was killed (SIGTERM). `.harness.json` is 0644 but holds no token. | Low (stale files) | Low | High |
| S5 | Terminal control API | Binds `127.0.0.1` ephemeral TCP (not remote). Wrong token gives `unauthorized` (verified live); token file `terminal-api.json` is 0600. Any local process can reach the port but needs the token. No rate limit seen. | Potential exposure (local only) | Low | Medium |
| S6 | Filesystem | Workspace file ops reject `..`, absolute paths, drive prefixes and canonicalise to catch symlink escape (`apps/desktop/src-tauri/src/workspace.rs:174-245`, with unit tests). **Not exercised through the UI** (the attempt did not reach the name field): source-only claim. | Not confirmed | n/a | Medium |
| S7 | Rendering | No `dangerouslySetInnerHTML`/`innerHTML` in non-test frontend source (grep). Injection strings were not rendered live. | Not confirmed | n/a | Medium |
| S8 | Credentials | API keys are said to go to the OS credential store (`keyring`); not tested (no keys entered). | Not tested | n/a | n/a |
| S9 | WebKit inspector | Listened on loopback only because the evaluator set `WEBKIT_INSPECTOR_SERVER`; the shipped app does not set it. | Informational | Info | High |

CVSS was not scored: S1's attack conditions depend on deployment. Not tested: payload size limits, stale-revision refusal over IPC, malformed IPC, project import/export containment, symlink handling, dependency advisories.
