# KiCad library architecture audit (Milestone 0)

Status: static-review only, not runtime verified. Nothing from KiCad was built or run.

## What was inspected
| Item | Version |
|---|---|
| `KiCad/kicad-source-mirror` (GPL-3.0) | `master` @ `52b43fb8fc8e11fdab5d683eaefc91a49f9cbcd7` (2026-11-15) |
| `kicad/libraries/kicad-symbols` | `master` @ `9fa442824ffd3786b0c420b5d7e45f9a461b7c72` (2026-11-12) |
| `kicad/libraries/kicad-footprints` | `master` @ `a0dcd398e7ea4b00cdeb91efc35f09cceaa9097d` (2026-11-02) |
| `kicad/libraries/kicad-packages3D` | `master` @ `afa6aab16f43a736cd062b8c853863e753653921` (2026-11-03) |

Files actually opened: `include/lib_id.h` (header and doc comments), and the file listings of
`common/` and `include/` at that commit (`lib_id.cpp`, `footprint_info.cpp`, `lib_table_grid_tricks.cpp`,
`lib_table_notebook_panel.cpp`, `lib_table.keywords`, `footprint_info.h`, `lib_table_*.h`). The three
library repositories' `LICENSE.md` were read. **Not read line by line:** the schematic/PCB IO plugins, the
library tables' parser, the symbol/footprint chooser dialogs and the 3D resolver. Statements about those
below are KiCad's documented behaviour at concept level and are labelled so; they must be re-checked
against source before any behaviour is claimed as matching.

## Findings that drive EAK
1. **Identifier = nickname + item.** `LIB_ID` is "a dyad of the library nickname and the name of the item
   in the library" (`smt:R_0805`, header comments). Display names are not identity. EAK adopts
   `library_id:item_id` (`QualifiedId`, `eak-core/src/libmeta.rs`).
2. **Nickname indirection via library tables** (concept level): a table maps nickname to location, with a
   global table and a project table; a missing nickname is a resolvable error, not a crash. EAK adopts
   scopes (System, User, Project, Imported, Organisation) and a catalogue with explicit `MissingLibrary`
   and `Disabled` states.
3. **Definition vs instance** (concept level): a placed schematic symbol carries a copy of the library
   symbol in the schematic, so the library can change without altering the sheet; PCB footprints are
   likewise instance copies. EAK's equivalent is `DefRef` (stable id + library version + content hash)
   stored with the placed instance; `LibraryCatalog::check_ref` reports `Drifted` instead of replacing.
4. **Footprint assignment** is a separate, editable association between symbol and footprint (concept
   level). EAK already has `pin_map` (pin number to pad numbers) in `ComponentDef` (ADR-0015).

## Licensing constraints
KiCad application source is GPL-3.0: no code is copied. The library repositories are CC-BY-SA 4.0 with
KiCad's design-use exception text (read in each `LICENSE.md`); importing user-supplied KiCad libraries
keeps the original licence in `LibraryManifest.license` and the attribution in `source`. Bundling KiCad
library content in EAK releases is **not** done and needs a separate decision (see
`docs/legal/THIRD_PARTY_REUSE_REVIEW.md`).

## Proposed EAK architecture (this milestone and next)
- M1 (done in source): identity, scope, maturity, content-hash manifest, catalogue, committed references.
- M3 (not started): KiCad `.kicad_sym` / `.kicad_mod` importers written independently from the public file
  format documentation, with bounded size, preview, collision report and a never-silent-drop rule.
