# Third-party source notices

Source code derived from these projects is included in EAK under their licences. Each
derived file carries the upstream copyright notice and a "Modified by EAK" note.

| Project | Licence | Pinned commit | Record |
|---|---|---|---|
| herdr | Apache-2.0 | `ee5bfb5041bb3e4a49d591c72308aae3c81ecdde` | [`herdr/UPSTREAM.md`](herdr/UPSTREAM.md), [`herdr/LICENSE`](herdr/LICENSE) |
| DESK | MIT | `5ad8492c37a5dac919d97f4eccd8250780490e7a` | [`DESK/LICENSE`](DESK/LICENSE); net-priority model ported (re-written in TypeScript/Rust, no DESK source copied); mental models M1-M6, 12-step schematic procedure and component derating facts re-expressed as typed Rust data in `eak-intelligence/src/mental.rs` |

## three.js (npm dependency, 3D board viewer)

MIT licence, Copyright (c) 2010-2026 three.js authors. Used unmodified as an npm package by `apps/desktop/src/viewer3d`; no source is vendored.

## JetBrains Mono (bundled font, schematic symbol text)

JetBrains Mono NL Regular 2.304, Copyright 2020 The JetBrains Mono Project Authors
(https://github.com/JetBrains/JetBrainsMono), SIL Open Font License 1.1: [`JetBrainsMono/OFL.txt`](JetBrainsMono/OFL.txt).
Shipped unmodified as `apps/desktop/src/assets/fonts/JetBrainsMonoNL-Regular.ttf`.
