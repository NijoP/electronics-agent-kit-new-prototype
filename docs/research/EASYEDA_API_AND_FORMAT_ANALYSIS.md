# EasyEDA API and file-format analysis

How EasyEDA Pro structures documents, primitives, layers, events, validation and files, as far as the
public repositories show it, and what that means for EAK. Sources are the group A repositories of the
[ecosystem audit](EASYEDA_ECOSYSTEM_AUDIT.md) (sections 1-8), plus groups B and E where noted. The
editor itself is closed source: the API facts come from the generated reference in
`easyeda-api-skill/references/` (the typings package `@fabeda/pro-api-types` is not in any inspected
repository) and the format facts from `easyeda-format-skill`, `easyeda-file-format` and the working
reader/writer in `easyeda-eprj3-skill`. Nothing here was verified by running EasyEDA.

## 1. Document hierarchy and lifecycle

| Concept | EasyEDA Pro | Evidence | EAK today | EAK target |
|---|---|---|---|---|
| Containers | Workspace > Team > Folder > Project > Board > {Schematic > Schematic pages, PCB}; Panel beside boards | `easyeda-api-skill/references` (`DMT_*`, `IDMT_BoardItem`, `EDMT_ItemType`) | One `Project` with sheets and one PCB | Keep one project = one board; Workspace/Team/Folder are cloud concepts and not applicable |
| Board | Links exactly one schematic and one PCB | `IDMT_BoardItem.schematic/.pcb` | Implicit (the project) | Same; no separate board entity needed until multi-board projects |
| Editor document kinds | `EDMT_EditorDocumentType`: home, schematic page, symbol, PCB, footprint, panel, 2D/3D preview, simulation pages | `references/enums/EDMT_EditorDocumentType.md` | One workspace view | `DocumentRef` kinds in the [target architecture](../architecture/EDA_DESKTOP_TARGET_ARCHITECTURE.md), section 1 |
| Library documents | Symbol, Footprint, Device (links symbol + footprint + 3D model + attributes), reusable blocks (CBB), simulation models | `LIB_*` classes; `easyeda-format-skill/documents/{SYMBOL,FOOTPRINT,DEVICE}.md` | `ComponentDef` = symbol + footprint + pin map + ratings + provenance, copied into the project | Same model; add multi-part symbols and per-field evidence |
| Lifecycle | `createProject`, `openProject`, ready state, `openDocument`/`activateDocument`/`closeDocument`, `PCB_Document.save` | `DMT_Project`, `DMT_EditorControl` | Create/open/close project; every command durable | Document open/close is UI state; Save = checkpoint (target architecture) |
| Library scopes | System, personal, project, favourite libraries | `LIB_LibrariesList` | Built-in + project copy | Built-in, user, project (plan Phase 5) |

## 2. Primitives

**PCB** (`EPCB_PrimitiveType`): Arc, Attribute, Component, ComponentPad, Dimension, Fill, Image, Line,
Object, Pad, Polyline, Pour, Poured (the computed fill), Region, String, Via. The file format adds
TEARDROP, LAYER_FILL, flex (FPC) and 3D-shell records (`easyeda-format-skill/documents/PCB.md`).

**Schematic** (`ESCH_PrimitiveType`): Arc, Attribute, Bezier, Bus, Circle, Component, ComponentPin,
Ellipse, Object, Pin, Polygon, Rectangle, Text, Wire. Net flags (power/ground), net ports and
short-circuit flags are **symbol components**, not distinct primitive types
(`sch_PrimitiveComponent.createNetFlag/createNetPort`).

| EasyEDA primitive | EAK-IR today | Gap / decision |
|---|---|---|
| Wire (polyline with a segment list) | `Wire { a, b }` single segment | Keep segments; a polyline is several commands in one `Batch` |
| Bus, bus entry | none | Add (plan Phase 2) |
| Net flag / net port (component-based) | `NetLabel { kind: Net \| Power }` | Add port/global kinds as label kinds rather than pseudo-components; EAK keeps nets derived |
| Junction | none (coincident points join) | Add an explicit junction (plan Phase 2) |
| Text, rectangle, polygon, arc, circle, Bezier | none | Add text and simple graphics (plan Phase 2); Bezier is not needed |
| PCB Line (track) with `netName`, `layerId`, `width` | `Track { layer, width, a, b, net }` | Same shape; the layer becomes a `LayerId` |
| Via (span by design-rule name) | `Via` (implicitly through) | Explicit `span` on the via (plan 3.1) |
| Pour / Poured | `Zone`, with the fill derived on read | Same split (definition vs computed fill); add priority, thermal relief, island removal |
| Region (keepout, rule area) | none | Add keepouts and rule regions (plan 3.2) |
| Dimension, String | none | Add (plan 3.2) |
| Teardrop | none | Future capability |

