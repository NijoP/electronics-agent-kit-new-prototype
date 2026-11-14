# Third-party licences and attribution (terminal workspace)

See also `LICENSING.md` for the rest of EAK.

| Component | Licence | How EAK uses it |
|---|---|---|
| Herdr (herdrdev/herdr), studied at `ee5bfb5`, run as v0.7.4+ | Apache-2.0 | Separate program run as a supervised sidecar (`docs/HERDR_INTEGRATION.md`). No Herdr source is copied into EAK; ideas adapted in `eak-terminal` (foreground-process state, agent self-reports, `wait`) are reimplemented. Herdr is installed and distributed by its own project with its own LICENSE/NOTICE. |
| portable-pty 0.9 | MIT | Pseudo-terminals (Unix PTY, Windows ConPTY) in `crates/eak-terminal`. |
| @xterm/xterm, @xterm/addon-fit, @xterm/addon-search | MIT | Terminal rendering in the desktop UI. |
