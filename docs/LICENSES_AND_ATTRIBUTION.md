# Third-party licences (terminal workspace)

See also `LICENSING.md` for the rest of EAK. `crates/eak-terminal/src/agents.rs` is derived
from Apache-2.0 third-party source; its licence, upstream commit and modifications are recorded
in `third_party/licenses/THIRD_PARTY_NOTICES.md`. No third-party terminal binary is bundled.

| Component | Licence | Use |
|---|---|---|
| portable-pty 0.9 | MIT | Pseudo-terminals (Unix PTY, Windows ConPTY) in `crates/eak-terminal` |
| @xterm/xterm 6, @xterm/addon-fit, @xterm/addon-search | MIT | Terminal rendering in the desktop UI |

## EAK's own licence and the rest
EAK is MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`). Rust dependency licences are in
`LICENSING.md`. The only derived third-party source is recorded in
`third_party/licenses/THIRD_PARTY_NOTICES.md` (licence text, upstream commit, modifications);
it is the one place where that upstream project is named. EasyEDA and KiCad were studied for
requirements only; no source was copied (`docs/research/LICENCE_AND_ATTRIBUTION_AUDIT.md`).
