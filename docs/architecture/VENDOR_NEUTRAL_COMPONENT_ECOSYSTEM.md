# Vendor-neutral component ecosystem

Status: static-review only, not runtime verified. Milestone 1 (identity) is implemented in source with
test source added; the decoupling items below are classified individually.

## Principle
EAK's native library works offline and never needs a supplier response to create, edit, import or place a
component. Supplier data is generic, optional, stale-prone commercial metadata, never part of identity.

## Current coupling (grep at this commit, case-insensitive `lcsc|jlcpcb|easyeda`)
| Area | Where | Class |
|---|---|---|
| Core model | `eak-core/src/library.rs` (`ComponentDef.lcsc` legacy field, `supplier_part`, `set_offer`, `migrate_offers`, search over `lcsc`), `builtin.rs` (`lcsc: None` fields; one AMS1117 part carries `C6186`) | generic `supplier_offers` already exist (ADR-0028; desktop scope views in ADR-0033, ADR-0034 for assembly classification); legacy `lcsc` mirror remains for old files |
| Command | `eak-runtime/src/command.rs` (`lcsc` argument of a component edit) | legacy field; to become `supplier_offers` edit |
| Manufacturing | `eak-manufacturing/src/{lcsc,jlcpcb,provider,sourcing,bom}.rs` and tests/fixtures | optional adapter; network provider |
| Validation | `eak-validation/src/{sourcing,gates,dfm}.rs` | gate names/profile mention the fabricator |
| Harness/intelligence | `eak-harness`, `eak-intelligence` tools and knowledge | tool schemas mention supplier |
| Desktop | `PartsPanel.tsx`, `LibraryPanel.tsx`, `ManufacturingPanel.tsx`, `Workspace.tsx`, `types.ts`, `api.ts`, `src-tauri/src/parts.rs` | UI labels and IPC |
| Docs | `docs/research/EASYEDA_*`, ADR-0010, ADR-0014 | research/history |

## Plan and status
| Capability | Status |
|---|---|
| Stable library identity, scopes, maturity, content hash, catalogue, committed `DefRef` with drift detection (`eak-core/src/libmeta.rs`, ADR-0035) | Implemented in source; test source added; static-review only |
| Generic supplier metadata kept (`manufacturer`, `mpn`, `supplier_offers`) | Already present; unchanged |
| Placement independent of any supplier response | Already true for placement (library is local); not re-verified at runtime |
| Move legacy `lcsc` field behind `supplier_offers` only; remove from search keys and command arguments | Not yet done (needs IR migration and ADR; planned next) |
| Adapters (`lcsc`, `jlcpcb`) disabled by default, isolated, labelled third-party | Not yet done |
| Remove vendor branding from desktop navigation and labels | Not yet done |

Nothing is claimed as removed that is not listed as Implemented.
