# ADR-0033: Library scopes, a cross-scope browser, and supplier discovery separate from engineering import

Status: accepted (code-reviewed only; nothing built or run on the authoring machine — CI is the first compile)

## Context
Directive part 2: a real library-management system with distinct sources, a browser (search,
categories, manufacturer/source/package filters, favourites, tags, recents, previews, pin/pad
inspection, datasheet, 3D, supplier availability), and two separate operations — *supplier
discovery* (find and store the supplier product and its commercial data) and *engineering library
import* (create or attach a validated definition) — without ever inventing a pinout, footprint,
pad dimension or rating. A supplier result must not be rejected because EAK has no symbol and
footprint for it, and must not be treated as engineering-approved either.

## Decision
- **No IR change.** Scopes are a view over existing `ComponentDef`s plus files outside the project:
  - `builtin` (shipped) and `project` (the open project's `library`) already existed;
  - `personal`: `<app config>/library/personal/<id>.json`, one `ComponentDef` per file, available to
    every project; saving copies a project/built-in/imported definition unchanged (pin map and ratings
    kept) and must pass `ComponentDef::validate`;
  - `imported:<name>`: `<app config>/library/imported/<name>/` from a folder the user names, which must
    contain `manifest.json` {name, licence, source_url}. **No licence, no import.** Only definitions that
    validate are kept; their provenance records library, licence and source and says "not verified by EAK".
    Nothing is downloaded or copied automatically, and no third-party (e.g. EasyEDA) asset is used.
  - supplier records and Distributor sourcing records stay in `<project>/sourcing` (unchanged); Fabricator assembly
    data is a separate model (ADR-0034).
- Entries are keyed by (scope, id) and never merged: the same id in two scopes is two rows; the same MPN
  text from two manufacturers is two products.
- `libstore.rs` (desktop): `library_browse` (merged, filtered by source/category/manufacturer/package/tag/
  favourites/recent, with facets that count the text-query matches before the other filters, and every
  unreadable or invalid library file listed in `problems` instead of dropped), `library_meta`
  (favourite / tags / "used just now" — `meta.json`, tags normalised and bounded, recents bounded),
  `library_personal_save|remove`, `library_import_open_source`, `library_imported_remove`. Names that become
  file names are restricted to `[A-Za-z0-9_.-]`, no leading dot.
- **Linking** (`eak-manufacturing::library_link`): `link_record(listing, defs)` says what a supplier listing is:
  `existing_eak` (a definition carries the Distributor number, or the same normalised MPN + manufacturer),
  `imported_definition` (an `DISTRIBUTOR_*` import), `supplier_only` (no definition, but a library template can
  carry it — named), or `needs_authoring` (no trustworthy symbol/footprint: keep the listing, author later).
  `unverified` is set independently when the matching definition has unverified ratings or no datasheet,
  and is always set for the last two. An MPN match from a different manufacturer is flagged and does not
  make the listing "existing".
- **Supplier discovery ≠ engineering import**: new `parts_save_record` stores the Distributor listing in
  `<project>/sourcing` with no engine command, no revision change and no definition; the catalogue shows each
  listing's standing, offers "Save record" always, and offers "Import" only when a template can carry it
  (`supplier_only`). `parts_import` is unchanged (template reuse only; electrical ratings stay unverified).
- Frontend: `LibraryBrowser.tsx` (lazy, inside the Parts panel) and link badges in `CataloguePanel.tsx`.
  The shell `LibraryPanel` (placement, footprint assignment, supplier properties) is untouched.
- New IPC commands: `parts_save_record`, `library_browse`, `library_meta`, `library_personal_save`,
  `library_personal_remove`, `library_import_open_source`, `library_imported_remove`.

## Not done (honest limits)
- The "needs authoring" path ends at a clear message and a saved supplier record; there is no guided
  symbol/footprint authoring wizard in this slice (the schematic/PCB library editors exist separately).
- Importing a symbol/footprint from an external library format is not implemented: only EAK
  `ComponentDef` JSON is read, so no pinout can be mis-translated. Footprint previews for supplier-only
  listings are not shown (there is no footprint to show).
- Favourites, tags and recents are per machine (config dir), not per project, and are not synced.
- Bulk BOM matching (EasyEDA device-standardization style) is not implemented; matching is per listing
  and explainable (ADR-0032 `matching`, this ADR's `link_record`).

## Consequences
Components with valid Distributor listings are now classified against all four library scopes instead of being
refused; a listing is never discarded for lacking an engineering definition; the engineering state
(unverified / needs authoring) is explicit everywhere it is shown.
