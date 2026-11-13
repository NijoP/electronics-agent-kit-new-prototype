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
