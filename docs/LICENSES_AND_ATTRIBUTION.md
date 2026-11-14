# Third-party licences (terminal workspace)

See also `LICENSING.md` for the rest of EAK. `crates/eak-terminal/src/agents.rs` is derived
from Apache-2.0 third-party source; its licence, upstream commit and modifications are recorded
in `third_party/licenses/THIRD_PARTY_NOTICES.md`. No third-party terminal binary is bundled.

| Component | Licence | Use |
|---|---|---|
| portable-pty 0.9 | MIT | Pseudo-terminals (Unix PTY, Windows ConPTY) in `crates/eak-terminal` |
| @xterm/xterm 6, @xterm/addon-fit, @xterm/addon-search | MIT | Terminal rendering in the desktop UI |
