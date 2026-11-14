# EAK competitive architecture: what EasyEDA shows and where EAK differs

Comparison of EAK's architecture with the EasyEDA Pro ecosystem as it appears in the public
repositories ([ecosystem audit](EASYEDA_ECOSYSTEM_AUDIT.md)). EasyEDA Pro is the interaction benchmark
named by the owner's directive; this document separates what EAK should match (the user experience),
what it should do differently (the engineering core), and what it should not do at all. EasyEDA's
closed-source editor internals are not known; statements about EasyEDA below are limited to what the
public repositories show.

## Positioning

| Dimension | EasyEDA Pro (from the public repositories) | EAK (on `main` at `afd9351`, 2026-11-14) | Position EAK should take |
|---|---|---|---|
| Engine | Closed-source editor; extensions reach it only through the `eda.*` API | Open Rust engine (`eak-core`, `eak-runtime`, `eak-engineering`, `eak-validation`, `eak-manufacturing`) | Keep; this is EAK's main differentiator |
| Mutation boundary | Per-primitive create/modify/delete calls; schematic and PCB commit differently; no undo/redo API | One typed `Command` enum through `Engine::execute`; revision check, approval, transaction, journal, diff undo | Keep and extend (`Batch`, new primitives) |
| AI integration | Local bridge executing arbitrary JavaScript in the editor; MCP server mode in the client; AI extensions trust model output | Typed tool per command, approval classes, sandbox preview, independent verification | Keep; add the desktop assistant and activity panels |
| Data model | Append-only record log per document with logical clocks; floating-point mil; up to 32 copper layers | Canonical JSON EAK-IR, integer nanometres, two copper layers (2-16 since PR #22, merged after this snapshot) | Keep the representation; complete the ordered stack-up |
| Verification | Opaque `PCB_Drc.check` / `SCH_Drc.check`; DFM checks as extensions; no published rule catalogue | ERC/DRC/MECH with stable ids, revision + input hash, measured vs required, `not_implemented` | Keep; widen rule coverage using EasyEDA's taxonomy as a checklist |
| Manufacturing | Many export formats (`PCB_ManufactureData`); viewers marked "reference only" | Gerber, Excellon, BOM, CPL, manifest, release gate that never says "ready" | Keep the gate; add formats over time |
| Desktop UX | Professional workspace: document tabs, panels, layer manager, object trees, cross-probing, net highlighting (seen through the viewers and the API surface) | One workspace screen, SVG editors, findings panel, command palette | **Gap to close: this is what Phases 1-4 are for** |
| Extensibility | Large extension ecosystem with manifest, menus, iframe UI | None | Future capability; do not start before the core workspace is solid |

## What EAK should match (interaction quality)

Drawn from the viewers and the API surface; each is re-implemented natively.

1. **Document-centric workspace.** EasyEDA treats schematic pages, PCB, symbol, footprint, panel and
   previews as editor documents with tabs (`EDMT_EditorDocumentType`). EAK adopts the same user model,
   with documents as typed views onto the single EAK-IR (target architecture, section 1).
2. **Layer handling.** An ordered layer list with per-layer colour, visibility, lock and type, copper
   counts up to 32, and active-layer-exclusive picking where through-hole objects stay pickable on every
   copper layer (`easyeda-viewer`). EAK adopts this in the multi-layer upgrade.
3. **Navigation.** Cursor-anchored zoom, drag pan with a small click-vs-drag threshold, fit, and view
   memory per document (`easyeda-viewer/src/ui/camera.ts`). EAK's `Canvas.tsx` already zooms about the
   cursor; per-document view memory is new.
4. **Selection and highlighting.** Two-way linking between object tree, properties panel and canvas;
   dim-everything-else focus (`online-gencad-viewer`); a separate highlight layer so selection does not
   repaint the board (`eext-interactive-html-bom`); BOM-row and net cross-probing.
5. **Rules as objects.** Net classes, differential pairs, equal-length groups, pad-pair groups and
   region/selector rules (`PCB_Drc` API, `easyeda-file-format/en/pcb/rule.md`). EAK's `Constraints`
   gains these as typed data (plan Phases 3-4).
6. **Long operations with progress and cancel.** Routers and tuners in the ecosystem run in workers with
   stage/percent progress, cancel and best-so-far results (`eext-open-auto-router`,
   `eext-freerouting-intergration`, `eext-quick-equal-length`). EAK's job system provides the same, with
   stale-revision protection.

## Where EAK must be different (engineering core)

1. **One commit path.** EasyEDA's API mixes `setState_*().done()` (PCB) and `modify()` (schematic), and
   returns different interface variants from `get()` and `getAll()`. EAK keeps one typed command path
   with uniform semantics and undo.
2. **Exact geometry.** EasyEDA works in floating-point mil (and changed units between format versions);
   EAK keeps integer nanometres for exact clearance checks and deterministic routing.
3. **No arbitrary code execution for AI.** EasyEDA's AI bridge runs model-generated JavaScript in the
   editor. EAK's agent only proposes typed commands, previewed on a sandbox and approved; verification is
   independent of the model.
4. **AI output is a candidate, never a fact.** EasyEDA's library builders write model-extracted pins and
   pad dimensions straight into the design with no provenance. EAK records evidence per field and marks
   AI output unverified until deterministic checks and a human approve it.
5. **Honest verification.** EasyEDA's PCB DRC returns a boolean or an untyped list. EAK reports are
   revision-bound, list unimplemented checks and never call a board manufacturing-ready.
6. **No dependence on external engines.** Routing integrations in the ecosystem lean on Freerouting,
   KiCad tooling or a paid cloud router. EAK's router is native; external routers are at most optional
   plug-ins over DSN/SES.

## What EAK should not do

- Copy EasyEDA's API shape, numbering schemes (`EPCB_LayerId`) or file format as its internal model.
- Reuse any GPL/AGPL/unlicensed code, or GPL-derived constants (see the
  [licence audit](LICENCE_AND_ATTRIBUTION_AUDIT.md)).
- Imitate EasyEDA branding, icons or exact visual design (directive Part 4.2).
- Build an extension marketplace before the workspace, editors and verification are complete.

## Competitive gaps, ranked by user impact

| Rank | Gap (EAK vs the EasyEDA benchmark) | Plan phase |
|---|---|---|
| 1 | Workspace shell: menus, tabs, dockable panels, inspector, explorers | Phase 1 |
| 2 | Direct manipulation: drag-move, multi-select, rubber-band wires, copy/paste | Phase 2 |
| 3 | Multi-layer boards with a layer manager and stack-up | Phase 3 |
| 4 | Interactive routing with airwires and live clearance feedback | Phase 4 |
| 5 | Clickable findings that navigate, zoom and highlight | Phase 4 |
| 6 | Component library browser, symbol and footprint editors | Phase 5 |
| 7 | Simulation workspace | Phase 7 |
| 8 | Breadth of manufacturing exports (IPC-2581, ODB++, IPC-D-356, DXF) | after Phase 7 |

## Where EAK is already ahead

- A typed, journalled, undoable command engine shared by the desktop and the AI agent (a CLI client is
  proposed in the open PR #11).
- Revision-bound verification evidence with measured vs required values and explicit not-implemented
  lists.
- A manufacturing release gate tied to the exact verified revision, with tamper detection on the
  package.
- A model-agnostic agent with approval classes, sandbox preview and OS-keyring key storage.
- Deterministic domain classification with maturity ceilings (DESK registry).

These are the properties to protect while the workspace catches up.
