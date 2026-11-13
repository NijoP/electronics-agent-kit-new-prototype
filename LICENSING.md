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
| tauri, tauri-build (2.x) | MIT OR Apache-2.0 | desktop shell (`apps/desktop/src-tauri`) |
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
