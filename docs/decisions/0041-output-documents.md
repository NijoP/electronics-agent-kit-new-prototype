# ADR-0041 Output documents: configurable BOM, schematic PDF/SVG, IPC-D-356 and JSON netlists

Status: accepted (Phase 1 slice D3). No contract change: EAK-IR, `Command`, `CommandResponse`
and the report types are unchanged; the existing manufacturing `Manifest` is reused as is.

## Context
Gaps #40-#42 of the Phase 1 study: the BOM was a fixed-column CSV inside the manufacturing
package, there was no schematic PDF or SVG, and the only netlist exports were SPICE and the
in-app JSON view. The plan (D3) asks for a BOM dialog with a column picker, sort/group and filter
rules (CSV, and XLSX if a permissive writer is approved), schematic PDF/SVG through the SVG export
backend (AD-4), IPC-D-356 and a generic JSON netlist, every output listed in a manifest and DRAFT
unless the release gate passes.

## Options considered
1. **Third-party writers** (an XLSX crate, an SVG-to-PDF crate). Rejected: each adds a dependency
   tree to the manufacturing crate for a few hundred lines of output; a PDF converter also pulls in
   font and image stacks. The formats needed here are small and fully specified.
2. **Write documents from the frontend** (the editor's SVG, browser print to PDF). Rejected: the UI
   would write project files (AGENTS.md), the output would depend on the webview, and the agent
   harness could not produce the same files.
3. **Chosen: one engine-side document backend in `eak-manufacturing`.**
   * `bom_config`: the configuration (columns with optional header and sort, grouping per part or per
     designator, "remove rows where" rules) is serde data with a version; JSON round-trips; the same
     `table()` builds the dialog preview (`bom_table` IPC) and the exported file.
   * `xlsx`: a minimal one-sheet workbook (inline strings, numbers) packed with the existing stored
     ZIP writer. Text is never written as a formula.
   * `schematic_doc`: each sheet drawn once as vector shapes (wires, symbol bodies and pin stubs,
     junctions, labels, no-connects, frame and title block), written as SVG per sheet and as one PDF
     with a page per sheet (Helvetica, not embedded). This is the SVG export backend of AD-4; the
     on-screen editor keeps the Canvas2D renderer.
   * `netlist_export`: IPC-D-356 (317/327 fixed-column records in mm, `NNAME` aliases for long net
     names, vias as 317 records, every line within 80 columns) and a JSON netlist.
   * `outputs::export`: writes the requested documents and `manifest.json` into
     `output/rev-<N>/bom` or `output/rev-<N>/documents`. The status comes from `release_gates`, the
     gate `compile()` uses (now shared): the desktop runs ERC, DRC, MECH and DFM on that revision and
     saves them as evidence first. A missing board outline is a blocking reason here. Documents that
     can carry a label say DRAFT themselves (PDF/SVG title block, IPC-D-356 comment, JSON `status`).
     Every file is self-validated before anything is written, and `verify_package` re-checks the
     folder later.

## Consequences
* New IPC commands `bom_table`, `export_bom`, `export_documents`; File menu entries "Export BOM…" and
  "Export schematic and netlist…".
* The BOM configuration is remembered per computer (local storage) and saved next to every BOM
  export, so an export can be reproduced.
* Not done here: netlist/schematic compare and the optional EasyEDA Pro importer (plan: later);
  solder-mask coverage is not in EAK-IR, so IPC-D-356 records carry `S0`; the PDF has no embedded
  fonts and replaces characters outside printable ASCII with `?`. No agent-harness tool exposes
  these exports yet.
* Outputs stay DRAFT unless the release gate passes; the release-candidate label still says
  engineering and fabricator review are required.

## Source requirement
Phase 1 plan, slice D3 (gaps #40-#42); execution instructions: revision-bound outputs and release
gate; AGENTS.md: unknown checks never pass, the UI does not write project files.

Date: 2026-11-23
