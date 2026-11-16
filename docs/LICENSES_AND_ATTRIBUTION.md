# Third-party licences (terminal workspace)

See also `LICENSING.md` for the rest of EAK. `crates/eak-terminal/src/agents.rs` is derived
from Apache-2.0 third-party source; its licence, upstream commit and modifications are recorded
in `third_party/licenses/THIRD_PARTY_NOTICES.md`. No third-party terminal binary is bundled.

| Component | Licence | Use |
|---|---|---|
| portable-pty 0.9 | MIT | Pseudo-terminals (Unix PTY, Windows ConPTY) in `crates/eak-terminal` |
| @xterm/xterm 6, @xterm/addon-fit, @xterm/addon-search | MIT | Terminal rendering in the desktop UI |

## EAK's own licence and the rest
EAK is under the O'Saasy License (`LICENSE`; releases before the relicensing were MIT OR Apache-2.0). Rust dependency licences are in
`LICENSING.md`. The only derived third-party source is recorded in
`third_party/licenses/THIRD_PARTY_NOTICES.md` (licence text, upstream commit, modifications);
it is the one place where that upstream project is named. EasyEDA and KiCad were studied for
requirements only; no source was copied (`docs/research/LICENCE_AND_ATTRIBUTION_AUDIT.md`).

## Earlier EAK engineering repositories
`crates/eak-core/src/units.rs` is reused, with changes, from the author's earlier prototype
`NijoP/electronics-agent-kit-proto` at `79f4e9d21f4d0b0577321de165270bebe8bb7db7` (same owner;
EAK's licence applies; attribution in the file header). DESK (`NijoP/DESK`, MIT) at
`5ad8492c37a5dac919d97f4eccd8250780490e7a` was used as an engineering knowledge source only; no
DESK code is included. Inventory: `docs/engineering/INTELLIGENCE_INTEGRATION.md`.

## 3D component models (ADR-0025)
| Item | Licence | Decision |
|---|---|---|
| three.js (incl. VRMLLoader, GLTFLoader, OrbitControls) | MIT | Dependency, as before. |
| occt-import-js 0.0.23 (wraps OpenCascade) | LGPL-2.1 | Unmodified npm dependency (`apps/desktop/package.json`), loaded lazily as its own chunk and only when a STEP model is present. Never vendored or modified; the unmodified package can be swapped by relinking the app. Licence notice ships with the npm package. |
| opencascade.js | LGPL | Not used. |
| KiCad 3D packages | CC-BY-SA-4.0 with a library exception | Not bundled or copied. Users point a component at a file in their own local library; the file's licence is recorded in `ModelRef.licence`. |
| easyeda2kicad (AGPL) | AGPL | Reference only; no code reused, no EasyEDA/Distributor model fetching implemented. |
