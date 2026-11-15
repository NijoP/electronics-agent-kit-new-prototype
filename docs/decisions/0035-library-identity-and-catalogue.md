# ADR-0035 Library identity, scope and committed-definition references

Status: accepted (static-review only, not runtime verified).

Context: components were addressed by free ids inside one built-in set; there was no library identity,
scope, version or content hash, so a library update could not be told apart from the definition a
project committed (see `docs/research/KICAD_LIBRARY_ARCHITECTURE_AUDIT.md`).

Decision: add `eak_core::libmeta` (not an IR change; no schema bump): `LibraryManifest`
(id, namespace, version, content hash, licence, source, scope, enabled), `QualifiedId` (`library:item`),
`LibraryMaturity` with evidence rules, an in-memory `LibraryCatalog` that rejects collisions, and
`DefRef` (id + library version + content hash) with `check_ref` returning `Current | Drifted |
MissingLibrary | Disabled`. Content hashes are computed by callers (a crate that already depends on
`sha2`); `eak-core` validates only their form, so no dependency is added.

Consequences: a later change may store `DefRef` on placed instances (an IR change needing its own ADR).
Supplier data stays generic metadata; nothing here depends on any supplier.

Relation to ADR-0033: that ADR defines the desktop-side scope views (builtin, project, personal,
imported) and the browser; this ADR adds the core identity/version/hash/drift types underneath them.
The desktop scopes are not yet wired to `LibraryCatalog`; that is the next slice.

## Vendor-neutral rename (migration note)
All supplier-specific names were replaced by generic ones (distributor offers/SKU, fabricator
profile). Project files written before this change stored the single legacy part-number key and a
supplier key under the former vendor-specific spelling; those keys are no longer read. Re-enter the
distributor SKU (or re-run supplier discovery) on affected components. Git history before this commit
still contains the former names (history is not rewritten). The optional distributor and fabricator
adapters point at placeholder `.invalid` hosts and are non-functional until a real endpoint is configured.

## KiCad independence
EAK does not depend on KiCad. Its native library format and IR are the source of truth. "Import from
KiCad" (`eak-core/src/kicad_import.rs`) is an optional one-way converter of user-supplied text files
into EAK-native definitions with provenance; no code path needs a KiCad install, KiCad libraries or
KiCad formats, and no KiCad library is bundled. UI and docs must call it "Import from KiCad", one
interchange option among others, never "KiCad libraries" as EAK's own.
