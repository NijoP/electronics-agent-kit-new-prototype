# ADR-0034: Fabricator assembly classification kept separate from Distributor listings; fail-closed BOM warnings

Status: accepted (code-reviewed only; nothing built or run on the authoring machine — CI is the first compile)

## Context
Directive part 3: a part listed at Distributor is not thereby assemblable by Fabricator; basic / extended /
preferred-extended status must not be inferred from a category or from catalogue presence; the BOM
must carry Comment, Designator, Footprint and Distributor part number, group correctly, and warn clearly;
the CPL must follow the currently published format; nothing may be called manufacturing-ready and
nothing may be ordered.

## Decision
- **Format check** (Fabricator help pages, 2026): BOM columns `Comment, Designator, Footprint, Distributor Part #`
  (their pages also show the header `Fabricator Part #`; the existing `Distributor Part #` header is kept) and CPL
  columns `Designator, Mid X, Mid Y, Layer, Rotation` with millimetre coordinates. The existing
  `bom::fabricator_csv` and `fabricator::cpl_csv` already emit exactly these; no change was needed. BOM lines are
  grouped by (component id, value), so two different parts with the same displayed value are never merged.
  Bottom-side rotation/polarity conventions per package remain "not verified" in the manifest.
- **Assembly classification is its own record** (`eak-manufacturing::assembly`): `AssemblyRecord {distributor,
  class (unknown|basic|extended|preferred_extended|not_assemblable), source, verified_at, stock, unit_price}`
  stored in `<project>/sourcing/assembly/<Distributor>.json`. Distributor's documented OpenAPI has no such field, so a
  class only enters EAK by (a) the user's attestation (`assembly_attest`, `user_attested`) or (b) a Fabricator
  parts-library CSV the user provides (`assembly_import_parts_file`; header located by column name, rows
  limited to the BOM's Distributor numbers, unrecognised type text stays `unknown`, a file without Distributor and type
  columns is refused). No record = `unknown`.
- **Warnings** (`assembly::warnings`, structured, in the BOM preview and in the prep manifest): missing Distributor
  number, missing manufacturer, ambiguous MPN (an MPN matching several stored Distributor listings, or one listing
  with no number chosen), missing footprint, unverified assembly compatibility, not assemblable, missing
  supplier data, stale supplier data and stale assembly data (> 30 days). They are warnings that the prep
  statement counts; the statement still says "PREPARED FOR REVIEW … NOT manufacturing-ready", and the existing
  DFM-failure block is unchanged. Nothing uploads, quotes or orders.
- New IPC: `assembly_attest`, `assembly_import_parts_file`; `ManufacturingPreview` and `PrepManifest` gain
  an `assembly` report (serde default, so older manifests load).

## Not done (honest limits)
- No live Fabricator catalogue lookup: Fabricator publishes no API this app is authorised to use, so classification
  is attested or file-imported only. Stock/price in an assembly record are as written in the imported file.
- Warnings do not (yet) block the prep bundle by themselves; they are listed and counted. Whether a
  particular warning kind should hard-block is a product decision.
- The parts-file column names are matched by name from public descriptions of Fabricator parts exports; a
  differently-named export is refused with the expected names rather than guessed.
