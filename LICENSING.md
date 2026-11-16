# Licensing

EAK is licensed under the [O'Saasy License](LICENSE) (<https://osaasy.dev>), copyright (c) 2026 NijoP. In one line: free to use, modify and self-host, but you may not offer EAK itself to third parties as a competing hosted or SaaS product. The licence text is used verbatim from the official source. Releases published before the relicensing commit were published under MIT OR Apache-2.0 and stay available under those terms. The workspace `license` is `LicenseRef-OSaasy` (not an SPDX identifier) with `license-file = "LICENSE"`; the npm packages say `SEE LICENSE IN LICENSE`. One small piece of Apache-2.0 third-party source is included with its notices in `third_party/licenses/` (unchanged).

## Rust dependencies (direct)
| Crate | Licence | Use |
|---|---|---|
| serde, serde_json | MIT OR Apache-2.0 | serialisation |
| thiserror | MIT OR Apache-2.0 | errors |
| sha2 | MIT OR Apache-2.0 | hashes for evidence/manifest |
| uuid | MIT OR Apache-2.0 | object ids |
| rusqlite (bundled SQLite) | MIT; SQLite is public domain | metadata index |
| tauri, tauri-build (2.x) | MIT OR Apache-2.0 | desktop shell (`apps/desktop/src-tauri`) |
| keyring 3 (`apple-native`, `windows-native`, `sync-secret-service`, `crypto-rust`) | MIT OR Apache-2.0 | OS credential storage for model API keys (`apps/desktop/src-tauri`); transitive backends not yet inventoried |
| ureq 3 (`rustls` feature; optional, `remote-http`) | MIT OR Apache-2.0 | HTTPS client for model providers (`eak-intelligence`) |

### Crates added by `eak-intelligence` (feature `remote-http`)
Generated with `cargo metadata --all-features` by diffing the lockfile against `main`. All of them
reach the build only through the optional `ureq` dependency, which the `remote-http` feature
enables; the default build compiles none of them. None is copyleft: the licences are MIT,
Apache-2.0, ISC, BSD-3-Clause and CDLA-Permissive-2.0.

| Crate | Version | Licence |
|---|---|---|
| base64 | 0.23.1 | MIT OR Apache-2.0 |
| bytes | 1.12.1 | MIT |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| http | 1.5.0 | MIT OR Apache-2.0 |
| httparse | 1.10.1 | MIT OR Apache-2.0 |
| log | 0.4.35 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| ring | 0.17.14 | Apache-2.0 AND ISC |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT |
| rustls-pki-types | 1.16.0 | MIT OR Apache-2.0 |
| rustls-webpki | 0.103.17 | ISC |
| subtle | 2.6.1 | BSD-3-Clause |
| untrusted | 0.9.0 | ISC |
| ureq | 3.5.2 | MIT OR Apache-2.0 |
| ureq-proto | 0.7.1 | MIT OR Apache-2.0 |
| utf8-zero | 0.8.1 | MIT OR Apache-2.0 |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| webpki-roots | 1.0.12 | CDLA-Permissive-2.0 |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 |
| zeroize | 1.9.1 | Apache-2.0 OR MIT |

A full transitive inventory (`cargo deny`/`cargo about`) is required before the first installer is produced.

## Frontend dependencies (direct, `apps/desktop/package.json`)
| Package | Licence | Use |
|---|---|---|
| react, react-dom | MIT | UI |
| @tauri-apps/api, @tauri-apps/cli | MIT OR Apache-2.0 | IPC client, build CLI |
| vite, @vitejs/plugin-react | MIT | dev server and bundler |
| typescript | Apache-2.0 | type checking |
| vitest, jsdom, @testing-library/react | MIT | headless UI tests (dev only) |
| @types/react, @types/react-dom | MIT | type definitions (dev only) |
The app icon (`apps/desktop/src-tauri/icons/icon.svg` and the generated PNG/ICO/ICNS files) is original to EAK.

## Reference projects
These are concepts only and none is a dependency; see docs/REPOSITORY_RESEARCH.md. Restrictions:
- Lightpanda (AGPL-3.0): never bundled, linked or modified without legal review.
- Rune (GPL-3.0): no code reuse.
- SoloJiang/weft: no licence was detected, so no code reuse.
- Cursor: proprietary; behavioural inspiration only.

## Engineering data
Built-in parts are generic definitions. The AMS1117 and ATtiny85 pinouts follow publicly published
datasheets; no datasheet content is redistributed.

## DESK domain registry data
`crates/eak-intelligence/data/desk_domains.json` is derived from the owner's DESK repository
(github.com/NijoP/DESK, `library/pcb_domains`, MIT licence). It is reference data (class names,
keywords, constraint and verification prompts, standard identifiers) with no standard text.
DESK's Python code is not used; the classifier is re-implemented in Rust.

## ngspice (optional external tool, not bundled)
`eak-simulation` runs a user-installed `ngspice` executable as a subprocess (ADR-0013). Nothing
from ngspice is copied, linked or redistributed by EAK. The Debian package (ngspice 42,
`/usr/share/doc/ngspice/copyright`) declares the upstream source as BSD-3-Clause (documentation
BSD-3-Clause / CC-BY-SA-4.0 / public domain; some build-system files GPL-2+, not used by EAK).