**Nets** are plain strings on primitives in both products. EasyEDA keeps net records only for special
attributes (colour, type, differential pair, length group), and pad-to-net assignment lives on separate
`PAD_NET` records in the file (`easyeda-eprj3-skill/docs/format-reference.md`). EAK stores the same
information as `FootprintInstance.pad_nets`, synchronised from the schematic, and DRC-010 flags drift;
no change is needed.

## 3. Layers and stack-up

- **API:** `EPCB_LayerId` numbers top 1, bottom 2, silkscreen 3/4, solder mask 5/6, paste 7/8,
  assembly 9/10, board outline 11, multi-layer 12, document 13, mechanical 14, **inner 1-30 = 15-44**,
  component shape/marking/pin layers 48-52, 3D shell 53-55, drill drawing 56, ratline 57, stiffeners
  58/59, **custom 1-30 = 71-100**, substrate 101. `TPCB_NumberOfCopperLayers` is the type
  `2 | 4 | 6 | ... | 32` (checked in `references/types/TPCB_NumberOfCopperLayers.md`). Layer types:
  signal, plane (internal electrical), silkscreen, solder mask, paste mask, assembly, custom, other.
  `IPCB_LayerItem` carries id, name, type, colour, transparency, inactive colour, status and lock.
- **File:** ordered `LAYER` records (stack order = order of appearance) and `LAYER_PHYS` for physical
  stack-up properties; via start/end layers are defined indirectly through a named "via layer type"
  design rule (`easyeda-api-skill/format/pcb/pad_via.md`).
- **EasyEDA Std differs:** Std numbers paste and mask layers the other way round
  (`easyeda-documents/Open-File-Format/PCB.md`), so layer ids are not portable between the two products.

**Implication for EAK (plan 3.1):** adopt the *structure* (an ordered list of typed layers with physical
properties, non-copper roles per side, user/custom layers, copper counts up to 32) but not the
numbering. EAK uses stable string ids (`cu.top`, `cu.in1`, ...), an explicit `ViaSpan` on each via, and
rejects unrepresentable inputs rather than guessing. Single-sided (one copper layer) boards are valid
in EAK even though the EasyEDA type starts at 2.

## 4. Units and geometry

| | EasyEDA Pro | EasyEDA Std | EAK |
|---|---|---|---|
| PCB unit | mil, floating point (V4); V3 used 0.01 inch | 10 mil | integer nanometres |
| Schematic unit | 0.01 inch | 10 mil | integer nanometres |
| Rotation | degrees, counter-clockwise | degrees | quarter turns |
| Y axis | file option `yAxisDirection` in V4 local files | down (SVG coordinates) | not fixed by an ADR (ADR-0005 covers units and rotation only) |

EAK's integer-nanometre geometry is the sounder base for exact DRC and deterministic results
(ADR-0005). Any import/export adapter would need explicit unit and axis conversion and a per-format
layer map. Free-angle rotation is a future EAK-IR question (EasyEDA allows any angle); quarter turns
remain sufficient for Phases 1-4.

## 5. Selection, events and properties

- Selection: `PCB_SelectControl` / `SCH_SelectControl` (get selected primitives, select, clear,
  `doCrossProbeSelect(components, pins, nets, highlight, select)`, mouse position). The current
  document is only available through `DMT_SelectControl.getCurrentDocumentInfo()`.
- Events: mouse events, primitive add/delete/modify events, net events, cross-probe events, real-time
  DRC results and editor-tab events, each registered with a caller-chosen listener id.
- Properties: getters per field; PCB primitives commit with `setState_*(...).done()`, schematic
  primitives through a class-level `modify(id, {...})`; `get()` and `getAll()` can return different
  interface variants (`extension-dev-skill/resources/experience.md`).

