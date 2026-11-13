# Licensing

EAK is proprietary (see LICENSE). No third-party source code has been copied into this repository.

## Rust dependencies (direct)
| Crate | Licence | Use |
|---|---|---|
| serde, serde_json | MIT OR Apache-2.0 | serialisation |
| thiserror | MIT OR Apache-2.0 | errors |
| sha2 | MIT OR Apache-2.0 | hashes for evidence/manifest |
| uuid | MIT OR Apache-2.0 | object ids |
| rusqlite (bundled SQLite) | MIT; SQLite is public domain | metadata index |
| ureq 3 (`rustls` feature) | MIT OR Apache-2.0 | HTTPS client for model providers (`eak-intelligence`) |

### Transitive dependencies added with ureq (`eak-intelligence`)
| Crate | Licence |
|---|---|
| rustls | Apache-2.0 OR ISC OR MIT |
| ring | Apache-2.0 AND ISC |
| rustls-webpki, untrusted | ISC |
| webpki-roots (Mozilla CA bundle) | CDLA-Permissive-2.0 |
| subtle | BSD-3-Clause |
| bytes | MIT |
| zeroize | Apache-2.0 OR MIT |
| wasi | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| base64, getrandom 0.2, http, httparse, log, once_cell, percent-encoding, rustls-pki-types, ureq-proto, utf8-zero, windows-sys 0.52, windows-targets 0.52 and its per-target `windows_*` crates | MIT OR Apache-2.0 |

A full transitive inventory (`cargo deny`/`cargo about`) is required before the first installer is produced.

## Reference projects
These are concepts only and none is a dependency; see docs/REPOSITORY_RESEARCH.md. Restrictions:
- Lightpanda (AGPL-3.0): never bundled, linked or modified without legal review.
- Rune (GPL-3.0): no code reuse.
- SoloJiang/weft: no licence was detected, so no code reuse.
- Cursor: proprietary; behavioural inspiration only.

## Engineering data
Built-in parts are generic definitions. The AMS1117 and ATtiny85 pinouts follow publicly published
datasheets; no datasheet content is redistributed.
