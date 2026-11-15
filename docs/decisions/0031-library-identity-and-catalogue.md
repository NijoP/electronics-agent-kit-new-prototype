# ADR-0031 Library identity, scope and committed-definition references

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