**EAK:** selection stays a set of stable ids in the frontend; change notification comes from engine
`Event`s and `CommandResponse` deltas (one uniform commit path), so EAK does not need per-primitive
listeners. Cross-probing by component, pin and net is reproduced through backend queries
(`related_objects`, `net_members` in the target architecture). The inconsistent commit semantics are a
cautionary example: every EAK property edit is a typed command.

## 6. Undo/redo, persistence and collaboration

- **No undo/redo API** appears in the EasyEDA reference (searched in `easyeda-api-skill/references`).
- **File format** (`easyeda-format-skill/SKILL.md`, `easyeda-file-format/README.md`): each document is
  an append-only log of records `{"type","id","ticket"}||{payload}|`; later lines override earlier ones;
  conflicts resolve by the larger `ticket` (a logical clock), then by client id; deletion writes an empty
  payload (tombstone); `DELETE_DOC` deletes a document reversibly. This is a last-writer-wins register
  per element, designed for collaboration.
- **Containers:** `.eprj2` is one SQLite file; `.eprj3` is a folder of JSON log files ("Git friendly");
  `.epro2` is a zip archive. In `.eprj3`, each schematic/PCB container embeds the symbol, footprint and
  device documents its parts need (`easyeda-eprj3-skill/docs/format-reference.md`).

**EAK:** the fsync'd journal plus byte-stable canonical JSON and diff-based undo/redo (ADR-0001,
ADR-0003) already give durable history, crash recovery and Git-friendly diffs for a single user, and
EAK's self-contained project library matches EasyEDA's embedded library snapshot. A per-element logical
clock would only matter for multi-user collaboration, which is a future capability.

## 7. Validation interfaces

- `PCB_Drc.check(strict, userInterface, includeVerboseError)` returns a boolean or an untyped array;
  real-time DRC can be started and stopped; rule configurations, net classes, differential pairs,
  equal-length groups and pad-pair groups are separate API objects. `SCH_Drc.check` returns
  `{ type: fatalError|error|warn, rule, net?, primitives }`. No rule catalogue is published.
- The rule model in the file format is template -> rule -> selector (net, net class, region)
  (`easyeda-file-format/en/pcb/rule.md`).
- The only published, coded rule tables are in `easyeda-eprj3-skill` (ERC E001-E008 and W001-W003,
  layout L001-L010, copper DRC D001-D012, each with a fix string; rules that do not apply to a
  placed-only board are skipped and reported) and `eext-fab-order-dfm-checker` (18 PCB and 7 SMT
  checks against Fabricator capability tables).

**EAK:** EAK's reports are stronger contracts (revision and input hash, `not_implemented`,
`limitations`, measured vs required, stable finding ids). Use the EasyEDA rule taxonomy as a checklist:
region- and selector-scoped rules, differential pairs, length groups and pad-pair groups are missing
from EAK `Constraints` and enter with plan Phases 3-4.

## 8. Integration surfaces

| Surface | EasyEDA | EAK equivalent |
|---|---|---|
| Extension API | `eda.<prefix>_<Class>` namespaces (`dmt_`, `pcb_`, `sch_`, `lib_`, `sys_`), all promise-based, sandboxed iframe UI | Typed `Command` + read-only queries; no plugin system yet (future) |
| AI integration | Local bridge that executes arbitrary JavaScript inside the editor (`easyeda-api-skill/scripts/bridge-server.mjs`; `easyeda-client-cli` `invoke`); MCP server mode | One typed tool per `Command`, approval-gated (`eak-intelligence`); arbitrary code execution is excluded by EAK's non-negotiables |
| Self-description | CLI `doc` command; compiler-derived API graph (`extension-dev-skill/scripts/eda-api.js`) | Generate the agent tool catalogue and TypeScript types from the Rust `Command` types |
| Manufacturing data | Gerber, BOM, pick-and-place, DSN, IPC-2581C, IPC-D-356A, ODB++, DXF, PADS, Altium, flying-probe and test-point files (`PCB_ManufactureData`) | Gerber, Excellon, BOM, CPL today; GenCAD in plan Phase 7; the rest are future capabilities |

## 9. Import/export of EasyEDA files

Not required by the directive. If it is ever wanted, it is a native Rust adapter written from
`easyeda-format-skill` (MIT) and its JSON Schemas, with the reader caveats recorded by
`easyeda-eprj3-skill` (split each line on the first `||` only; mixed client ids are normal; footprint
coordinates are mil even when the canvas says mm). The adapter would translate into EAK commands and
would never make EasyEDA files a source of truth.
