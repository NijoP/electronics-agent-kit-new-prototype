# EasyEDA ecosystem audit

Structured investigation of the public [EasyEDA GitHub organisation](https://github.com/easyeda),
carried out for Phase 0 of the 2026-11-14 master directive ("Transform EAK into a professional, fully
interactive EDA desktop application", Part 2). Companion documents:
[feature matrix](EASYEDA_FEATURE_MATRIX.md),
[API and format analysis](EASYEDA_API_AND_FORMAT_ANALYSIS.md),
[EAK competitive architecture](EAK_COMPETITIVE_ARCHITECTURE.md),
[licence and attribution audit](LICENCE_AND_ATTRIBUTION_AUDIT.md).

## Scope and method

- **Inspection date:** 2026-11-14. Commit hashes per repository are listed below and in each section.
- **Organisation size:** `gh api orgs/easyeda/repos` returned **104 public repositories**. The
  directive names 37 of them (groups A-E); **all 37 exist and are public**. Those 37 were shallow-cloned
  (`git clone --depth 1`) outside the EAK repository and inspected read-only. `easyeda-documents`
  (archived, about 340 MB, mostly images) was cloned without checkout and inspected through
  `git ls-tree` and `git show` on a handful of small files.
- **No code was built, run or tested**, and nothing from these repositories was copied into EAK.
  Findings cite the source paths that were read; where only a README, a minified bundle or nothing at
  all was available, the section says so. Statements inferred from file or class names rather than
  read logic are marked as inferred.
- The investigation was split into three read-only passes (group A; groups B and C; groups D and E)
  with one brief and one output template, then reviewed. Key claims were re-checked against the clones
  before publication: the `EPCB_LayerId` inner/custom ids and `TPCB_NumberOfCopperLayers`, the
  easyeda-eprj3-skill DRC code list, the ngspice bundle's GPL-licensed wrapper sources and GPLv2+
  `xspice/icm/table` model, the OpenAutoRouter GPL notice, and the README-only state of
  `eext-simulation-with-ltspice`.
- The remaining 67 organisation repositories were **not inspected**; they are listed with their
  GitHub metadata only in [Appendix: other repositories](#appendix-other-repositories-in-the-organisation-not-inspected).
- Classification vocabulary (directive Part 2): *applicable directly*, *requires adaptation*,
  *requires native EAK implementation*, *future capability*, *not applicable*. Every classification
  assumes EAK's policy: concepts are re-implemented natively in Rust/TypeScript; no EasyEDA code,
  assets or runtime are used.

## Headline conclusions

1. **Nothing in the organisation is an editable EDA kernel EAK could reuse.** The editor (EasyEDA Pro)
   is closed source; the public repositories are extensions (`eext-*`) that only work inside it through
   the `eda.*` API, documentation and LLM "skills", viewers, and two GPL/AGPL engines. EAK's own Rust
   engine stays the source of truth.
2. **The most valuable material is design knowledge:** the Pro file format and document hierarchy
   (`easyeda-format-skill`, `easyeda-file-format`, `easyeda-api-skill`), a 2-32 copper-layer model with
   an ordered stack-up and via spans, rule taxonomies (net class, differential pair, equal-length group,
   region rules), coded ERC/DRC/DFM rule tables (`easyeda-eprj3-skill`, `eext-jlc-order-dfm-checker`),
   and interaction patterns from the viewers (cursor-anchored zoom, layer-exclusive picking,
   dim-others highlighting, BOM-to-board cross-probing).
3. **EasyEDA's extension API is a weaker boundary than EAK's command engine:** per-primitive
   create/modify/delete with inconsistent commit semantics, no undo/redo API, an opaque DRC entry point,
   and an "AI interface" that executes arbitrary JavaScript in the editor (`easyeda-client-cli`,
   `easyeda-api-skill`). EAK should keep its typed `Command` boundary.
4. **The AI component tools trust model output.** Symbol and footprint builders turn vision-model JSON
   straight into pins and pads with only shape checks and no provenance. The best pattern seen is the
   device-standardisation matcher, where the model chooses among real library search results. EAK must
   treat AI output as an unverified candidate and verify it deterministically.
5. **Routing:** the only real routers are GPL (an OpenAutoRouter port; Freerouting). External routers
   are reached over Specctra DSN/SES. EAK should design its own negotiated-congestion and interactive
   routing from public literature, and at most offer an optional, out-of-process DSN/SES adapter.
6. **Simulation:** the ngspice extension runs ngspice 46 as WebAssembly, but the netlist and model
   association come from the closed host, and the bundle mixes BSD, LGPL, MPL, GPLv2+ and GPL-3.0
   components. The AGPL `easyeda-simulation-engine` drives ngspice through its shared-library API. EAK
   must write its own EAK-IR-to-SPICE netlister and run a user-installed simulator out of process.
7. **Licences:** of the 37, 25 ship an Apache-2.0 licence file, 2 MIT, 1 BSD-3-Clause (whose bundle
   contains LGPL/MPL/GPL parts), 2 GPL-3.0, 1 AGPL-3.0, and 6 have no licence file; details in the
   licence audit.

## Inventory of the 37 directive repositories

| # | Group | Repository | Commit inspected | Licence file | Nature |
|---|---|---|---|---|---|
| 1 | A | [easyeda-api-skill](https://github.com/easyeda/easyeda-api-skill) | `ed922cd6a4ba` | none | LLM skill: generated `eda.*` API reference, format docs, one Node bridge script |
| 2 | A | [easyeda-api-sdk](https://github.com/easyeda/easyeda-api-sdk) | `f83525eb54f8` | Apache-2.0 | Extension project template and build tooling (typings live in npm `@jlceda/pro-api-types`, not here) |
| 3 | A | [easyeda-format-skill](https://github.com/easyeda/easyeda-format-skill) | `4fcab1747d47` | MIT | Pro format spec ("V4") as an LLM skill: 297 JSON Schemas, AJV validator |
| 4 | A | [easyeda-eprj3-skill](https://github.com/easyeda/easyeda-eprj3-skill) | `8ff62c549c81` | MIT | Real code (~24k lines Node): offline `.eprj3` project builder with ERC/DRC/layout gates and tests |
| 5 | A | [easyeda-file-format](https://github.com/easyeda/easyeda-file-format) | `9186c4ac43e6` | none | Docs only: V3 Pro file-format spec |
| 6 | A | [easyeda-documents](https://github.com/easyeda/easyeda-documents) | `c5637b5f1fa0` | none (archived) | Outdated EasyEDA Std tutorials, Std file format, API samples |
| 7 | A | [extension-dev-skill](https://github.com/easyeda/extension-dev-skill) | `d9148cba5241` | none | LLM skill: extension recipes plus a TypeScript-compiler API query CLI |
| 8 | A | [easyeda-client-cli](https://github.com/easyeda/easyeda-client-cli) | `5452c638fca8` | none | Docs only: CLI and bridge built into the desktop client (closed source) |
| 9 | B | [easyeda-viewer](https://github.com/easyeda/easyeda-viewer) | `fd69dd16134a` | Apache-2.0 | Read-only offline viewer for Pro projects (LeaferJS canvas) |
| 10 | B | [online-gencad-viewer](https://github.com/easyeda/online-gencad-viewer) | `cdc76cdaa520` | Apache-2.0 | GenCAD 1.4 viewer (LeaferJS canvas) |
| 11 | B | [eext-gencad-viewer](https://github.com/easyeda/eext-gencad-viewer) | `b537e0cc0dfc` | Apache-2.0 | Extension wrapper around a built copy of #10 |
| 12 | B | [eext-gerber-viewer](https://github.com/easyeda/eext-gerber-viewer) | `085eb879b61e` | Apache-2.0 | Extension with a bundled Gerber/Excellon viewer (tracespace-derived plotter) |
| 13 | B | [eext-interactive-html-bom](https://github.com/easyeda/eext-interactive-html-bom) | `430ea9d06a1c` | Apache-2.0 | Interactive HTML BOM (InteractiveHtmlBom-derived viewer) |
| 14 | C | [eext-ai-library-builder](https://github.com/easyeda/eext-ai-library-builder) | `27731deb50a3` | Apache-2.0 | Datasheet page -> vision model -> parametric symbol/footprint |
| 15 | C | [eext-ai-symbol-builder](https://github.com/easyeda/eext-ai-symbol-builder) | `e9fb3b167356` | Apache-2.0 | Pinout image -> vision model -> symbol |
| 16 | C | [eext-ai-device-standardization](https://github.com/easyeda/eext-ai-device-standardization) | `89abac48075b` | Apache-2.0 | BOM/schematic part matching to the LCSC library, optional LLM choice |
| 17 | C | [eext-datasheet-helper](https://github.com/easyeda/eext-datasheet-helper) | `6be8cf0b8e28` | Apache-2.0 | Datasheet Q&A (online LLM or local in-browser models) |
| 18 | C | [eext-component-box](https://github.com/easyeda/eext-component-box) | `0390a4c0e9db` | Apache-2.0 | Personal parts inventory plus ESP32 storage-box hardware (no AI) |
| 19 | D | [eext-open-auto-router](https://github.com/easyeda/eext-open-auto-router) | `2a308fdd2deb` | GPL-3.0 | Island placement + PathFinder negotiated-congestion router (OpenAutoRouter port) |
| 20 | D | [eext-freerouting-intergration](https://github.com/easyeda/eext-freerouting-intergration) | `a96e2e9c5cee` | Apache-2.0 | Client for a local Freerouting HTTP API (DSN/SES) |
| 21 | D | [easyeda-pcb-router](https://github.com/easyeda/easyeda-pcb-router) | `1f230c15ec82` | GPL-3.0 | Java WebSocket server wrapping Freerouting |
| 22 | D | [eext-kirouting-integration](https://github.com/easyeda/eext-kirouting-integration) | `d682f9f99f01` | none | Bridge to KiCadRoutingTools via a Python FastAPI server |
| 23 | D | [eext-deeppcb-autorouter](https://github.com/easyeda/eext-deeppcb-autorouter) | `f519a70d17b9` | Apache-2.0 | Client for the DeepPCB paid cloud router (DSN/SES) |
| 24 | D | [eext-diff-pair-assistant](https://github.com/easyeda/eext-diff-pair-assistant) | `99fc251f2a4a` | Apache-2.0 | Name-based differential-pair detection and rule management (no routing) |
| 25 | D | [eext-quick-equal-length](https://github.com/easyeda/eext-quick-equal-length) | `8c2b3de7a6b6` | Apache-2.0 | Meander length tuning with neighbour pushing |
| 26 | D | [eext-pad-fanout](https://github.com/easyeda/eext-pad-fanout) | `7c88e571457e` | Apache-2.0 | Octilinear pad fanout planner with unit tests |
| 27 | D | [eext-pcb-component-grouping](https://github.com/easyeda/eext-pcb-component-grouping) | `95421773a018` | Apache-2.0 | PCB placement grouped by schematic regions |
| 28 | D | [eext-balance-copper](https://github.com/easyeda/eext-balance-copper) | `4a52f9021a9d` | Apache-2.0 | Copper-thieving pattern fill |
| 29 | E | [eext-jlc-order-dfm-checker](https://github.com/easyeda/eext-jlc-order-dfm-checker) | `afd538786d51` | Apache-2.0 | JLCPCB capability DFM checks (18 PCB + 7 SMT) |
| 30 | E | [eext-design-rule-generator](https://github.com/easyeda/eext-design-rule-generator) | `2cf9635b335b` | Apache-2.0 | Net classification, stack-up choice, impedance surrogate model |
| 31 | E | [eext-netlist-explorer](https://github.com/easyeda/eext-netlist-explorer) | `6661961fc878` | Apache-2.0 | Netlist browser and diagrams |
| 32 | E | [eext-bom-compare](https://github.com/easyeda/eext-bom-compare) | `4489dd9b857e` | Apache-2.0 | File-vs-file BOM diff with a multi-vendor test corpus |
| 33 | E | [eext-export-gencad](https://github.com/easyeda/eext-export-gencad) | `aba4dff5b0fb` | Apache-2.0 | GenCAD 1.4 writer |
| 34 | E | [eext-simulation-with-ngspice](https://github.com/easyeda/eext-simulation-with-ngspice) | `1ac297f92b79` | BSD-3-Clause (bundle contains LGPL/MPL/GPL parts) | ngspice 46 WebAssembly workbench, waveforms, Monte Carlo |
| 35 | E | [eext-simulation-with-ltspice](https://github.com/easyeda/eext-simulation-with-ltspice) | `0fb2afa18164` | Apache-2.0 | README only; no source published |
| 36 | E | [easyeda-simulation-engine](https://github.com/easyeda/easyeda-simulation-engine) | `f06a1dd20b60` | AGPL-3.0 | C++ simulation server: ngspice shared library + SimulIDE-derived core |
| 37 | E | [eext-export-design-report](https://github.com/easyeda/eext-export-design-report) | `31a8cfec95bc` | Apache-2.0 | Net-length / pad / board-area CSV exporter (not a full design report) |

## Per-repository findings

Paths in the sections below are relative to the root of the repository being described.

## Group A - Editor API and document architecture (EasyEDA repos 1-8)

Method: read-only inspection of the shallow clones (and `git ls-tree`/`git show` for easyeda-documents) plus `gh api repos/easyeda/<repo>` for licence/archive flags. No builds or installs. Important caveat for repo 2: the `@jlceda/pro-api-types` typings (the real `index.d.ts`, ~659 KB per extension-dev-skill SKILL.md) are NOT contained in easyeda-api-sdk; the API class/enum/interface names cited below come from the generated reference docs in easyeda-api-skill `references/` (same API Extractor-style names: `class X`, `enum E...`, `interface I...`) and are labelled as such.

Repo-type summary (docs/skill prompts vs real code):

| # | Repo | Nature |
|---|---|---|
| 1 | easyeda-api-skill | 472 of 475 files are Markdown (LLM skill + generated API reference + format docs); only real code is one 499-line Node bridge `scripts/bridge-server.mjs` |
| 2 | easyeda-api-sdk | Real but tiny code: extension project template/build tooling (8 .ts files, ~870 lines of build scripts + 25-line `src/index.ts`); no typings inside |
| 3 | easyeda-format-skill | Docs/skill prompts + 297 JSON Schemas + one 1402-line `validate.js` (AJV validator) |
| 4 | easyeda-eprj3-skill | Real code: ~23.9k lines of zero-dependency Node JS (build, ERC, DRC, layout lint, netlist, CLI bridge) plus tests, wrapped as a skill |
| 5 | easyeda-file-format | Docs only (67 .md + 5 .png); content duplicates easyeda-api-skill `format/` |
| 6 | easyeda-documents | Archived EasyEDA Std tutorial docs; mostly images; a few small .js API samples |
| 7 | extension-dev-skill | Skill prompts/recipes (64 .md) + one real TypeScript-compiler-based CLI `scripts/eda-api.js` (928 lines) with a small test |
| 8 | easyeda-client-cli | Docs only (4 .md); the CLI itself is closed-source inside the desktop client |

---

### 1. easyeda-api-skill
- URL: https://github.com/easyeda/easyeda-api-skill
- Commit inspected: ed922cd6a4baf217eb554edef8b6c017f6b22114
- Licence: NO LICENSE file in the repo (GitHub reports no SPDX licence). Only `SKILL.md` front matter says `license: MIT`; `package.json` has no `license` field. ; reusable in EAK: NO for anything copied (no licence file; treat as unlicensed). Concepts/facts only.
- Language / size: 475 files, 472 Markdown, `scripts/bridge-server.mjs` (Node, dependency `ws ^8.21.3`), package version 1.1.69. Reference docs say 120 classes, 62 enums, 70 interfaces (`SKILL.md`; `references/classes` has 176 files incl. IPCB_/ISCH_ interface classes).
- What it is: An Agent-Skills package for AI tools to drive EasyEDA Pro. An AI POSTs JavaScript to a local bridge (`POST /execute`, `GET /health`), the bridge relays it over WebSocket (ports 49620-49629) to the `run-api-gateway.eext` extension inside the desktop client, where it runs as `async function(eda)`. It also ships a generated API reference and the V3 document-source format docs under `format/`.
- Key source paths:
  - `SKILL.md` - workflow, runtime constraints, module-prefix table
  - `.../easyeda-api-skill/scripts/bridge-server.mjs` - HTTP+WS bridge (`/health`, `/execute`, `/eda-windows`)
  - `.../easyeda-api-skill/references/_index.md`, `_quick-reference.md` (3103 lines of method signatures)
  - `.../references/enums/EPCB_LayerId.md`, `EDMT_EditorDocumentType.md`, `EDMT_ItemType.md`, `EDMT_ProjectFileFormat.md`, `EPCB_PrimitiveType.md`, `ESCH_PrimitiveType.md`, `EPCB_LayerType.md`
  - `.../references/types/TPCB_NumberOfCopperLayers.md`
  - `.../references/classes/PCB_Layer.md`, `PCB_Drc.md`, `SCH_Drc.md`, `PCB_Event.md`, `PCB_SelectControl.md`, `DMT_*.md`, `LIB_*.md`
  - `.../easyeda-api-skill/format/index.md`, `format/project/meta.md`, `format/pcb/{common,primitive,shape,pad_via,rule}.md`, `format/schematic/{wire,component,structure}.md`
- Implementation observations:
  - Document hierarchy (API view): `DMT_Workspace` > `DMT_Team` > `DMT_Folder` > Project (`DMT_Project`, `IDMT_ProjectItem.data: Array<IDMT_BoardItem | IDMT_SchematicItem | IDMT_PcbItem | IDMT_PanelItem>`) > Board (`DMT_Board`, `IDMT_BoardItem` has `.schematic` and `.pcb`) > Schematic (`DMT_Schematic`) > SchematicPage (`createSchematicPage`, `IDMT_SchematicPageItem`); PCB via `DMT_Pcb`; panels via `DMT_Panel`. `EDMT_ItemType` values: Workspace, Team, Folder, Project, Board, Schematic, 'Schematic Page', PCB, Panel, plus CBB Project/Schematic/PCB (reusable blocks).
  - Editor document kinds (`EDMT_EditorDocumentType`): HOME -1, BLANK 0, SCHEMATIC_PAGE 1, SYMBOL_COMPONENT 2, PCB 3, FOOTPRINT 4, PROJECT 5, PANEL 26, SYMBOL_CBB 17, SYMBOL_NET_FLAG 18, SYMBOL_NET_PORT 19, SYMBOL_DRAWING 20, simulation page/symbol kinds (Ngspice/SimulIDE), PCB_2D_PREVIEW 12, PCB_3D_PREVIEW 15. Symbols/footprints are edited as their own documents.
  - Lifecycle: `DMT_Project.createProject(projectFriendlyName, projectName?, teamUuid?, folderUuid?, description?, collaborationMode?, fileFormat?)`, `openProject`, `getReadyState` (`EDMT_ProjectReadyState`), `waitForReady`; `EDMT_ProjectFileFormat` = `eprj2` (single-file SQLite DB) or `eprj3` (folder + multi-file JSON, "Git friendly"); the file-format param only applies in offline client modes. `DMT_EditorControl`: openDocument/activateDocument/closeDocument/split screen/zoom.
  - Namespace pattern: everything hangs off global `eda` with `<prefix>_<Class>` mounts: `dmt_*` (document tree), `pcb_*`, `sch_*`, `lib_*`, `sys_*`; `IPCB_*`/`ISCH_*` are primitive object types, `E<AREA>_*` enums. Per-primitive classes such as `pcb_PrimitiveLine`, `pcb_PrimitiveVia`, `pcb_PrimitivePad`, `pcb_PrimitivePour`, `pcb_PrimitiveRegion`, `pcb_PrimitiveDimension`, `pcb_PrimitiveComponent`; `sch_PrimitiveWire`, `sch_PrimitiveBus`, `sch_PrimitiveComponent`, `sch_PrimitivePin`, `sch_PrimitiveText`, etc. All methods are `Promise`-returning; each primitive class has create/delete/get/getAll/getAllPrimitiveId/modify (e.g. `pcb_PrimitiveLine.create(net, layer, startX, startY, endX, endY, lineWidth?, primitiveLock?)`).
  - PCB primitive types (`EPCB_PrimitiveType`): Arc, Attribute, Component, ComponentPad, Dimension, Fill, Image, Line, Object, Pad, Polyline, Pour, Poured, Region, String, Via. Schematic (`ESCH_PrimitiveType`): Arc, Attribute, Bezier, Bus, Circle, Component, ComponentPin, Ellipse, Object, Pin, Polygon, Rectangle, Text, Wire. Note: no separate net-label/no-connect/junction primitive in the enum; net labels/ports/flags are created via `sch_PrimitiveComponent.createNetFlag(identification 'Power'|'Ground'|'AnalogGround'|'ProtectGround', net, x, y, ...)`, `createNetPort('IN'|'OUT'|'BI', ...)`, `createShortCircuitFlag` - i.e. they are symbol components.
  - Layers: `EPCB_LayerId` numeric ids: TOP 1, BOTTOM 2, TOP_SILKSCREEN 3, BOTTOM_SILKSCREEN 4, TOP_SOLDER_MASK 5, BOTTOM_SOLDER_MASK 6, TOP_PASTE_MASK 7, BOTTOM_PASTE_MASK 8, TOP/BOTTOM_ASSEMBLY 9/10, BOARD_OUTLINE 11, MULTI 12, DOCUMENT 13, MECHANICAL 14, INNER_1..INNER_30 = 15..44, COMPONENT_SHAPE 48, COMPONENT_MARKING 49, PIN_SOLDERING 50, PIN_FLOATING 51, COMPONENT_MODEL 52, SHELL_3D_* 53-55, DRILL_DRAWING 56, RATLINE 57, TOP/BOTTOM_STIFFENER 58/59, CUSTOM_1..CUSTOM_30 = 71..100, SUBSTRATE_1 101. Copper count is settable via `PCB_Layer.setTheNumberOfCopperLayers(n)` with `TPCB_NumberOfCopperLayers = 2|4|6|...|32` (even numbers up to 32 = 2 outer + up to 30 inner). `EPCB_LayerType`: SIGNAL, PLANE (`INTERNAL_ELECTRICAL`), SILKSCREEN, SOLDER_MASK, PASTE_MASK, ASSEMBLY, CUSTOM, OTHER. `IPCB_LayerItem` has id, name, type, color, transparency, inactiveColor, inactiveTransparency, layerStatus, locked. `PCB_Layer` also: addCustomLayer/removeLayer, lock/unlock, visibility, physical stackup (`IPCB_PhysicalStackingConfiguration`, `savePhysicalStackingConfiguration`, ...), `setPcbType(EPCB_PcbPlateType)`.
  - Nets: `PCB_Net` (getAllNets -> `IPCB_NetInfo {net, color, length}`, getNetLength, highlight/select, `getNetlist(ESYS_NetlistType)`/`setNetlist`); schematic side `SCH_Netlist.getNetlist/setNetlist`. Nets are plain strings on primitives (`net` param), not first-class objects with ids.
  - Net classes / rules (`PCB_Drc`): createNetClass/addNetToNetClass, differential pairs, equal-length net groups, pad-pair groups, rule configurations (get/save/overwrite/rename/setAsDefault, `getNetRules`, `getRegionRules`, `getNetByNetRules`).
  - Validation APIs: `PCB_Drc.check(strict, userInterface, includeVerboseError)` returns boolean, or `Array<any>` when verbose (note: verbose result is untyped); `startRealTimeDrc/stopRealTimeDrc/getRealTimeDrcStatus`; `SCH_Drc.check(...)` verbose returns `Array<ISCH_DrcError>` with `{type: 'fatalError'|'error'|'warn', rule: string key like 'IN-IN', net?, primitives: ISCH_DrcErrorPrimitive[]}`. No ERC/DRC rule catalogue is documented in the API refs.
  - Selection and events: `DMT_SelectControl.getCurrentDocumentInfo()` (the only current-document query), `PCB_SelectControl`/`SCH_SelectControl` (getAllSelectedPrimitives(_PrimitiveId), doSelectPrimitives, clearSelected, doCrossProbeSelect(components, pins, nets, highlight, select), getCurrentMousePosition); events via `PCB_Event`/`SCH_Event`/`DMT_Event`: addMouseEventListener (`EPCB_MouseEventType`), addPrimitiveEventListener (`EPCB_PrimitiveEventType` = add/delete/modify), addNetEventListener, addCrossProbeSelectEventListener, addRealTimeDrcResultEventListener, DMT `addEditorTabEventListener`. Listener ids are caller-chosen strings (`removeEventListener(id)`).
  - Component/symbol/footprint/device: library classes `LIB_Device` (create/modify/get/search/searchByProperties/getByLcscIds), `LIB_Symbol`, `LIB_Footprint` (each with getDocumentSource/updateDocumentSource, i.e. raw log-format source round-trip), `LIB_3DModel`, `LIB_Cbb`, `LIB_Classification`, `LIB_SimulationModel`; `ELIB_LibraryType`: CBB 1, SYMBOL 2, DEVICE 3, FOOTPRINT 4, MODEL 5, PANEL_LIBRARY 29, simulation models. `LIB_LibrariesList` exposes system/personal/project/favorite library uuids (`getProjectLibraryUuid`) and `registerExtendLibrary`. A schematic part is placed from a Device (`sch_PrimitiveComponent.create(component, x, y, subPartName?, rotation?, mirror?, addIntoBom?, addIntoPcb?)`); the Device links Symbol + Footprint (+3D model).
  - Properties/attributes: primitives expose `getState_*` getters; PCB primitives chain `setState_*` then `.done()`; `PCB_PrimitiveAttribute`/`ISCH_PrimitiveAttribute` are separate attribute primitives (see repo 7 pitfalls for the SCH/PCB asymmetry).
  - Undo/redo: NO undo/redo API appears anywhere in the references (grep for undo/redo only matches `format/index.md`, which is about log-format "revoke" semantics). The API exposes `PCB_Document.save()`, `importChanges` (sync schematic->PCB), `autoLayout`/`autoRouting`/`clearRouting`, import of autoroute `.ses`/JSON files.
  - Units: PCB data unit = mil; schematic data unit = 10 mil (0.01 inch) (`references/classes/SYS_Unit.md`). Floating-point numbers, not integers.
  - Manufacturing data: `PCB_ManufactureData` (getGerberFile, getBomFile, getPickAndPlaceFile, getDsnFile, getIpc2581cFile, getIpcD356aFile, getOpenDatabaseDoublePlusFile, getDxfFile, getPadsFile, getAltiumDesignerFile, getFlyingProbeTestFile, getTestPointFile, ...), `SCH_ManufactureData`, `SYS_FormatConversion` (Altium/DISA library conversion).
  - Stability policy: `guide/stability.md` - API semver separate from editor version; beta/alpha APIs gated to development builds.
  - Extension runtime constraints (SKILL.md): extension main process is sandboxed (no `alert`, `localStorage`); UI via `sys_IFrame`; data via `sys_Storage`.
  - Tests: none in repo.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Hierarchy Workspace>Team>Folder>Project>Board>{Schematic>Pages, PCB} with Board linking exactly one schematic and one PCB | requires adaptation | `references/interfaces/IDMT_BoardItem.md`, `classes/DMT_Board.md` | EAK-IR is single-design; consider an explicit Board-level container tying schematic sheets to a PCB; teams/workspaces are cloud concepts (not applicable) |
| Typed namespace-per-domain API (`dmt_/pcb_/sch_/lib_/sys_`) with create/get/getAll/modify/delete per primitive | requires adaptation | `references/_quick-reference.md` | Map to EAK's 38 typed `Command` variants; EAK's single `Engine::execute` is a stronger boundary (EasyEDA exposes ~mutating methods directly) |
| Layer id enum with 32 copper layers (2 + INNER_1..30), custom layers 71-100, non-copper roles | requires adaptation | `references/enums/EPCB_LayerId.md`, `types/TPCB_NumberOfCopperLayers.md` | Use as a checklist when upgrading EAK from Top/Bottom to a stackup-ordered layer list; prefer ordered stackup + roles over magic numbers |
| Physical stackup configs (`IPCB_PhysicalStackingConfiguration`) | future capability | `classes/PCB_Layer.md` | Needed for impedance/controlled-impedance later |
| DRC/ERC entry points (`PCB_Drc.check`, `SCH_Drc.check`, real-time DRC, rule configurations, net classes, diff pairs, equal-length groups, pad-pair groups) | requires native EAK implementation | `classes/PCB_Drc.md`, `SCH_Drc.md` | EasyEDA only calls an opaque in-client engine; EAK already owns eak-validation. Use the rule-object taxonomy (net class, diff pair, length group, region rule, net-by-net rule) as a feature checklist |
| Selection/event model (primitive add/delete/modify events, cross-probe select between schematic and PCB) | requires adaptation | `classes/PCB_Event.md`, `PCB_SelectControl.md` | Derive UI events from EAK Command deltas; implement cross-probe via component/pin/net ids |
| Bridge pattern: external AI sends code to running editor over local WS | not applicable | `scripts/bridge-server.mjs` | EAK's agent uses typed tools per Command; arbitrary-JS eval is exactly what EAK's non-negotiables exclude |
| Raw document-source round-trip (`LIB_Symbol.getDocumentSource`/`updateDocumentSource`, `PCB_Document.importChanges`) | requires native EAK implementation | `_quick-reference.md` | Note that EasyEDA offers a source-level escape hatch; EAK should keep everything inside typed commands |
| Absence of undo/redo in API | not applicable | grep negative | EAK's diff-based undo/redo is an advantage; no EasyEDA design to learn from |
| Manufacturing export set (Gerber, BOM, PnP, IPC-2581, IPC-D-356A, ODB++, DXF, DSN) | future capability | `classes/PCB_ManufactureData.md` | Candidate list for eak-manufacturing roadmap beyond Gerber/Excellon/BOM/CPL |

- Gaps/unknowns: No real source of the editor; reference docs are generated prose, not typings. Behaviour of `check()` rules and verbose DRC result shape not documented. Licence unclear (no file).

---

### 2. easyeda-api-sdk
- URL: https://github.com/easyeda/easyeda-api-sdk
- Commit inspected: f83525eb54f86f131cf90780ed3fa0237dbb75aa
- Licence: `LICENSE` = Apache License 2.0 (Version 2.0, January 2004); `package.json` `license: Apache-2.0`; `extension.json` `license: Apache-2.0` ; reusable in EAK: yes-with-attribution (but little of value to reuse)
- Language / size: TypeScript, 36 files (8 .ts, 11 .json, 6 .md). devDependencies: `@jlceda/pro-api-types ^0.4.26`, esbuild, ts-node, eslint (`@antfu/eslint-config`), jszip, ws. package version 1.6.29.
- What it is: A project template/scaffold (`npx github:easyeda/easyeda-api-sdk my-extension` via `build/create.js`) for building EasyEDA Pro extensions: esbuild bundling to an IIFE, `extension.json` manifest, packaging into `.eext` (zip via jszip), and a dev mode that pushes the rebuilt `.eext` to the client over WebSocket (port 59394). It does NOT contain API typings; those are pulled from the npm package `@jlceda/pro-api-types` (referenced in `tsconfig.json` `include`).
- Key source paths:
  - `.../easyeda-api-sdk/src/index.ts` - entry: exports `activate(status?: 'onStartupFinished', arg?)` and menu handler `about()` using `eda.sys_Dialog.showInformationMessage` and `eda.sys_I18n.text`
  - `.../extension.json` - manifest: `engines.eda ^3.2.0`, `entry ./dist/index`, `headerMenus` keyed per editor (`home`, `sch`, `pcb`) with `registerFn`, `activationEvents`, `dependentExtensions`
  - `.../build/dev.ts`, `build/packaged.ts`, `build/utils.ts`, `build/update.ts`, `build/manifest.ts`, `config/esbuild.common.ts` (platform browser, format iife)
  - `.../.sdk-manifest.json` - hash list of framework files for self-update
  - `.../locales/*.json`, `iframe/index.html`
- Implementation observations:
  - Extension model: single exported-function bundle loaded into the editor sandbox; menu entries bind to exported function names; i18n via `locales/` and `locales/extensionJson/`.
  - API boundary: the typings declare a global `eda` of class `EDA` (see repo 1 `references/classes/EDA.md`); the extension runs in the browser sandbox with only `eda.*` and no direct file access.
  - No tests; lint-staged + eslint only.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Extension manifest with per-editor `headerMenus`, `activationEvents`, `engines.eda` range | future capability | `extension.json` | If EAK adds plugins, a manifest with editor-scoped menu contributions and an engine-version range is a sound minimal design |
| Dev mode hot-push of packaged extension over local WebSocket | not applicable | `build/dev.ts` | Tauri dev loop covers this |
| Typings as separate npm package versioned independently of the app | requires adaptation | `package.json` (`@jlceda/pro-api-types`) | For an EAK plugin/agent SDK, generate TS types from Rust `Command` schemas (e.g. via a schema export) rather than hand-written |

- Gaps/unknowns: The typings (`@jlceda/pro-api-types`) were not available in this clone, so class/type listings for "SDK typings" come from repo 1 references and repo 7's description of `index.d.ts` (single declaration file, `@public/@beta/@alpha/@internal` JSDoc tags, `eda` mount union types such as `sch_PrimitiveComponent: SCH_PrimitiveComponent | SCH_PrimitiveComponent3`).

---

### 3. easyeda-format-skill
- URL: https://github.com/easyeda/easyeda-format-skill
- Commit inspected: 4fcab1747d47b2ae01d7f93a485859970c5cf933
- Licence: `LICENSE` = "MIT License / Copyright (c) 2026 EasyEDA"; `package.json` `license: MIT`; SKILL.md `license: MIT` ; reusable in EAK: yes-with-attribution (policy: learn and reimplement)
- Language / size: 917 files: 615 .md, 299 .json (297 JSON Schemas under `schemas/` plus examples), `validate.js` (1402 lines, AJV `^8.12.0` + ajv-formats). package "easyeda-pro-format-skill" 1.0.0 (CHANGELOG up to 1.4.0, dated 2026-10-22).
- What it is: The most current (called "V4") specification of the EasyEDA Pro on-disk log format, organised as an LLM skill: `documents/<DOCTYPE>.md` (15 doc types) -> `primitives/<DOCTYPE>/<primitive>.md` (field tables with constraints) -> `schemas/*.json` -> `examples/<DOCTYPE>/`. `validate.js` validates generated records against the schemas with a mandatory generate-then-validate loop (max 3 retries).
- Key source paths:
  - `.../easyeda-format-skill/SKILL.md` - record grammar, eventual-consistency rules, deletion, DOCHEAD/CANVAS rules
  - `.../FORMATLOG.md` - V3 -> V4 differences (units, booleans, doc types, arcs, layer naming)
  - `.../documents/{PCB,SCH,SCH_PAGE,SYMBOL,FOOTPRINT,DEVICE,BOARD,CONFIG,PANEL,PANEL_LIB,SIMULATION,SIMULATION_SCH,BLOB,FONT,REFERENCE}.md`
  - `.../primitives/PCB/{line,arc,via,pad,poly,fill,region,pour,poured,teardrop,layer,layer_phys,net,rule,rule_template,rule_selector,eqlen_grp,x_net,partition,...}.md`
  - `.../primitives/REFERENCE/e-layer-code.md` - layer-code enumeration
  - `.../schemas/` (e.g. `t-pcb-line.json`, `t-sch-component.json`, `t-sch-pin.json`, `t-net-class` family) and `validate.js`
- Implementation observations:
  - Record grammar: one record per line, `{"type","id","ticket"}||{inner JSON}|` with trailing `|` + LF. Append-only log; later lines override earlier ones. Conflict resolution key is document + normalised id; larger `ticket` wins; on tie the lexicographically larger DOCHEAD `client` wins (SKILL.md notes this contradicts the older V2/V3 docs, which said the smaller wins). Deletion = re-write the record with empty inner payload; document deletion = `DELETE_DOC` record `{"isDelete":true}` (undoable). This is a CRDT-like last-writer-wins register per element, designed for collaboration, and also gives free history.
  - Document types (V4 `t-doc-head`, 20 values): CONFIG (renamed from PROJECT_CONFIG), BOARD, SCH, SCH_PAGE, PCB, PANEL, PANEL_LIB, SYMBOL, FOOTPRINT, DEVICE, BLOB, FONT, VARIANT, COMPONENT_GROUP(_DATA), SIMULATION, SIMULATION_SCH, EDIT_HEAD, INSTANCE (per FORMATLOG #3). Hierarchy: CONFIG (project) > BOARD > SCH > SCH_PAGE, and PCB, PANEL; library docs SYMBOL/FOOTPRINT/DEVICE referenced by uuid.
  - Primitives are scoped by document type: `LINE` in SCH_PAGE is a wire/line, `LINE` in PCB is a copper/silk segment with `netName`, `layerId`, `width`, `startX/startY/endX/endY` (mil), `locked`, `zIndex`, `groupId`, `partitionId` (`primitives/PCB/line.md`). A `validate.js` TYPE_MAP resolves `<DOCTYPE>_<TYPE>`.
  - PCB primitive set (`documents/PCB.md`): META, CANVAS, BOARD, LAYER, LAYER_PHYS, ACTIVE_LAYER, PARTITION, NET, PRIMITIVE, GROUP, SILK_OPTS, PREFERENCE, VIA, PAD, LINE, ARC, OBJ, EQLEN_GRP, POLY, FILL, LAYER_FILL, REGION, POUR, POURED, IMAGE, TEARDROP, FPC_FILL, SHELL, CREASE (flex/3D enclosure), dimension, string, rule/rule_template/rule_selector, panelize. Component = footprint instance referencing FOOTPRINT doc.
  - Layer model in file: `LAYER` rows carry `layerId`, `layerType`, `layerName`, status bitfield (1 used, 2 displayed, 4 locked in V3 doc), colors, transparency; V3 states the count of SIGNAL/PLANE/SUBSTRATE layers is unlimited and stacking order is the order of appearance; `ELayerCode` numbering matches repo 1 (1 top, 2 bottom, 15-44 inner 1-30 ...; inner up to `Inner32` in the file-level enum). `layer_phys` stores stackup physical properties.
  - Vias: start/end layer defined indirectly by `ruleName` ("via layer type design rule"), supporting through/blind/buried; `unusedInnerLayers` list hides pads (V3 `format/pcb/pad_via.md`).
  - Units: V3 = global 0.01 inch; V4 PCB = mil, grid sizes x10 on write; schematic stays 0.01 inch; V4 adds `yAxisDirection` (`up`/`down`) for local eprj3 files; V4 booleans are JSON booleans (V3 used 1/0).
  - Schematic: components are SYMBOL-doc instances placed by `COMPONENT` records; multi-part symbols = several `PART` rows, all `COMPONENT` rows share a Unique ID (FORMATLOG #14). Wires are polylines (`dots` segment arrays) without intrinsic net data in V3 `format/schematic/wire.md`; connectivity is derived.
  - Tests: `validate.js` plus examples; no unit-test runner. The doc itself lists "holes" it cannot confirm (e.g. rect-path anchor semantics, per easyeda-eprj3-skill gotchas).
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Append-only keyed log with per-record logical clock + client tiebreak, tombstones, doc-level DELETE | future capability | `SKILL.md` (eventual consistency), `FORMATLOG.md` #7 | Strong precedent for EAK's journal; consider per-entity ids + logical clocks if multi-user/sync is ever planned. Current EAK diff-based undo + fsync journal is sufficient for Phase 1-4 |
| Strict doc-type scoping of primitive schemas (same type name differs per doc) | requires adaptation | `SKILL.md` TYPE_MAP section | Avoid this ambiguity in EAK-IR: keep distinct Rust types for schematic wire vs PCB track |
| JSON Schema per primitive + generate-then-validate loop | applicable directly | `schemas/`, `validate.js` | EAK can emit JSON Schemas from `eak-core` serde types and validate project files in CI |
| Layer model: ordered LAYER records, stackup order implied by sequence, unlimited signal/plane layers, inner layers up to 32 in code numbering | requires adaptation | `primitives/PCB/layer.md`, `primitives/REFERENCE/e-layer-code.md` | Target design for EAK's multi-layer upgrade: ordered `Vec<Layer{kind, copper|plane|dielectric}>`, not an enum |
| Via spans via named design rule (through/blind/buried) | requires adaptation | `format/pcb/pad_via.md` (repo 1) | In EAK store explicit start/end layer indices on the via instead of rule-name indirection |
| Net records only for special attributes (netType, color, diff pair, length group); net class/diff pair as separate rows | requires adaptation | `format/pcb/primitive.md` (repo 1), `primitives/PCB/net.md` | EAK net classes + constraints already cover this; note differential pair and equal-length group data model |
| Symbol/Footprint/Device split (Device links symbol + footprint + attributes, multi-part symbols) | requires adaptation | `documents/DEVICE.md`, `documents/SYMBOL.md`, FORMATLOG #14 | Matches EAK "component definition with symbol + footprint + pin map"; adopt multi-part symbol support (gate-level parts sharing a designator) |
| Flex PCB / 3D shell / creases / FPC stiffener primitives | future capability | `primitives/PCB/{shell,crease,fpc_fill}.md` | Out of scope for Phase 1-4 |
| Simulation docs (Ngspice/SimulIDE, Monte-Carlo settings) | future capability | FORMATLOG #9, #11 | Out of scope |

- Gaps/unknowns: Format is reverse-documented from the editor; no reader/writer code here. Some semantics (rect path anchors) flagged as unknown by the downstream eprj3 skill.

---

### 4. easyeda-eprj3-skill
- URL: https://github.com/easyeda/easyeda-eprj3-skill
- Commit inspected: 8ff62c549c81e1d373f4cb274871ad1ff42b61e4
- Licence: `LICENSE` = "MIT License / Copyright (c) 2026 easyeda-eprj3-skill contributors"; `package.json` has no `license` field ; reusable in EAK: yes-with-attribution (policy: learn concepts, reimplement in Rust)
- Language / size: JavaScript (Node, zero npm dependencies per package.json), 188 files, ~23.9k lines in `scripts/` + `test/` (largest: `scripts/lib/eprj3.js` 2069, `scripts/build.js` 2032, `scripts/lib/sch-route.js` 1432, `scripts/lib/sch-patterns.js` 1189). Version 2.2.7 (2026-11-09). Examples include real `.eprj3`/`.esch2` files. 18 JSON circuit "recipes" in `recipe/`.
- What it is: A skill that builds a whole EasyEDA Pro `.eprj3` folder project (schematic + placed PCB) offline from one `design.json`, with a deterministic pipeline: intake -> build -> five gates (format, ERC, layout lint, copper DRC, intent) -> report. Since 1.13 it deliberately does not route or pour; instead it drives the desktop client through the CLI (`lceda-pro`/`easyeda-pro`, V4.1.94+) for library search, schematic->PCB import, autolayout/autoroute, pour, netlist export and screenshots.
- Key source paths:
  - `.../easyeda-eprj3-skill/SKILL.md`, `docs/{format-reference,checks,cli,build,workflow,intake,footprint-evidence,corpus,development}.md`
  - `.../scripts/lib/eprj3.js` (container read/write), `scripts/lib/netlist.js`, `erc.js`, `drc.js`, `checks.js`, `sch-route.js`, `sch-place.js`, `pcb-plan.js`, `design.js`, `parts.js`, `cli-lib.js`, `audit.js`
  - `.../scripts/eda.js` (CLI bridge wrapper), `scripts/{build,erc,drc,lint-layout,intent,autofix,netcheck}.js`
  - `.../test/{smoke,pipeline,dfm,corpus}.js`, `test/corpus/*.json`
  - `.../templates/{project-index.eprj3,sheet.esch2,pcb.epcb2,schematic.ecfg,schematic.evar,panel.epan2}`
- Implementation observations:
  - Real on-disk layout of an eprj3 project (`docs/format-reference.md`): `<name>/<name>.eprj3` (pretty JSON index with `profile.{boards,schematics,sheets,pcbs,panels,...}` and uuids), `sch/<schematic>/<sheet>.esch2`, `<schematic>.ecfg`, `<schematic>.evar`, `pcb/<pcb>.epcb2`, `panel/<panel>.epan2`. Containers (`.esch2/.epcb2/.epan2`) hold several log documents back to back, each starting with a DOCHEAD line; each container embeds the SYMBOL/FOOTPRINT/DEVICE docs its placed parts need (self-contained library snapshot). `client` = first 16 hex of md5(owner_uuid). `.eprj2` = single-file SQLite; `.epro2` = zip archive (repo 5).
  - Parser caveat documented from real files: split on the first `||` only, because `||` can occur inside JSON strings; mixed DOCHEAD clients in one file are normal; ticket pair (ticket,id) must be unique per document.
  - PCB container record order: META, LAYER rows (1=Top, 2=Bottom...), mandatory empty NET, named NETs, BOARD_OUTLINE POLY (`polyType:"BOARD_OUTLINE"`, layer 11), COMPONENT blocks with PAD_NET rows (`[“PAD_NET”, componentId, padNumber, padId]` -> net), LINE tracks. FOOTPRINT PAD has `defaultPad {padType RECT|ELLIPSE..., width, height}`, `hole` (null for SMD else `THoleDef {holeType ROUND|RECT|SLOT|ROUND_RECT}`), through-hole pads on layer 12 (MULTI). Footprint coordinates are mil even when CANVAS says `unit:"mm"`.
  - Pad-to-net connectivity lives on separate `PAD_NET` records, not on the pad; schematic nets are derived from wire geometry + pin positions + net labels (`scripts/lib/netlist.js` recovers a netlist from files).
  - Gates: ERC codes E001-E008 + W001-W003 (unconnected pin, single-pin net, built-netlist != intent (`E003`), duplicate designators, part missing on PCB, PCB/schematic net mismatch, auto-named net, multiple drivers, power-net w/o flag, missing decoupling, intra-part bridge); layout lint L001-L010; copper DRC D001-D012 (clearance, min track width, edge clearance, unrouted net, missing outline, no ground pour, orphan pad, via geometry, min drill, annular ring, silk-on-pad, acute angle); scores `100 - sum(weights)`; every finding has a `fix:` string; `autofix.js` automates NC pins and decoupling caps; D004/D006 are intentionally skipped on placed-only boards ("never judge what was not promised" - consistent with EAK's rule of not reporting unknown checks as passing).
  - Real EasyEDA client interop: scripts call client APIs `lib_Device.search`, `lib_Symbol.getGeometry`, `sch_SourcePrimitiveComponent.add`, `sch_Drc.check`, `sch_ManufactureData.getPngFile`, PCB autolayout/route, pour creation; the client rewrites versions/clients on save (round-trip verified against client 4.1.84).
  - Tests: four runnable suites (`npm test` = smoke, pipeline, dfm, corpus + depcheck) plus `eval/cases.json`; I did not run them.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Layered "gates" (format/ERC/layout/DRC/intent) each with stable rule codes, severities, weights and a `fix:` suggestion | applicable directly | `docs/checks.md`, `scripts/lib/{erc,drc,checks}.js` | Mirror the rule-code table when extending eak-validation (e.g. min drill, annular ring, silk-over-pad, acute angle, multiple drivers, intent-vs-netlist comparison); keep EAK's not_implemented list |
| Netlist-from-geometry recovery and comparison against declared intent (E003) | requires adaptation | `scripts/lib/netlist.js`, `docs/checks.md` | EAK already resolves netlists; add an "intent vs built" cross-check useful for AI self-verification |
| Placed-only mode with explicit skipped rules and `rats` handoff list | applicable directly | `docs/checks.md` "Copper policy" | Matches EAK's honest-reporting principle; reuse for unrouted boards |
| Deterministic design.json -> project builder with block layout of schematic, recipes (buck, LDO, USB-C, RS485, ...) | requires adaptation | `scripts/build.js`, `recipe/*.json` | Inspiration for eak-intelligence circuit recipes emitted as Command sequences |
| Reading/writing EasyEDA eprj3 containers | requires native EAK implementation | `scripts/lib/eprj3.js`, `docs/format-reference.md` | Only if EAK wants EasyEDA import/export; implement natively in Rust from the format docs; useful as a field-level spec incl. gotchas |
| Driving the vendor GUI client via CLI for routing/pour | not applicable | `docs/cli.md` | EAK has native A* router/zone fill and must not depend on EasyEDA as engine |
| Footprint evidence / generated symbols and footprints from datasheet parameters | future capability | `docs/footprint-evidence.md`, `scripts/generate-{symbol,footprint}.js` | Relevant to eak-intelligence component provenance later |

- Gaps/unknowns: Tests and gates were not executed. Router/pour deliberately absent. Rect-path anchor semantics noted as undocumented upstream. No undo/redo concept (offline generator).

---

### 5. easyeda-file-format
- URL: https://github.com/easyeda/easyeda-file-format
- Commit inspected: 9186c4ac43e67d1bb111ada3f5cc020b76bc543a
- Licence: NO LICENSE file; GitHub reports no SPDX licence ; reusable in EAK: NO (unlicensed). Facts/concepts only.
- Language / size: 72 files: 67 Markdown (`cn/` and `en/` mirrors of `index.md`, `pcb/*`, `project/*`, `schematic/*`) and 5 PNG under `storage/images`. No code.
- What it is: The published V3 (2025.10.21) EasyEDA Pro file-format spec: log documents, record structure, eventual consistency, units, doc types, and per-primitive formats for project, schematic and PCB. Its README says the latest format is in easyeda-format-skill and V2.2 lives in `easyeda/easyeda-file-format-v2`. It also describes the `epro2` zip project archive and the `eprj2` SQLite offline project file.
- Key source paths:
  - `.../easyeda-file-format/README.md` - versions, conventions, document types, data format, eventual consistency, deletion
  - `.../en/pcb/{common,primitive,shape,pad_via,component,rule,partition,panel,3d,text,dimension,attr,obj}.md`
  - `.../en/schematic/{common,component,pin,wire,shape,structure,table,text,attr,obj}.md`
  - `.../en/project/{meta,common,blob,instance,variant,group}.md`
- Implementation observations:
  - Content of `en/pcb/shape.md` is identical (ignoring whitespace) to `easyeda-api-skill/format/pcb/shape.md`; the two repos mirror the same docs (I diffed one file; assumed same for the rest but not diffed all).
  - V3 conventions: camelCase keys, unique ids per primitive, CCW degrees, 0.01 inch units, colours `#RRGGBB` (`""` = none), booleans 1/0.
  - Project meta docs: CONFIG (`name`, `default_sheet`), BOARD (`title`,`sort`), SCH (`name`,`board`,`source`,`version`), SCH_PAGE (`name`,`schematic`,`description`,`sort`), PCB (`name`, board link) - i.e. explicit parent-pointer relations by uuid.
  - PCB design-rule system (`rule.md`): rule template -> rule -> rule selector (net / net class / region selectors) model.
  - Variants and component grouping stored as separate docs (`project/variant.md`, `group.md`) with instance attribute overrides (`instance.md`) - a design-variant (assembly variant) model.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Rule template / rule / selector model (rule applies to nets, net classes, regions) | requires adaptation | `en/pcb/rule.md` | Compare with EAK constraints + net classes; add region-scoped and selector-based constraints later |
| Assembly variants + instance attribute overrides | future capability | `en/project/variant.md`, `instance.md` | BOM/CPL variants (DNP per variant) is a plausible post-Phase-4 feature |
| Explicit eprj2 (SQLite) / eprj3 (folder JSON) / epro2 (zip) project container trio | requires adaptation | `README.md` | Folder-of-text + zip-export is a good Git-friendly pattern; EAK project store can follow similar philosophy |
| Reading the format for import | requires native EAK implementation | `en/*` | Write own Rust reader if import is ever required; spec is usable as reference but unlicensed |

- Gaps/unknowns: Says it is "not the latest"; V4 deltas are in repo 3. Licence absent.

---

### 6. easyeda-documents
- URL: https://github.com/easyeda/easyeda-documents
- Commit inspected: c5637b5f1fa0df90628032ac8ec3150a1e76d58b (partial, no-checkout; inspected via `git ls-tree`/`git show`; last commit 2026-06-23)
- Licence: no top-level LICENSE; GitHub reports archived = true, no SPDX licence; `parsedown/LICENSE.txt` is "The MIT License (MIT) / Copyright (c) 2013 Emanuil Rusev" (vendored third party) ; reusable in EAK: NO (unlicensed, and outdated)
- Language / size: 1096 files: 881 PNG, 92 GIF, 74 MD, 12 JS (mostly jQuery/sidebar helpers), 8 PDF, 6 JSON. Dirs: `Tutorial/` (467 images), `Tutorial_Chinese/`, `Open-File-Format/`, `API/example/`, `Simulation-eBook/`, `Gerber-viewer/`, `parsedown/`.
- What it is: README (first lines): this is a tutorial for EasyEDA Std (the older product), "already very outdated", kept for archival purposes only. Of interest here are `Open-File-Format/*.md` (the Std JSON format) and `API/example/*.js` (Std scripting samples).
- Key source paths:
  - `Open-File-Format/{index,common,schematic,PCB,schematicFileObject,PCBFileObject,OpenFileFormat}.md`
  - `API/example/modifyTrackVia.js`, `schematicShapes.js`, `API/example/theme/{manifest.json,main.js}`
  - `README.md`, `Tutorial/{PCB,Schematic,OpenFileFormat}.md`
- Implementation observations:
  - Std format is a JSON file whose records are `~`-delimited strings (delimiters `~`, `` ` ``, `^^`, `#@$` documented in `common.md`); head `"3~1.7.5~..."` = doc type 3 (PCB) + version 1.7.5; PCB library type 4; schematic types analogous; canvas row `CA~...` with units, grid, snap, routing angle; layers array rows like `"1~TopLayer~#FF0000~true~true~true"`; design-rule block with `trackWidth`, `track2Track`, `track2Pad`.
  - Layer numbering differs from Pro: Std lists 5/6 as Top/Bottom Paste and 7/8 as Top/Bottom Solder, whereas Pro `EPCB_LayerId` has 5/6 solder mask and 7/8 paste mask (so do not assume layer ids are portable between Std and Pro).
  - All coordinates stored as 10x mil ("stroke width 1 = 10 mil"); y axis downward from top-left in SVG canvas coordinates.
  - Std scripting API sample: `api('getSource',{type:"json"})` / `api('applySource',{source, createNew})` - whole-document read-modify-write, no incremental commands.
  - EasyEDA Std = SVG-based editor, 2 default copper layers plus inner layers in Std PCB docs (not enumerated in the files I read).
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Std (v1.x) JSON-with-tilde-delimited-strings format | not applicable | `Open-File-Format/common.md` | Only relevant if EAK wants legacy Std import (LCSC/OSHWHub parts still exist in Std format); would be native work |
| Whole-document getSource/applySource scripting | not applicable | `API/example/modifyTrackVia.js` | Contrasts with EAK's typed commands |
| Std vs Pro layer numbering mismatch | requires native EAK implementation | `Open-File-Format/PCB.md` | If EAK imports either format, maintain separate layer-id maps per source format |

- Gaps/unknowns: Large archive not cloned; inspected only the tree plus ~6 small files. Tutorial content (UI behaviour, PCB workflow) not reviewed.

---

### 7. extension-dev-skill
- URL: https://github.com/easyeda/extension-dev-skill
- Commit inspected: d9148cba524149d17afcbfd196fab3226a8df5da
- Licence: NO LICENSE file in repo (GitHub: no SPDX); SKILL.md front matter says `license: MIT` ; reusable in EAK: NO for copying (no licence file). Concepts only.
- Language / size: 69 files: 64 Markdown (49 scenario `recipes/*.md`, `resources/guide/*`, `resources/experience.md`), 4 JS in `scripts/` (`eda-api.js` 928 lines, `eda-api.test.js` 118 lines, `lint-eda-api.js`, `build-registry.js`). Skill version 2.0.0.
- What it is: A skill for writing EasyEDA Pro extensions where the authoritative API source is the target project's installed `node_modules/@jlceda/pro-api-types/index.d.ts` rather than docs. Its CLI `scripts/eda-api.js` loads the typings via the TypeScript compiler API, builds a cached type graph (keyed by package version + declaration SHA-256 + TS version), and offers `doctor`, `search`, `inspect --mount ... --member ... --closure`, and `lint` (type-check plus policy checks for runtime restrictions and `extension.json`).
- Key source paths:
  - `.../extension-dev-skill/SKILL.md` - evidence priority, type-driven rules, recipe index
  - `.../scripts/eda-api.js` (functions `buildGraph`, `extractEdaMounts`, `inspectGraph`, `closureFor`, `lintProject`, `runPolicyChecks`, `lintExtensionJson`), `scripts/eda-api.test.js`
  - `.../resources/experience.md` - pitfalls list (18 items)
  - `.../recipes/{pcb_drc_check,sch_drc_check,pcb_layer_operations,dmt_project_management,dmt_schematic_pcb_management,pcb_primitives_bindraw,sch_primitives_bindraw,pcb_event_bindraw,pcb_select_control,lib_symbol_footprint,lib_libraries_management,sch_netlist_operations}.md`
  - `.../resources/guide/{extension-json,inline-frame,invoke-apis,stability,error-handling}.md`
- Implementation observations (API boundaries and quirks, valuable for understanding EasyEDA's editor model):
  - Mount structure: `eda.<prefix>_<Class>`; some mounts are unions of class generations, e.g. `sch_PrimitiveComponent` may be `SCH_PrimitiveComponent | SCH_PrimitiveComponent3` (asserted in `scripts/eda-api.test.js`), so the schematic component API has two coexisting generations.
  - `docInfo.documentType` uses `EDMT_EditorDocumentType` (PCB = 3, not 2); current document is only available via `eda.dmt_SelectControl.getCurrentDocumentInfo()`.
  - Primitive access asymmetry (experience.md #7/#8): `get()` and `getAll()` can return different interface variants (`ISCH_PrimitiveComponent` vs `ISCH_PrimitiveComponent$1`) and `setState_*` commits only on PCB (via `.done()`); on SCH you must call class-level `modify(primitiveId, {...})`. Pins/pads have no `getState_Net`. These are evidence the editor API grew organically and EAK should not copy its shape.
  - Canvas UUID differs from Library UUID (#6).
  - Recipes show DRC flow: `pcb_Drc.check(strict, userInterface, includeVerboseError)` plus separate rule-config/net-class/diff-pair/equal-length APIs; and `sch_Drc.check`.
  - Tests: one test file for the CLI type-graph logic (not run).
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Query API surface from compiler-extracted type graph with `--closure` and cache keyed by declaration hash | applicable directly | `scripts/eda-api.js`, `SKILL.md` | For eak-intelligence tool docs, generate the tool catalogue from the Rust `Command` types so agents never rely on prose docs |
| "Evidence priority" ordering (typings > build diagnostics > JSDoc > recipes > guides) and recorded SDK proof hash | applicable directly | `SKILL.md` | Adopt in EAK agent rules: tool schema version + hash recorded with each AI-proposed change |
| Scenario recipes (trigger intent -> prerequisite -> call chain) | requires adaptation | `recipes/*.md` | Use as intent catalogue for agent tools (create board, add wire, run DRC, export BOM) |
| Runtime policy lint (forbid `alert`, `localStorage`, etc. in sandbox) | not applicable | `scripts/eda-api.js` `runPolicyChecks` | EAK has no extension sandbox yet |
| Document/primitive API asymmetries (SCH vs PCB commit semantics) | not applicable | `resources/experience.md` | Cautionary example: EAK commands must have uniform commit semantics |

- Gaps/unknowns: Typings themselves not in repo; `index.d.ts` size and union facts are from SKILL.md/test, not verified against the npm package. Licence ambiguity as above.

---

### 8. easyeda-client-cli
- URL: https://github.com/easyeda/easyeda-client-cli
- Commit inspected: 5452c638fca802ea29e27277479a56d869feb10a
- Licence: NO LICENSE file; GitHub reports no SPDX licence ; reusable in EAK: NO (unlicensed). Docs only, nothing to copy anyway.
- Language / size: 4 Markdown files (README.md 99 lines, easyeda-cli.md 115, cli-for-ai.md 94, easyeda-format.md 66). No code.
- What it is: Documentation of the CLI and local "bridge service" built into the EasyEDA Pro desktop client (v4.1.70+): `easyeda-pro` / `lceda-pro` executable with `doctor`, `session open`, `invoke` (execute JavaScript in a session), `doc` (query API reference and file-format fields at runtime), uniform JSON response envelope (`ok`, `error`), and an MCP server mode. Deliberately avoids hard-coded command examples because the CLI is still evolving.
- Key source paths:
  - `.../easyeda-client-cli/easyeda-cli.md` - concepts: bridge service, session (one editor window), invocation, response envelope; requirements; capability list
  - `.../cli-for-ai.md` - how an agent should discover commands (`--help`, `--search <keyword>`, `doc`)
  - `.../easyeda-format.md` - short V3 format summary (`.eprj3`/`.epro2`; units: schematic 10 mil, PCB mil)
- Implementation observations:
  - Session = headless or visible editor window opened against a project; commands are addressed by session id; online/browser edition has no sessions or bridge (desktop only).
  - The CLI exposes the same extension API (`eda.*`) via JS `invoke`, i.e. the vendor's "AI interface" is arbitrary JS execution plus a self-describing doc command, not typed mutations.
  - MCP server mode shows vendor direction: agents use the editor as an MCP tool provider.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Session-based headless engine addressable by CLI and MCP with a uniform `{ok,value,error}` envelope | requires adaptation | `easyeda-cli.md` | EAK CLI/MCP front end over `Engine::execute` could use the same envelope idea; EAK already shares commands across UI/CLI/AI |
| Self-describing runtime docs (`doc` command lists classes, signatures, enums, format fields) | applicable directly | `easyeda-cli.md`, `cli-for-ai.md` | Provide `eak doc commands` / schema introspection so agents discover typed commands instead of memorised prompts |
| Arbitrary JS `invoke` as the integration surface | not applicable | `easyeda-cli.md` | Violates EAK's typed-command rule |

- Gaps/unknowns: Actual CLI is closed-source (inside the client); no command reference in the repo by design; licence absent.

---

## Summary: groups A
- Document hierarchy to emulate: Project > Board (links exactly one schematic + one PCB) > Schematic > Pages; PCB and Panel as siblings; Symbol/Footprint/Device as library documents referenced by uuid and snapshotted into each container (repos 1, 3, 4, 5). EAK-IR's single-design model should gain an explicit Board/sheet-set concept; Workspace/Team/Folder are cloud concepts (not applicable).
- Layers: EasyEDA Pro supports 2..32 copper layers (`TPCB_NumberOfCopperLayers`, `EPCB_LayerId` INNER_1..INNER_30 = ids 15..44, custom layers 71-100) and the file format treats the stackup as an ordered LAYER list with physical properties (repos 1, 3). For EAK's planned multi-layer upgrade use an ordered stackup of typed layers (signal/plane/dielectric) instead of a Top/Bottom enum; avoid magic numeric ids and note Std vs Pro ids differ (repo 6).
- Storage model: Pro's format is an append-only per-record log with logical clocks (`ticket`), client tiebreaks and tombstones (repos 3, 5); `.eprj2` = SQLite, `.eprj3` = folder of JSON log files (Git friendly), `.epro2` = zip (repos 1, 4, 5). This is a precedent for EAK's journal, but EAK's integer-nm geometry and diff-based undo are better suited to single-user Phase 1-4; collaboration-grade CRDT logs are future capability.
- Units and numbers: Pro uses floating-point mil (PCB) / 10 mil (schematic), with V3 vs V4 differences; Std uses 10x mil (repos 1, 3, 5, 6). EAK's integer nanometre decision is sounder; import/export would need explicit conversion and layer-id maps per format.
- API boundary lesson: EasyEDA's `eda.*` namespaces (`dmt_`, `pcb_`, `sch_`, `lib_`, `sys_`) expose per-primitive create/modify/delete with inconsistent commit semantics (SCH `modify()` vs PCB `setState_*().done()`), mixed return interface variants, no undo/redo API, and an arbitrary-JS integration path (repos 1, 7, 8). EAK's single typed `Command` through `Engine::execute` with undo/redo is a stronger boundary; keep it, but mine the API list as a feature checklist.
- Validation: EasyEDA exposes only opaque `PCB_Drc.check` / `SCH_Drc.check` (verbose PCB result untyped; SCH error = `{type, rule, net, primitives}`) with rule configuration objects (net class, differential pair, equal-length group, pad-pair group, region/net rules) (repos 1, 7). The most reusable validation knowledge is in easyeda-eprj3-skill: coded rule tables ERC E001-E008/W001-W003, layout L001-L010, copper DRC D001-D012 including DFM rules (min drill 12 mil, annular ring 4 mil, silk-on-pad, acute-angle) and an intent-vs-netlist check (repo 4). Treat as a rule-coverage checklist for eak-validation, re-implement natively.
- Licences: only easyeda-api-sdk (Apache-2.0), easyeda-format-skill (MIT) and easyeda-eprj3-skill (MIT) ship LICENSE files. easyeda-api-skill, extension-dev-skill (both only claim MIT in SKILL.md front matter), easyeda-file-format, easyeda-client-cli and easyeda-documents (archived, only vendored parsedown MIT) have no licence file and must be treated as not reusable (repos 1, 5, 6, 7, 8). EAK policy (learn and re-implement) applies to all.
- Real code vs prompts: only easyeda-eprj3-skill (~24k lines JS with tests) and the small tools in extension-dev-skill/easyeda-format-skill/api-skill are executable; the rest is documentation. Nothing here is a real EDA kernel, router or DRC engine; the editor itself is closed-source (repos 1-8).
- Highest-value takeaways for EAK, in order: (1) eprj3-skill rule-code tables and DFM checks for eak-validation; (2) format-skill/api-skill file-format docs for a future EasyEDA import/export and as a model for multi-layer stackup, via spans, net/diff-pair/length-group data, multi-part symbols and assembly variants; (3) extension-dev-skill's compiler-derived API query CLI as a pattern for generating agent tool catalogues from Rust command types; (4) client-cli's session + MCP + self-describing `doc` design as a reference for an EAK CLI/MCP front end.


Method: read-only inspection of the shallow clones. Line counts are `wc -l`. Where only a minified/bundled
artefact exists this is stated. All ten repositories carry an identical Apache License 2.0 `LICENSE` file
("Apache License / Version 2.0, January 2004"). No `NOTICE` file exists in any of the ten. Apache-2.0 is
reusable with attribution, but EAK policy is to learn concepts and re-implement in Rust.

---

## Group B - Visual editor and interaction (repos 9-13)

### 9. easyeda-viewer
- URL: https://github.com/easyeda/easyeda-viewer
- Commit inspected: fd69dd16134a32b64084afc76fa67875d96189a5
- Licence: Apache License 2.0 (LICENSE; package.json `"license": "Apache-2.0"`) ; reusable in EAK: yes-with-attribution (concepts only per EAK policy)
- Language / size: TypeScript, 315 files, ~13.5k lines under src/test/desktop/scripts; deps `leafer-ui ^1.9.1`, `fflate ^0.8.2`; dev: vite, vitest, pixelmatch, pngjs, puppeteer-core. Optional Go + WebView2 desktop shell (`desktop/*.go`).
- What it is: A read-only, fully offline, single-file viewer for EasyEDA Pro projects (`.eprj3` folder, `.epro2`, zip, single `.esch2/.epcb2/.epan2/.esym2`, `.elibz2`, `.epru`). It parses the EasyEDA Pro line-record format in the browser and draws schematic, PCB, panel, symbol and footprint on a LeaferJS Canvas scene graph. README states explicitly: no editing, saving, DRC/ERC, BOM, 3D or Gerber export. It is a viewer, not an editable engine.
- Key source paths:
  - `docs/ARCHITECTURE.md` - 5-layer architecture (parse / neutral model / render / UI / Go shell); data flow bytes -> container sniff -> record stream -> `ProjectModel` -> lazy `openDoc` -> `renderDoc` -> `RenderObject[]` + `RenderLayer[]`.
  - `src/core/parse/records.ts` - format is newline-separated `{outerJson}||{innerJson}|` lines; many documents per file, each started by a DOCHEAD record. `container.ts`, `eprj3.ts`, `epro2.ts`, `zip.ts`, `single.ts` are container adapters.
  - `src/core/types.ts`, `src/core/model.ts` - format-neutral intermediate model (`Rec`, `DocSegment`, `OpenedDoc`, `ProjectModel`), lazy per-document opening, `resolveAttrRef` for `{=Key}` formula attributes.
  - `src/core/render/layers.ts` - `RenderApi`/`RenderObject`/`RenderLayer` contract and `pcbStackKey` layer ordering. `RenderObject` carries world bbox, optional `pickBox`, `pathPts`, `pickLayers`, `layerKey`, and a custom `hit()` callback.
  - `src/core/render/pcb.ts` (1953 lines), `sch.ts` (1514), `geom.ts` (451), `font.ts` - rendering; Y-flip `xfOf`, arc sampling into polylines, text frame polygons, pre-vectorised glyph outlines from FONT documents.
  - `src/ui/camera.ts` (111) - whole zoom/pan implementation.
  - `src/ui/shell.ts` (1446) - `onCanvasClick` hit-test and selection, layer visibility, view memory, `selectByDesignators`.
  - `src/ui/props.ts`, `tree.ts`, `measure.ts`, `units.ts`, `annotations.js`, `embed.ts` - properties panel, trees, measure tool, mm/mil toggle, annotation layer, embeddable API.
  - `docs/easyeda-viewer-api.md` - integration API (`createViewer`, `loadFiles`, `open`, annotations API; postMessage subset for cross-origin iframe).
  - `test/parse.spec.ts`, `test/sch-rotation.spec.ts` (vitest), `scripts/ref-diff.mjs` (pixelmatch regression vs official PNG exports).
- Implementation observations:
  - Rendering tech: LeaferJS (HTML5 Canvas, retained-mode Group tree), `new Leafer({ type: 'draw' })`. Leafer's own event/hit system is switched off (`hittable:false` on the world group); picking is custom. Not SVG, not WebGL.
  - Camera: a `world` Group is translated/scaled. Wheel zoom is cursor-anchored, `zoomAt(px,py,factor)` recomputes tx/ty so the world point under the cursor is invariant; factor `1.0015^(-deltaY)`; scale clamped 0.002..400. Pan on left, middle or right drag with a 3 px move threshold (`didPan`) to separate click from drag. `fit(bbox, margin)`, `centerOn`, and `getView/setView` store the viewport-centre world point (not tx/ty) so per-document view memory survives resize. Typed zoom-percent field.
  - Hit testing: on click, screen point -> world point, tolerance `6 / scale` (constant 6 px). Linear scan over all `RenderObject`s (no spatial index, no quadtree/rbush found by grep). Per object: skip hidden layers, bbox/pickBox coarse reject, then either exact point-to-polyline distance (`segDist`, tracks/wires/arcs sampled to polylines) or bbox containment plus optional stroke-only `hit()`. Tie-breaking: nearest wire wins among wires, smallest bbox area among non-wires, and when both kinds hit, the one with the smaller bbox area wins so pads/text are not stolen by a track that ends on them.
  - Layer-aware picking (PCB): when an active layer is set, only objects on that layer are pickable and there is no fall-through to other layers; through-hole objects (vias, TH pads) carry `pickLayers` listing all copper layers so any active copper layer can pick them. Component pick box is the union of body box and visible attribute text (designator/value) boxes.
  - Selection feedback: highlight overlay painted above the doc root; outline follows the real path for line primitives, rotated corners for rotated boxes, else bbox. Properties panel and object tree are two-way linked with canvas selection.
  - Hover: no hover-highlight handler found in `shell.ts` (grep for hover returned nothing relevant); interaction is click-select only. This is an observation from grep, not an exhaustive proof.
  - Grid and snapping: no snapping and no interactive grid (grep matches for "grid" are title-block table lines in `sch.ts`). Measure tool (`measure.ts`) is screen-space overlay; grep for "snap" in it returned no snapping logic. Consistent with a read-only viewer.
  - Layer visibility: `RenderLayer` groups with counts; panel toggles group visibility; layers with zero objects are hidden from the panel; stack order via `pcbStackKey`.
  - Search/navigation: project tree and object tree each have a search input (`type=search`, auto-expand when searching); arrow keys open documents; `selectByDesignators` selects by component designator (also searches sibling sheets when the open page has no hit); BOM-row/netlist selection hooks in the API. Net-name overlays and pad numbers are separate topmost overlay groups (`nn:1`, `nn:2`).
  - Performance: constant-pixel-width overlays recomputed on `onView`; per-document lazy parsing; render once into the retained tree. No Web Workers, no culling and no spatial index found by grep (`Worker|cull|quadtree|rbush|spatial` returned no hits in `src`). Leafer's retained canvas does viewport redraw internally; the project does not add its own culling.
  - Fidelity method: pixelmatch diff against official-client exports is the verification approach; `docs/PRD.md` (514 lines) records tagged requirements (`#pick-precise`, `#via-pick-any-layer`, etc.).
  - Single-file Vite build (~432 kB per docs), Windows desktop shell in Go with `//go:embed` and a loopback HTTP server.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Cursor-anchored wheel zoom, pan, fit, per-document view memory | applicable directly | `src/ui/camera.ts` | Same maths in EAK desktop SVG canvas; store view centre + scale per sheet |
| Click-to-pick with constant-pixel tolerance, nearest-segment for wires, smallest-area for boxes | requires adaptation | `src/ui/shell.ts` `onCanvasClick` | Re-implement over EAK-IR geometry in integer nm; add a spatial index (viewer has none) |
| Active-layer-exclusive picking with through-hole objects pickable on every copper layer | requires adaptation | `src/ui/shell.ts`, `src/core/render/pcb.ts` (`pickLayers`) | Directly relevant to the planned multi-layer upgrade of the 2-value copper enum |
| Component pick box = body + visible attribute text | applicable directly | `pcb.ts` (`pickBox`) | Include designator/value text extents in component hit region |
| Two-way tree <-> canvas selection, properties panel, object tree with search | requires adaptation | `src/ui/tree.ts`, `props.ts` | Drive from EAK-IR queries; all edits still via `Engine::execute` |
| Select components by designator list (BOM/netlist cross-probe) | requires adaptation | `shell.ts` `selectByDesignators`, `docs/easyeda-viewer-api.md` | Expose as UI/AI query; never mutates |
| Pixel-diff regression against reference exports | requires adaptation | `scripts/ref-diff.mjs` | Use for EAK renderer regression using EAK's own Gerber/golden renders |
| Hover highlight, grid, snapping, editing | not applicable (absent in this repo) | grep of `src/ui` | Must be built natively; this repo gives no reference for them |
| Rendering through LeaferJS Canvas | not applicable | `package.json` | EAK uses SVG canvas; Canvas/WebGL becomes a future option if SVG node counts bite |
| Parsing EasyEDA Pro `.eprj3/.epro2` | not applicable | `src/core/parse/*` | EAK rule: no EasyEDA as source of truth; at most a future import adapter |
| Annotation/review layer stored outside project files | future capability | `src/ui/annotations.js`, `docs/easyeda-viewer-api.md` | Possible review-comments feature, kept separate from IR |
| Embeddable API (`createViewer`, postMessage) | future capability | `src/embed.ts` | Only relevant if EAK ships an embeddable read-only viewer |

- Gaps/unknowns: Did not read the full 1953-line `pcb.ts`/1514-line `sch.ts`; observations on rendering come from targeted greps and sections. Leafer's internal dirty-rect/viewport culling was not inspected (dependency, not in clone). Hover absence is inferred from grep.

---

### 10. online-gencad-viewer
- URL: https://github.com/easyeda/online-gencad-viewer
- Commit inspected: cdc76cdaa520765831bf7e3674e47cb213c975f8
- Licence: Apache License 2.0 (LICENSE) ; reusable in EAK: yes-with-attribution (concepts only)
- Language / size: TypeScript, 34 files, ~3.1k lines; deps `leafer-ui ^2.1.0`, `@leafer-in/view`, `@leafer-in/viewport`; Vite + vite-plugin-singlefile. No tests or linter (stated in `CLAUDE.md`). `dist/` contains a prebuilt `online-gencad-viewer.html`.
- What it is: A browser viewer for GenCAD 1.4 (`.cad`) PCB interchange files. Parses sections, renders multi-layer PCB on LeaferJS Canvas, with component/net lists, search, filtering and highlight-with-dim.
- Key source paths:
  - `src/parser/index.ts` (351), `types.ts`, `unit-converter.ts` - GenCAD parser: HEADER, BOARD, PADS, PADSTACKS, ARTWORKS, SHAPES, COMPONENTS, DEVICES, SIGNALS, TRACKS, ROUTES.
  - `src/renderer/index.ts` (286) - layer map, bounds, fit, manual wheel zoom and drag pan.
  - `src/renderer/component-renderer.ts` (645), `route-renderer.ts`, `board-renderer.ts`, `primitives.ts`, `colors.ts`.
  - `src/main.ts` (354) - load flow, highlight/dim state, list-to-canvas panning.
  - `src/ui/layout.ts`, `layer-controls.ts`, `left-panel.ts`, `property-panel.ts`, `file-picker.ts`.
  - `CLAUDE.md`, `PRD.md`, `README-EN.md`.
- Implementation observations:
  - Rendering: LeaferJS Canvas Group tree; layer groups held in a `Map<string, Group>` keyed `ROUTE_TOP`, `PADS_BOTTOM`, `VIAS_*`, etc. with documented back-to-front stacking order incl. per-inner-layer groups.
  - Y-up GenCAD to Y-down canvas: negate Y and rotation at element creation.
  - Zoom: wheel handler in capture phase on the container, fixed factor 1.35 per notch, cursor-anchored (`newX = sx - (sx - x) * ratio`), min scale 0.01 (the viewport plugin's zoom config is max 256 but the manual handler has no max clamp). Pan: left or right drag with pointer capture.
  - Identification for interaction: custom properties (`_component`, `_signal`, `_pin`, `_padLayer`) set on Leafer nodes; no geometric hit-testing of its own.
  - Highlight: selecting a component or net dims all other groups by setting `opacity = 0.15`, restoring saved original opacity on `unhighlight`, then forces a synchronous redraw (`renderer.totalTimes = 0; renderer.render()`). Click on empty canvas exits highlight mode (pointer-down/up position compared to separate click from drag).
  - Search and navigation: left panel has component list and net list, each with search box; clicking a component pans so it is centred at the current zoom; nets pan only if needed (`panToNetIfNeeded`).
  - Filtering: top buttons toggle element classes (routes, vias, components, designators, values, net labels); right-side panel toggles individual layers. Per-layer visibility only.
  - Properties: `property-panel.ts` is a 21-line key/value list with HTML-escaping, no editing.
  - Performance: `performance.mark/measure` logging of parse/render times and element counts; no culling, spatial index or worker use found.
  - Does not model GenCAD units beyond header `UNITS` conversion; the parser skips some items (`case 'TRACK': break; // skip track ref for now`).
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Net/component highlight with dim-others (opacity 0.15) | applicable directly | `src/main.ts` `highlightNet`, `highlightComponent` | Good UX template for net cross-probe in EAK PCB view |
| Component and net lists with search, click pans to target | applicable directly | `src/ui/left-panel.ts`, `src/main.ts` | Implement over EAK-IR queries |
| Element-class filter plus per-layer visibility | applicable directly | `src/ui/layer-controls.ts` | Replicate; ties to multi-layer upgrade |
| Back-to-front layer stacking convention with per-inner-layer groups | requires adaptation | `CLAUDE.md` (stacking order) | Use as ordering reference when EAK adds inner layers |
| GenCAD 1.4 parsing | future capability | `src/parser/index.ts` | A GenCAD export/import could be a verification-by-reimport route; not Phase 1-4 |
| Cursor-anchored zoom / drag pan | applicable directly | `src/renderer/index.ts` | Same maths as easyeda-viewer |
| Canvas rendering via LeaferJS | not applicable | `package.json` | EAK uses SVG |
| Geometric hit-testing, editing | not applicable (absent) | `src/` | Not provided here |

- Gaps/unknowns: No tests. Did not read `component-renderer.ts` in full. Whether Leafer culls offscreen nodes is unknown.

---

### 11. eext-gencad-viewer
- URL: https://github.com/easyeda/eext-gencad-viewer
- Commit inspected: b537e0cc0dfcf5be544e2d0589f457f719fb21a1
- Licence: Apache License 2.0 (LICENSE; `extension.json` `"license": "Apache-2.0"`) ; reusable in EAK: yes-with-attribution (concepts only). The embedded viewer is a compiled bundle containing third-party code (LeaferJS); its licence text is not shipped in the repo.
- Language / size: TypeScript wrapper of 31 lines + one 2085-line bundled `src/online-gencad-viewer.html` (minified single-file build of #10, with a sample GenCAD file appended). 34 files total. `package.json` is the stock `pro-api-sdk` 1.3.2 template (esbuild, eslint, jszip).
- What it is: An EasyEDA Pro extension that adds a PCB header menu "Open GenCAD Viewer..." which opens the viewer HTML in an iframe via `eda.sys_IFrame.openIFrame(...)`. There is no viewer source here; the viewer is the built artefact of `online-gencad-viewer`.
- Key source paths:
  - `src/index.ts` - `openGenCAD()` opens `/dist/iframe/online-gencad-viewer.html` at 9999x9999 with maximise/minimise; `about()`.
  - `src/online-gencad-viewer.html` - minified bundle (readable only as compiled JS).
  - `extension.json` - manifest; header menu in `pcb` group; `engines.eda ^3.0.0`.
  - `需求.md` - a requirements document describing a different product ("BOM device smart footprint matching", for migrating OrCAD/Altium/PADS schematics); it does not describe this GenCAD viewer. This is a documentation mismatch in the repo.
- Implementation observations:
  - The extension itself does not export GenCAD from the open PCB; the user must load a `.cad` file manually (no EDA API call for export appears in `src/index.ts`).
  - No unique interaction logic: everything in section 10 applies, as a compiled copy.
  - No tests; lint config only.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Viewer hosted in an EDA iframe from a shipped HTML bundle | not applicable | `src/index.ts` | EAK is native Tauri; no iframe plugin model |
| GenCAD viewing | future capability | `src/online-gencad-viewer.html` | See #10; no unique value in this repo |

- Gaps/unknowns: Bundle not diffed against #10's `dist/`; assumed to be a build of it from title, structure and the matching sample GenCAD header, not byte-verified.

---

### 12. eext-gerber-viewer
- URL: https://github.com/easyeda/eext-gerber-viewer
- Commit inspected: 085eb879b61ee84dfc7288852b490990282327e5
- Licence: Apache License 2.0 (LICENSE); README says "licensed under Apache-2.0". The bundled viewer's About dialog strings say "Open Source License: MIT" and credit "tracespace v5" for parsing logic (a licence statement inconsistent with the repo LICENSE; tracespace is the third-party origin). Reusable in EAK: yes-with-attribution for concepts; do not copy the bundle because provenance of the ported code is ambiguous.
- Language / size: TypeScript wrapper (72 lines) + two readable-but-bundled HTML viewers (`src/online-gerber-viewer.html` 11,715 lines, `online-gerber-viewer1.html` 11,325 lines, differing files). 36 files. 
- What it is: An EasyEDA Pro extension that obtains the Gerber ZIP of the current PCB and opens it in a built-in multi-layer Gerber/Excellon viewer, or opens a blank viewer, or links to external viewers. README advises using JLCPCB DFM for authoritative review because "this viewer is for reference only and may not display everything correctly".
- Key source paths:
  - `src/index.ts` - `viewPcbGerber` (calls `eda.pcb_ManufactureData.getGerberFile()` per `CLAUDE.md`, passes data via `eda.sys_MessageBus`), `openGerberViewer`, `openOnlineGerberViewer`, `openEasyEDAGerberViewer`.
  - `CLAUDE.md` - build pipeline (esbuild + HTML injection of a MessageBus listener).
  - `src/online-gerber-viewer.html` - the actual viewer; the app code is readable inside the single-file build (not minified identifiers in the app part; library part is minified `ve$1`/`li$1` Leafer aliases).
- Implementation observations (all from the bundled HTML):
  - Gerber engine: a port of tracespace v5 plotter logic - `plot(tree)`, `createGraphicPlotter`, arc plotting with ambiguous-centre candidates (`findCenterCandidates`), aperture macros with expression solving (`plotMacro`, `solveExpression`) and macro primitives: circle, vector line, centre line, lower-left line, outline, polygon, moire, thermal. Regions (G36) and stroked paths supported.
  - Layer identification: filename-pattern matchers per CAD vendor (KiCad, Allegro, Eagle etc.) with a vote for the most common CAD; plus fallback on `%LN` file-function name; types copper/silkscreen/soldermask/paste/outline/drill/drawing with top/bottom/inner side. Falls back to heuristics. No use of X2 `%TF.FileFunction` attributes was found by grep (`%TF`/`%TA` returned no hits).
  - Drill alignment: `findDrillOffset` votes on (dx,dy) between drill circle centres and copper pad circle centres, samples up to 300 points, 1e-4 vote tolerance, minimum 3 inliers, then refines - corrects drill files whose origin differs from the Gerbers.
  - Rendering and batching: LeaferJS Canvas. `renderImageTree` sets `hitChildren=false; hitSelf=false` (no picking at all), emits shapes as paths, merges all strokes of the same width into path-data strings, chunked 2000 per Path node, plus a Path per region. This is the main performance technique seen: batching by stroke width and disabling hit testing.
  - ZIP import: bundled fflate-style `unzipSync` for batch import of Gerber ZIPs; progress and `yieldToMain()` between files to keep the UI responsive; toast messages; no Web Worker found (`Worker(` grep returned nothing).
  - UI: layer rows with eye toggles, fit view, zoom (`zoomLayer`), axes/units display (`convertUnit`), i18n (zh/en), About dialog. No object picking or measurement-by-click in the portion read.
  - No tests.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Independent Gerber re-parse + render of exported fab files as a review view | requires native EAK implementation | `src/online-gerber-viewer.html` (plotter) | EAK already writes RS-274X in `eak-manufacturing`; a Rust re-reader that plots the files back is the sound way to verify output, not trusting the writer |
| Aperture macro evaluation (moire, thermal, outline, polygon) | requires native EAK implementation | same | Needed if EAK ever reads third-party Gerbers or verifies its own macros |
| Drill-to-copper offset detection by voting | future capability | `findDrillOffset` | Useful for imported fab data checks |
| Filename-based layer identification | requires adaptation | `identifyLayers` | Prefer X2 file-function attributes first, filename as fallback |
| Stroke batching by width, chunked paths, hit testing disabled | applicable directly | `renderImageTree` | Pattern for fab preview performance in SVG/Canvas |
| Get Gerber from EDA via `eda.pcb_ManufactureData.getGerberFile()` | not applicable | `CLAUDE.md` | EAK calls its own manufacturing crate |
| Viewer "reference only" disclaimer | applicable directly | `README.en.md` | Matches EAK rule: no "manufacturing-ready" claim |

- Gaps/unknowns: Bundle is 11.7k lines; only the plotter, layer-identification, drill-alignment, render and unzip sections were read. Did not verify polarity (LPC/LPD clear) handling. Difference between `online-gerber-viewer.html` and `...1.html` not analysed. Licence mismatch (About says MIT, repo says Apache-2.0) is unresolved.

---

### 13. eext-interactive-html-bom
- URL: https://github.com/easyeda/eext-interactive-html-bom
- Commit inspected: 430ea9d06a1c975ed3d2c6da83a6686a1f737084
- Licence: Apache License 2.0 (LICENSE). `iframe/index.html` embeds third-party code with its own notices: Split.js v1.3.5 "MIT License", an LZString build, a pieroxy copyright notice, a jQuery Foundation notice. README states the rendering core comes from InteractiveHtmlBom (qu1ck, openscopeproject) via a port of `turbobabr/easyeda-ibom-extension`; those upstream licences are not reproduced in a NOTICE file in this repo. Reusable in EAK: yes-with-attribution for concepts; do not copy (mixed provenance).
- Language / size: TypeScript wrapper 55 lines + one 6,874-line `iframe/index.html` with all JS/CSS inline; docs (`architecture.md` 135 lines, `backlog.md`). 34 files.
- What it is: An EasyEDA Pro extension that opens (or exports as standalone HTML) an interactive BOM: BOM table on one side, PCB front/back Canvas view on the other, linked highlighting. Data is collected through EDA primitive APIs then fed into the iBOM `pcbdata` structure.
- Key source paths:
  - `src/index.ts` - `showIBOM`, `exportHTML`, `exportAdvancedHTML`, `about`.
  - `docs/architecture.md` - menu, API list (appendix A), data flow and coordinate/pad/pour/silk handling (appendix B).
  - `docs/backlog.md` - open items (back-side mirror verification, 4+ layer pours, 3D, CSV export).
  - `iframe/index.html` - viewer: canvas drawing (`drawFootprints`, `drawTracks`, `drawZones`, `drawVias`, `drawEdgeCuts`), `netHitScan`, `handleMouseClick`, `handleMouseWheel`, BOM filter, checkboxes, settings export/import.
- Implementation observations:
  - Data collection: `eda.pcb_PrimitiveComponent/Line/Arc/Polyline/Via/Pour/Poured/String/Fill.getAll()`, `component.getAllPins()`, `eda.pcb_Net.getAllNetsName()`, `eda.dmt_Board.getCurrentBoardInfo()`, and `eda.sys_FileManager.getFootprintFileByFootprintUuid()` to fetch `.elibz2` footprint files so silkscreen outlines can be drawn from the library footprint (cached per footprint UUID; silently skipped without team-library permission).
  - Data repairs: Y negation at collection (`neg`), `fixPcbData()` checks board-outline/component Y overlap and flips or shifts if under 50%; pour coordinates are in mm vs other primitives in 0.1 mm (multiply by 10); `pcb_PrimitivePad.getAll()` returns radians and duplicates component pads so it is avoided. These are EDA API quirks, documented as "known limitations".
  - Rendering: plain Canvas2D, separate canvases per layer role (bg, fab, silk, highlight) so highlight redraw does not repaint the board; crosshair for highlighted footprints; back view uses a mirrored canvas.
  - Interaction: wheel zoom is cursor-anchored, per-tick factor 1.1^(-delta/40) clamped to [0.5, 2]; pan by drag; click performs `footprintHitScan` then, if the footprint is already highlighted, `netHitScan`; clicking empty copper selects a net. `netHitScan` walks tracks (Path2D `isPointInStroke` for polylines, `isPointInPath` for polygons, distance-to-arc, distance-to-segment), then pads, then vias/zones. A `TODO: Should be refactored` comment sits at the top. Linear scan; no spatial index.
  - BOM table: filter box with highlight, grouping, per-row checkboxes (placed/soldered style) with stats, row click highlights footprint(s), net click highlights, layout settings importable/exportable as "InteractiveHtmlBom settings".
  - Export: serialises `pcbdata` into the template HTML via `JSON.stringify`, escaping `</`, and saves via `eda.sys_FileSystem.saveFile()`; LZString only has decompression so export is uncompressed (listed in backlog).
  - Tests: none present.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| BOM row <-> board highlight cross-probing, front/back switching | applicable directly | `iframe/index.html`, `README.en.md` | Strong fit with EAK BOM output and PCB view |
| Net highlight by click on track/pad/via/zone | requires adaptation | `netHitScan`, `handleMouseClick` | Re-implement against EAK-IR connectivity (exact copper geometry already in `eak-engineering`) rather than canvas path tests |
| Separate highlight canvas/layer for cheap redraw | applicable directly | `drawHighlightsOnLayer` | Use a separate SVG group/overlay for hover and selection |
| Standalone offline HTML export of board data + viewer | requires adaptation | `docs/architecture.md` section 2 | Natural release artefact from `eak-manufacturing`; must not imply "manufacturing-ready" |
| Footprint silkscreen taken from library files rather than only instance data | requires adaptation | `docs/architecture.md` appendix B | EAK-IR component definitions already carry footprint; keep single source |
| Coordinate/units repair heuristics (flip/shift if overlap < 50%) | not applicable | `fixPcbData` | Symptom of lossy EDA API; EAK's integer-nm IR should never need this |
| EDA primitive APIs (`eda.pcb_Primitive*.getAll`) | not applicable | `docs/architecture.md` appendix A | Not available outside EasyEDA |
| BOM checkboxes, filter, grouping, layout export | applicable directly | `iframe/index.html` | Reasonable BOM-view UX reference |

- Gaps/unknowns: Read selected parts of the 6.9k-line file (hit scan, wheel, draw functions, headers); did not read all BOM table code or the data-collection code. Upstream InteractiveHtmlBom licence not verified from this clone.

---

## Group C - AI component intelligence (repos 14-18)

Cross-cutting finding for group C: in all five repositories the AI (or heuristic) output is used essentially directly. There is no deterministic engineering verification of pin numbers, package dimensions or datasheet values. The only checks found are structural (JSON shape, pin-count positivity, coordinate validity) and, in the BOM matcher, a constraint that the AI must choose among real library search results. Details below.

### 14. eext-ai-library-builder
- URL: https://github.com/easyeda/eext-ai-library-builder
- Commit inspected: 27731deb50a3ca855874d5b70c32c5e2dc6888e0
- Licence: Apache License 2.0 (LICENSE) ; reusable in EAK: yes-with-attribution (concepts only). Vendored `iframe/assets/pdf.min.js`, `pdf.worker.min.js` (pdf.js, Apache-2.0 upstream) and `jszip.min.js` (JSZip is MIT-or-GPLv3 upstream) are minified; no per-file licence verified.
- Language / size: JavaScript in two big HTML files (`iframe/footprint.html` 5,208 lines, `iframe/symbol.html` 4,277 lines) + `src/index.ts` (39 lines); 40 files; no tests.
- What it is: EasyEDA Pro extension that creates symbols and footprints from PDF/image datasheet pages using a vision-language model. User uploads a PDF (rendered page by page to PNG with pdf.js), optionally box-selects regions, picks a package type, and the model returns JSON parameters, from which the extension draws primitives into the open symbol/footprint editor.
- Key source paths:
  - `iframe/footprint.html` - provider config (~line 1112), `packageParams` per package type (BGA, DIP, QFN, QFP, SOP, SOD, SOT23, SOT223 named in prompt), extraction loop (~2725-2930), `chooseBetterResult` (2603), footprint drawing with `eda.pcb_PrimitivePad.create` / `eda.pcb_PrimitivePolyline.create` (~4560-4880).
  - `iframe/symbol.html` - symbol flow: a bounding-box detection prompt ("IC package detection expert"), a pin-extraction prompt, drawing with `eda.sch_PrimitivePolygon.create` and `eda.sch_PrimitivePin.create(..., pinType)` (~4069-4200).
  - `src/index.ts` - opens the iframes (`symbol`, `footprint`), show/hide.
  - `README.en.md` - supported types table (BGA, DIP, QFN, QFP, SOP) with candid notes ("middle rectangle needs API improvement", "pads need API improvement").
- Implementation observations:
  - Provider/model: user-selectable. Built-in: Alibaba DashScope OpenAI-compatible endpoint (models qwen3.5-plus, qwen3-vl-plus, qwen-vl-max-latest, qwen2.5-vl-7b/72b, qvq-72b-preview), Zhipu BigModel (glm-4.5v), and Custom URL. A special-cased URL `https://chat.eda.cn/ai-chat-api/complete` needs no API key. API key stored with `eda.sys_Storage.setExtensionUserConfig('api_key', ...)`. Per-model token price table in the UI for cost display.
  - PDF handling: pdf.js renders pages to canvas then `toDataURL('image/png')`; text is not extracted - the model reads page images (plus optional manual crop boxes per image). The model is asked to "carefully measure the dimension callouts, in mm".
  - Prompting: Chinese prompts asking for JSON with `package_type` and a `parameters` array whose keys come from `packageParams`; when no type is chosen the extraction runs twice (first call detects the type, second uses the detected type) and `chooseBetterResult` picks between them.
  - `chooseBetterResult` is not a verification: it picks the result with more parameters (ties and near-ties go to the second run). Two runs are never compared for numeric agreement.
  - Footprint generation: deterministic geometry code per package family converts mm to mil and creates pads (BGA rows/cols with letter-number ball names via `getBGAPinName`, DIP/QFN/QFP/SOP by pins-per-side and pitch) and silkscreen/courtyard polylines. The geometry is formula-based, so given parameters the output is reproducible, but the parameters themselves are unverified model output.
  - Verification: none found. No cross-check of pin count against pad count, pitch times pin count against body size, IPC land pattern rules, thermal pad, or comparison with a second source. Errors are only about unparsable JSON or HTTP failure; user reviews a preview and can edit parameters manually (the human is the verifier).
  - Provenance: no record of datasheet name, page, or model/version written into the library item.
  - Symbol side: pin numbers/names come straight from the model's JSON, and pin types are set from a field in the same JSON.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Datasheet page -> VLM -> package parameters -> parametric footprint | requires adaptation | `iframe/footprint.html` extraction + drawing | Keep parametric generators (deterministic, testable in Rust) but gate model output behind engineering checks |
| Parametric package families (BGA/DIP/QFN/QFP/SOP) | requires native EAK implementation | `footprint.html` drawing code | Re-implement as IPC-style generators in `eak-engineering` or `eak-core` helpers with unit tests |
| Two-run extraction and "choose better" by parameter count | not applicable | `chooseBetterResult` | Counting parameters is not verification; if repeated extraction is used, compare values and flag disagreements |
| User box-selects the region of the page to send | applicable directly | `footprint.html` manual selection | Good way to reduce model error and give provenance (page + region) |
| Provider/model selection incl. custom OpenAI-compatible URL, cost display | applicable directly | provider configs | Matches EAK's model-agnostic agent |
| API key kept in app storage | requires adaptation | `setExtensionUserConfig('api_key')` | EAK uses OS keyring already |
| Verification of pin numbers/dimensions | requires native EAK implementation (absent here) | no code | Must be built: recompute and cross-check in `eak-engineering` / `eak-validation`, record `not_implemented` for what cannot be checked |

- Gaps/unknowns: Read the provider block, extraction/selection logic, comparison function and drawing call sites; did not read every line of either 4-5k-line file. No sample outputs or tests to assess accuracy.

---

### 15. eext-ai-symbol-builder
- URL: https://github.com/easyeda/eext-ai-symbol-builder
- Commit inspected: e9fb3b167356c85077307712f09e3133bd632585
- Licence: Apache License 2.0 (LICENSE) ; reusable in EAK: yes-with-attribution (concepts only)
- Language / size: JavaScript in one 1,182-line `iframe/index.html` + 35-line `src/index.ts`; 33 files; no tests.
- What it is: An older, narrower sibling of the library builder: uploads images of the symbol/pinout figure from a datasheet, sends each image to Qwen-VL on Alibaba DashScope, receives `{type, pins:[{pinNumber,pinName}]}`, and creates a schematic symbol on the canvas.
- Key source paths:
  - `iframe/index.html` - `analyzeImage` (~line 796), `createSymbol` (~1033), `createSymbolPins`, `createSymbolBoundingBox`; drawing via `eda.sch_PrimitivePolygon.create` and `eda.sch_PrimitivePin.create(x, y, pinNumber, pinName, rotation)`.
  - `README.en.md` - states the model (Qwen2.5-VL family) and output JSON, zip download of results, token/cost display.
- Implementation observations:
  - Model/provider: hard-coded `https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions`; selectable Qwen VL models (qwen-vl-max-latest default, qwen2.5-vl-7b/72b, qwen-vl-plus). Call via `eda.sys_ClientUrl.request` with `Authorization: Bearer <apiKey>`.
  - Input: images only (no PDF parsing in this repo; user supplies images).
  - Prompt: one line asking to extract `type` and the (`pinNumber`,`pinName`) list from the symbol diagram, with `type` restricted to SOP, QFN, BGA; "strictly output JSON".
  - Validation: `createSymbol` strips a ```` ```json ```` fence, `JSON.parse`, then checks only that `type` exists and `pins` is an array; later errors for non-positive pin count and invalid coordinates. No check for duplicate pin numbers, gaps in numbering, pin count vs package, or name sanity. Output is trusted directly and placed on the canvas.
  - Provenance: none recorded.
  - Results can be copied, edited in the UI and zipped; human review is the only gate.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| VLM extraction of pin number/name table from a pinout image | requires adaptation | `iframe/index.html` `analyzeImage` | Acceptable as a *proposal* source only; output must become a proposed component definition awaiting verification |
| Symbol layout by package type (SOP/QFN/BGA) | requires adaptation | `createSymbolBoundingBox`, `createSymbolPins` | EAK needs its own symbol layout generator on EAK-IR symbols |
| Schema-only validation then direct creation | not applicable | `createSymbol` | Anti-pattern for EAK (AI never self-certifies); add duplicate/gap/count checks and datasheet provenance |
| Token and cost accounting display | applicable directly | `README.en.md` | Useful for model-agnostic agent telemetry |

- Gaps/unknowns: Did not read pin-layout geometry functions in detail. Accuracy of the model is not evaluated anywhere in the repo.

---

### 16. eext-ai-device-standardization
- URL: https://github.com/easyeda/eext-ai-device-standardization
- Commit inspected: 89abac48075bd4e0ebc2a30bee55939251f8660f
- Licence: Apache License 2.0 (LICENSE) ; reusable in EAK: yes-with-attribution (concepts only). `iframe/vendor/jszip.min.js` vendored (MIT-or-GPLv3 upstream).
- Language / size: TypeScript (`src/bom-service.ts` 643, `src/index.ts` 43) + plain JS (`iframe/app.js` 5,165, `core.js` 81); 38 files. Tests: `tests/core.test.cjs` (31 lines, node:test).
- What it is: Matches schematic components or an imported BOM (CSV/XLS/XLSX) to devices in the EasyEDA/LCSC library, optionally with an OpenAI-compatible LLM, then binds the chosen device/footprint/symbol back to the schematic components (replace device, footprint only, symbol only, or data only), with an unbind to restore.
- Key source paths:
  - `src/bom-service.ts` - BOM parsing: encoding detection, CSV parser, header-row detection, own XLSX unzip/parse, column-role mapping, designator normalisation (ranges, lists), grouping, LCSC id extraction (`extractLcsc`).
  - `iframe/app.js` - `calcMatchScore` (3057), `extractCoreValue`, `aiMatchSingle` (2146), `aiMatchAll` (2395), `callAI` (2105), `searchDevice` (1778), `findFootprintFallback` / `findFootprintFallbackAI`, `bindComponent` (2780), `getOrCreateBindingVariant`, `unbindGroup`.
  - `iframe/core.js`, `tests/core.test.cjs` - HTML-escape, safe image URL, LRU cache, stable hash; the only tested code.
  - `README.en.md`.
- Implementation observations:
  - Library search: real EDA library APIs `eda.lib_Device.search`, `lib_Symbol.search`, `lib_Footprint.search`, and `lib_Device.getByLcscIds` for LCSC part-number lookup; results cached in an LRU keyed by target+library+keyword.
  - Standard matching: keywords built from user-selected BOM columns, then `calcMatchScore`: per selected column, exact equals scores 1, substring containment 0.5, score = percent of compared columns; tolerance suffix such as "5%" is stripped (`extractCoreValue`). With no columns selected: MPN/value equals device name = 100, containment = 85, else 60.
  - AI matching: an OpenAI-compatible chat completions call (`temperature: 0.3`, user-configured URL, key and model). Two prompt stages: (1) the model produces a search keyword JSON `{"keyword": ...}` with up to 3 retries if the format is wrong or search returns nothing (it must supply a different keyword each time); (2) the model is shown up to 10 library candidates (name, package, manufacturer, description) and must return `{"idx": n, "reason": ...}` or an empty array for no match, with retries on re-search. The model therefore selects among real library entries and cannot invent a part; if the model response fails to parse, the code falls back to the top deterministic score.
  - Batch AI merges several devices per request by context size; footprint fallback matches by footprint column text, with an AI variant.
  - Candidate list shows scores plus an `aiRecommended` flag, so the UI separates deterministic score from AI choice. The user still clicks Bind or Batch Bind; unbind restores the original.
  - Verification: the match score is string similarity only. No electrical equivalence check (value/tolerance/voltage/power rating parsing, pinout/footprint compatibility, MPN normalisation or alternate-part logic). Footprint compatibility is by name text only.
  - Security hygiene present: HTML escaping and image-URL scheme allowlist, with tests.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| BOM import (CSV/XLS/XLSX), header detection, designator range expansion, column-role mapping | requires adaptation | `src/bom-service.ts` | Re-implement in Rust for importing external BOMs into EAK component list |
| MPN / LCSC-id / value matching to a library with scoring | requires adaptation | `calcMatchScore`, `findMatches` | Keep deterministic scoring; add numeric parsing of values/tolerances and package equivalence |
| LLM chooses among real library search results (closed candidate set) | applicable directly | `aiMatchSingle` | Right pattern: AI ranks only candidates produced by the library index; deterministic gates after |
| LLM generates search keywords with retry on empty result | applicable directly | `aiMatchSingle` | Fine for retrieval, as long as results are re-checked |
| Bind modes (device/footprint/symbol/data) with unbind/restore | requires adaptation | `bindComponent`, `unbindGroup` | Map to typed commands with diff-based undo in `eak-runtime` |
| Library search through EDA API | requires native EAK implementation | `searchDevice` | EAK needs its own indexed library (with provenance) |
| Electrical/footprint equivalence verification | requires native EAK implementation (absent here) | no code | Build in `eak-engineering`/`eak-validation` |

- Gaps/unknowns: Read the matching, scoring, AI and search parts; did not read bind internals or the UI. Whether AI choice is ever auto-bound without a click was not exhaustively traced (batch bind exists and is user-triggered).

---

### 17. eext-datasheet-helper
- URL: https://github.com/easyeda/eext-datasheet-helper
- Commit inspected: 6be8cf0b8e28131e011ba11bf887dab4f027bed9
- Licence: Apache License 2.0 (LICENSE) ; reusable in EAK: yes-with-attribution (concepts only). Depends on third-party models with their own licences (Qwen3-0.6B, granite-docling-258M, multilingual-e5-small, Helsinki-NLP opus-mt) and on Transformers.js, pdf.js, tesseract.js; model files are downloaded at runtime, not vendored.
- Language / size: TypeScript + JS, 61 files, ~6.7k lines in `iframe/`. Deps: `@huggingface/transformers 4.0.1` (and 3.7.5 aliased as `transformers-vlm`), `pdfjs-dist 3.11.174`, `tesseract.js 5.1.0` with eng and chi_sim data. No test directory.
- What it is: A datasheet Q&A assistant opened for the selected schematic component. It obtains the datasheet PDF (from the component's LCSC/JLC datasheet link), extracts text, and answers questions either via an online OpenAI-compatible chat API or via a fully local pipeline (WebGPU/CPU in-browser VLM + embedding + LLM). It also has a PDF reader with local translation.
- Key source paths:
  - `src/index.ts` - reads selected component via `eda.sch_SelectControl.getAllSelectedPrimitives_PrimitiveId()` and `eda.sch_PrimitiveComponent.get`, stores device info (designator, manufacturerId, manufacturer, supplierId) with `eda.sys_Storage`, opens `local.html` / chat iframes.
  - `iframe/chat.js` (1,447 lines, online mode) - datasheet URL resolution (including `r.jina.ai` reader and `atta.szlcsc.com`, `datasheet.lcsc.com` patterns), pdf.js text extraction, OCR fallback with tesseract when text per page is low, keyword scoring retrieval, `buildSystemPrompt`, chat completions fetch.
  - `iframe/src/main.ts` (1,073 lines, local mode), `pdf.ts` (text blocks with coordinates, page canvas rendering), `vlm.ts` (tiled page recognition), `inference.ts`, `inference-worker.ts` (Web Worker), `inference-device.ts`, `ort-runtime.ts`, `document.ts`, `model-store.ts` (imported-model cache in browser Cache Storage), `model-response.ts`.
- Implementation observations:
  - PDF parsing: pdf.js `getTextContent` with per-item transform to build lines and blocks with x/y/width/height; if average text per page is low, OCR with Tesseract at a configurable scale in English or Chinese+English. Page rasters bounded to ~2200 px on the long side.
  - Local VLM path: page canvas is split into overlapping horizontal tiles (about 0.6x width per tile, 24 px overlap), blank tiles skipped by ink sampling, each tile sent to the VLM (granite-docling) and the DocTags output converted to text; overlap lines removed only at tile borders. Inference runs in a Web Worker. Embedding model used for retrieval (`embed`) in local mode; chat mode uses keyword frequency per page.
  - Online-mode retrieval: keyword match counts per page, 800-char chunks, capped total context, labelled `[Page N]`. Prompt tells the model to answer only from the supplied excerpts, state when information is absent, give values with units, and not invent. It is Q&A over excerpts, not structured extraction.
  - Provider: user-configurable OpenAI-compatible `apiBaseUrl` + key + model for online mode; local mode uses Hugging Face model downloads (default mirror configurable).
  - Outputs: free-text answers; page references appear in the context but no machine-readable extraction of pin tables, package dimensions or ratings is produced, and nothing is written back to the design.
  - Verification: none. The only grounding is prompt instructions and page tags; there is no automatic check that a quoted number appears in the source text, and no structured schema.
  - Hard-coded dependence on third-party reader proxies (`r.jina.ai`) for resolving datasheet web pages. That sends the datasheet URL to an external service.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Datasheet PDF text extraction with coordinates and OCR fallback | requires native EAK implementation | `iframe/src/pdf.ts`, `chat.js` | Do text/table extraction in Rust (PDF crate) with page+bbox provenance, keep OCR/VLM as optional fallback |
| Tiled page recognition with overlap for dense tables | future capability | `iframe/src/vlm.ts` | Useful technique for pin tables on large pages |
| Retrieval with embeddings (local) or keyword scoring (online) | requires adaptation | `main.ts`, `chat.js` | Page-cited retrieval is the right shape for evidence for ratings/pins |
| Prompt: "answer only from excerpts, say if absent" | applicable directly | `chat.js` `buildSystemPrompt` | Necessary but not sufficient; EAK must recompute numbers and verify quotes |
| Local in-browser model inference (WebGPU) | future capability | `inference-worker.ts` | Privacy option for confidential datasheets; heavy |
| Datasheet link resolution via LCSC patterns and jina reader proxy | not applicable | `chat.js` | Do not depend on third-party proxies; user-supplied PDFs and explicit fetch |
| Structured extraction (pin table, package dims, ratings) with provenance | requires native EAK implementation (absent here) | no code | This repo only answers questions; EAK's ratings-with-provenance needs structured extraction plus verification |

- Gaps/unknowns: Read the PDF, VLM, retrieval and prompt code; did not read the rest of `main.ts` or settings pages. No accuracy evaluation or tests exist.

---

### 18. eext-component-box
- URL: https://github.com/easyeda/eext-component-box
- Commit inspected: 0390a4c0e9dba6643ca2d18f8ff78328548efa70
- Licence: Apache License 2.0 (LICENSE; `Hardware/` firmware has no separate licence file found). Dependency `xlsx ^0.18.5` plus vendored `iframe/xlsx.full.min.js`. Reusable in EAK: yes-with-attribution (but low relevance).
- Language / size: JavaScript (`iframe/app.js` 506, `box.js` 990) + `src/index.ts` (118) + ESP32-S3 C++ firmware under `Hardware/` (BLE, OLED, LED grid, buttons, serial comm). 64 files; no tests.
- What it is: A personal component inventory ("component library") plus a 5x7 physical storage box mapped to inventory entries. It imports LCSC order spreadsheets, looks up each LCSC C-number through `eda.lib_Device.getByLcscIds`, tracks quantities, lets the user place a stored component onto the schematic or PCB, and syncs with ESP32-S3 hardware over USB serial or BLE. README: browser environment only, not client.
- Key source paths:
  - `src/index.ts` - `openLibrary`, `openBox`, `connectBox`, `findSelectedComponent` (via `eda.sch_SelectControl` / `eda.pcb_SelectControl`, `eda.sys_Storage`).
  - `iframe/app.js` - import (`.xls/.xlsx`, header cells like "LCSC Part Number", "立创商城编号"), batch lookup `eda.lib_Device.getByLcscIds`, match by `supplierId`, search filter on name/C-number/description, edit, `eda.sch_PrimitiveComponent.placeComponentWithMouse`.
  - `iframe/box.js` - 5x7 grid mapping, serial/BLE sync.
  - `Hardware/src/*.cpp` - firmware.
- Implementation observations:
  - No AI or LLM involved despite the "AI component" group heading (grep found none in the files read); the "intelligence" is order-sheet parsing and exact C-number lookup.
  - Library provenance: devices come from the LCSC library API by supplier id; fields kept include LCSC id, name, package, description, manufacturer, quantity and notes. Persistence is `eda.sys_Storage` user config keys (`myComponentLibDevices`, `myComponentLibBox`) - browser-local, not a portable file.
  - Search: case-insensitive substring over name, C-number and description; no fuzzy ranking despite the README word "fuzzy".
  - Inventory/hardware sync is about physical stock, not design correctness.
- Features and classification:

| Feature | Classification | Evidence (path) | Recommendation for EAK |
|---|---|---|---|
| Personal parts inventory with quantities, filter and edit | future capability | `iframe/app.js` | Possible later link between BOM and stock; outside Phase 1-4 |
| Order-sheet import to part numbers + quantities | requires adaptation | `iframe/app.js` | Same BOM/order import idea as #16 |
| Exact lookup by supplier part number (`getByLcscIds`) | requires native EAK implementation | `app.js` | EAK needs its own supplier-id/MPN index with provenance |
| Physical storage box over BLE/serial | not applicable | `Hardware/`, `box.js` | Unrelated to EAK scope |
| Place stored component into design | requires adaptation | `placeComponentWithMouse` | EAK equivalent is a typed place-component Command |

- Gaps/unknowns: Did not read `box.js` in full or firmware. Firmware licence not stated separately.

---

## Summary: groups B and C

- **Viewers are click-select, linear-scan, no spatial index.** easyeda-viewer and iBOM both hit-test by scanning every object (bbox coarse reject, then exact stroke/segment distance); neither has a quadtree, culling or workers. EAK can reuse the hit-resolution rules (constant-pixel tolerance, nearest wire, smallest-area box, pad over passing track) but should add a spatial index over integer-nm geometry. (easyeda-viewer, eext-interactive-html-bom)
- **Layer-aware picking is a real design lesson.** Active-layer exclusive pick with no fall-through, and through-hole objects pickable on any copper layer via a layer set, maps directly to EAK's planned multi-layer upgrade. (easyeda-viewer)
- **Zoom/pan is trivial and consistent across all viewers:** cursor-anchored scale (factor 1.0015^-dy, 1.1^-dy/40 clamped, or fixed 1.35), clamped scale, drag pan with a few-pixel click-vs-drag threshold, view memory stored as centre point + scale. Adopt as-is in the SVG canvas. (easyeda-viewer, online-gencad-viewer, iBOM)
- **No viewer here provides hover, grid, snapping or editing.** All are read-only; these must be built natively as part of the editor and are not covered by any reference. (easyeda-viewer, online-gencad-viewer, eext-gerber-viewer, eext-interactive-html-bom)
- **Highlight patterns worth copying:** dim-everything-else (opacity 0.15) for net/component focus, separate highlight overlay so hover/select does not repaint the board, BOM-row to board cross-probing, and a standalone offline HTML export. (online-gencad-viewer, eext-interactive-html-bom)
- **Fab-data review should be an independent re-read.** The Gerber viewer re-plots exported files (apertures, macros, regions, drill-offset detection) and carries a "reference only, use DFM" disclaimer. For EAK, a Rust Gerber/Excellon re-reader in verification is the sound way to check `eak-manufacturing` output; stroke batching by width in chunks is the useful render-performance technique. (eext-gerber-viewer)
- **AI library builders trust model output.** Symbol and footprint builders convert VLM JSON straight into pins and pads; the only checks are JSON shape and numeric validity, "choose better" of two runs counts parameters instead of comparing values, and no datasheet/page/model provenance is stored. EAK should treat such output as an unverified proposal and verify pin sets, pitch/pin-count/body consistency and package rules in `eak-engineering`/`eak-validation`. (eext-ai-library-builder, eext-ai-symbol-builder)
- **Parametric footprint generators are the reusable part.** Deterministic per-package geometry from parameters (BGA, DIP, QFN, QFP, SOP) is testable and belongs natively in Rust; the unreliable step is parameter extraction. (eext-ai-library-builder)
- **Best AI pattern seen: model picks from a closed candidate set.** The device-standardisation matcher has the LLM propose search keywords and then choose an index among real library results, with a deterministic percentage score and a fallback to the top score if the AI reply is unparsable. Its score is still only string similarity (no electrical or package equivalence), and its tests cover only helper functions. (eext-ai-device-standardization)
- **Datasheet helper is Q&A, not structured extraction.** It does page-cited retrieval (keyword or embeddings), OCR fallback, tiled VLM reading, and prompts the model to avoid invention, but extracts no typed pin/package/rating data and does no verification; it also routes URL resolution through a third-party reader proxy. EAK needs structured extraction with page+bbox provenance and recomputed values. (eext-datasheet-helper)
- **Licensing is clean at repo level, messy inside bundles.** All ten are Apache-2.0 with no NOTICE files; bundled viewers embed third-party code (LeaferJS, tracespace port, Split.js, LZString, pdf.js, JSZip) and one claims MIT in its UI while the repo is Apache-2.0. Do not copy any bundled code into EAK; learn the concepts only. (eext-gerber-viewer, eext-interactive-html-bom, eext-ai-library-builder, eext-gencad-viewer)
- **eext-component-box is out of scope for AI intelligence**: no AI, just LCSC order import, inventory and ESP32 hardware sync. Only the order-import / supplier-id lookup idea transfers. (eext-component-box)

## Groups D and E - PCB routing/engineering, design rules, manufacturing and simulation

All findings come from files read in the clones. "EasyEDA API" below means the host-provided `eda.*` extension
runtime (`eda.pcb_PrimitiveLine`, `eda.pcb_Drc`, ...). None of these extensions can run outside EasyEDA Pro.
General note for all `eext-*` repos in this report: they are TypeScript plugins that read/write through the EasyEDA
API, so the *glue* is never reusable in EAK; only the algorithms/concepts are of interest.

---

### 19. eext-open-auto-router
- URL: https://github.com/easyeda/eext-open-auto-router
- Commit inspected: 2a308fdd2debac93fcaace0cb89b8cdf63c48db9
- Licence: LICENSE = "GNU GENERAL PUBLIC LICENSE Version 3, 29 June 2007"; `NOTICE` says it is a port of OpenAutoRouter (GPL-3.0) and that the placement and routing algorithms "and many of their constants are derived from that project" ; reusable in EAK: **NO (GPL-3.0). Describe concepts only; do not copy code or constants.**
- Language / size: TypeScript, 77 files, ~13k lines (largest: `src/router/pathfinder.ts` 939, `grid.ts` 727, `placement/anneal.ts` 673). Iframe HTML + a bundled compute worker.
- What it is: Island-based auto-placement plus negotiated-congestion auto-routing for EasyEDA Pro, ported from the Python OpenAutoRouter project (source comments name the Python originals, e.g. `router/pf_body.py`, `router/astar_nb.py`). Everything runs locally in the editor process (worker), no remote service and no external tool.
- Key source paths:
  - `src/router/pathfinder.ts` - PathFinder negotiated-congestion loop (class `Pathfinder`).
  - `src/router/astar.ts` - A* search on the routing grid; `MinHeap` binary heap tie-broken by node id.
  - `src/router/grid.ts` - flat 2D lattice per copper layer, per-net windowed Euclidean distance fields.
  - `src/placement/islands.ts` - functional island construction; `src/placement/anneal.ts` - two-stage simulated annealing + legalisation.
  - `src/eda/pour.ts` - ground pour; `src/eda/rules.ts` - DRC rule reading; `src/eda/read.ts`/`write.ts` - board I/O.
  - `src/analysis/evaluate.ts` - open/clearance/via/length scoring and `pruneClearanceConflicts` safety net.
  - `src/worker/{client,direct,compute}.ts`, `src/pipeline.ts`, `iframe/progress.html` - background compute, progress, cancel.
- Implementation observations:
  - **Algorithm (routing)**: rip-up-everything PathFinder. Each iteration (`routeIterations` default 40, over-use cap 50) rips up all nets, routes every net in priority order (group, priority, pad count, name), counts colliding cells, adds history cost, grows over-use factor. Cost is *multiplicative* per cell: corridor squeeze x layer preference x over-use (where corridor < min width) x history. Best iteration is remembered and restored; a final phase routes with clearance as a hard constraint and a score guard picks the certified result.
  - **A* details**: state = (layer, cell); 8 compass moves plus through-via layer change; diagonal cost pitch*1.4142; octile heuristic scaled by the smallest multiplier in the window (admissible), with an optional inflation factor for bounded-suboptimal speed; a 45-degree step may not cut a blocked corner; windowed (bounding box + margin) search per net. Vias are through vias only.
  - **Grid**: pitch derived from board size (`gridPitch: 0` = auto); pads rasterised as obstacle cells tagged with net id (so a net can cross its own copper); board edge distance field; `capH` (room beside foreign pads/fixed copper/edge, hard) and `capS` (room beside foreign routed copper, soft); a track of width w fits where cap >= w. Existing filled zones are fixed copper, and a net may reuse its own pour as a target.
  - **Convergence signal**: true geometric clearance violations measured on real geometry, not halo cell count (comment explains halo count grows with copper length).
  - **Push-and-shove**: none. Interactive routing: none (batch only). Rip-up and retry is global (negotiation), plus a post-pass "repair then prune": conflicting primitives are deleted (open rather than short) and reconnection passes run with surviving copper fixed.
  - **Placement**: functional islands (chip = anchor; decoupling caps attached via supply rail ownership; other small passives by net affinity), laid out in the anchor frame with a weighted-distance cost; coarse anneal moves islands (x, y, rotation) on wirelength + overlap + board overshoot + aggressor/victim separation; fine anneal moves parts; identical-footprint swaps; greedy legalisation. Iterations 12,000 coarse / 24,000 fine by default. Incremental cost (only terms touching moved parts). Seeded RNG (`common/rng.ts`) for determinism.
  - **Ground plane**: after routing, one board-wide GND pour on a free (inner or bottom) layer, built as an inset polygon; EasyEDA itself performs the fill (`rebuildCopperRegion`); pads stub into it. Own pours are tagged and replaced on re-run; user pours never touched.
  - **Clearance/design rules**: reads safe spacing, track width, via size, board-edge clearance and solder mask expansion from the PCB rule set; runs native DRC (`eda.pcb_Drc`) after write-back and includes the result in the report.
  - **Background computation/cancel/progress**: the extension host has no `Worker`, so the worker is hosted in an iframe (`iframe/compute.html`), with main thread <-> iframe messaging over `eda.sys_Storage`; on the web editor a direct `Worker` is used with a DOM overlay; if both fail it falls back to main-thread (opt-in). Own progress dialog with stages, percent, timer and cancel; cancelled jobs are never silently retried. Pure engine code in `worker/compute.ts` never touches `eda`, which is a clean engine/IO split.
  - **Tests**: no unit tests in the clone; only `dev/harness.ts` (a CLI driver against a live editor). No determinism/regression suite visible.
  - **Constraints**: two-layer prototype focus ("prototype / teaching / validation"; README says it does not replace PI, impedance, EMC, stackup design). No diff-pair or length-match awareness found.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | PathFinder negotiated congestion on a grid | requires native EAK implementation (GPL-derived; describe only) | `src/router/pathfinder.ts` | EAK already has a native 2-layer A* router; consider adding a negotiation loop with history cost, designed from the published PathFinder literature, not from this code |
  | A* with octile heuristic, windowed search, via layer-change | requires adaptation | `src/router/astar.ts` | Concept already covered by `eak-engineering` A*; windowing per net is a useful speed idea |
  | Per-net windowed distance-transform clearance fields | requires native EAK implementation | `src/router/grid.ts` | Candidate for integer-nm grid in EAK; keep geometry exact via existing copper geometry |
  | Functional-island placement (anchor + decoupling caps) | requires native EAK implementation | `src/placement/islands.ts` | Strong fit for the constructive placer; EAK component definitions carry pin maps/ratings that can drive anchor/decoupling detection more robustly than reference-prefix heuristics |
  | Two-stage simulated annealing with incremental cost | requires native EAK implementation | `src/placement/anneal.ts` | Future refinement of placer; keep seeded determinism |
  | Prune-instead-of-short safety net | applicable directly | `src/analysis/evaluate.ts` | Matches EAK principle that unknown/failed states are never reported as pass; open nets must be reported, never silently shorted |
  | Worker host + progress + cancel | requires adaptation | `src/worker/*`, `src/pipeline.ts` | In EAK: run router in a Rust task with cancel token + progress events over Tauri IPC; every result applied via one `Command` |
  | Ground pour after routing | requires adaptation | `src/eda/pour.ts` | EAK has raster zone fill; add a "GND plane" command with approval |

- Gaps/unknowns: Not read in full: `evaluate.ts` scoring weights, `read.ts` footprint/pad extraction, locales. Convergence quality was not measured (no builds allowed).

---

### 20. eext-freerouting-intergration
- URL: https://github.com/easyeda/eext-freerouting-intergration
- Commit inspected: a96e2e9c5cee4da03d667cbfe32f6a6a6378c108
- Licence: LICENSE = "Apache License Version 2.0, January 2004"; package.json `"license": "Apache-2.0"`; reusable in EAK: yes-with-attribution (but the engine it drives, Freerouting, is a separate external GPL-3.0 program that the user installs; this repo only contains the client).
- Language / size: TypeScript, 42 files, ~5k lines including iframe HTML. No runtime deps beyond esbuild/ts tooling.
- What it is: A thin client that exports the PCB as Specctra DSN, uploads it to a locally running Freerouting v2.2.3+ HTTP API (port 37864, started by shipped launch scripts), polls job status, and imports the Specctra SES result back with the EasyEDA API.
- Key source paths:
  - `src/router/FreeRoutingRouter.ts` - orchestration, polling loop, cancel, partial-result preview.
  - `src/api/FreeRoutingAPI.ts` - REST client (`eda.sys_ClientUrl.request`).
  - `src/importer/SESImporter.ts` - SES import via `eda.pcb_Document.importAutoRouteSesFile`.
  - `scripts/start-freerouting-{windows.bat,linux.sh,mac.sh}` - launch API in headless mode with auth disabled.
  - `iframe/routing.html`, `iframe/progress.html`, `iframe/service-not-found.html` - UI.
- Implementation observations:
  - **Where routing runs**: external tool, local HTTP service (Freerouting), **interchange = Specctra DSN out, SES in**. DSN is produced by `eda.pcb_ManufactureData.getDsnFile`, base64-encoded, uploaded.
  - **REST flow**: `GET /system/status` (health), `POST /sessions/create`, `POST /jobs/enqueue` (name, priority NORMAL), submit input (DSN base64), settings (e.g. max_passes), start, poll status + job logs, `getJobOutput` for final SES, `getJobOutputPartial` for preview.
  - **Algorithm**: inside Freerouting (not in this repo). Exposed knobs: max passes, via cost, max threads, improvement threshold, trace pull-tight accuracy, start ripup costs, automatic neckdown, allowed via types.
  - **Progress/cancel**: polling with backoff (initial 300 ms), parses job logs to derive pass count and unrouted count into a percentage; Stop keeps the current result; periodic intermediate SES import updates the canvas (README admits old copper is cleared before each preview import, so preview is destructive). Timeouts wrap every call (`withTimeout`).
  - **Undo/safety**: before import, all unlocked tracks/arcs/vias are deleted (README: "existing routes are not preserved"); a pre-route snapshot of primitive IDs is taken (`collectRouteIds`) but no restore path was seen. Optional auto-DRC (`eda.pcb_Drc.check`) after import.
  - **Layer names**: converted from Freerouting names to EasyEDA names.
  - **Tests**: none found.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Optional external router via DSN/SES over a local HTTP API | requires adaptation | `src/router/FreeRoutingRouter.ts`, `src/api/FreeRoutingAPI.ts` | EAK must not make Freerouting mandatory; an *optional* adapter (DSN export, SES import mapped to `Command`s, GPL process kept external) is a viable later feature |
  | Job lifecycle (session, enqueue, poll, partial output, stop) | applicable directly | `FreeRoutingRouter.ts` | Good template for any long-running external/AI router job model with cancel and progress |
  | Specctra DSN/SES as interchange | requires native EAK implementation | `SESImporter.ts` | Would need native DSN writer + SES reader in `eak-manufacturing`/`eak-engineering` |
  | Destructive preview refresh | not applicable | README | Avoid; EAK previews should be non-mutating overlays until approved |

- Gaps/unknowns: Freerouting's own algorithm (maze search, rip-up) is not in this repo. The exact request settings payload beyond `max_passes` was not fully traced.

---

### 21. easyeda-pcb-router
- URL: https://github.com/easyeda/easyeda-pcb-router
- Commit inspected: 1f230c15ec8215a385cd260f2ecdbbc22464eb79
- Licence: LICENSE = "GNU GENERAL PUBLIC LICENSE Version 3, 29 June 2007"; `engine/NOTICE.md`: Freerouting copyright Alfons Wirtz, Andras Fuchs and contributors, GPL-3.0-or-later; worker code uses the same GPL licence; reusable in EAK: **NO (GPL-3.0). Describe concepts only.**
- Language / size: Java (Java 8 facade + separate Java 25 worker), 356 files; vendored Jetty 9.2.22 + WebSocket JARs in `lib/`; also contains a Python integration test (`tests/websocket_integration.py`) which would be disallowed in EAK.
- What it is: A local WebSocket server (Jetty) that the EasyEDA web editor connects to; it accepts a Specctra DSN, runs a headless Freerouting engine, streams progress and returns EasyEDA-compatible JSON. Two engines: a legacy fork of Freerouting embedded in-process (`freerouter/`) and a default "modern" worker that runs the pinned, SHA-256-verified official Freerouting 2.5.0 JAR in a separate Java 25 JVM.
- Key source paths:
  - `AGENTS.md`, `README.md` - architecture (AutoRouter -> RouterServer -> WSService/WSHandler -> RouterExecutor -> RoutingEngine -> Legacy or Modern).
  - `easyrouter/com/easyeda/router/{WSHandler,RouterExecutor,RouterServer,DsnFileUtil,Specctra}.java`, `easyrouter/com/easyeda/router/script.js` (browser client), `engine/` (adapters; legacy/modern selection), `engine/upstream.json` (pinned manifest).
  - `freerouter/autoroute/*` - maze-search engine: `MazeSearchAlgo`, `BatchAutorouter`, `BatchFanout`, `BatchOptRoute`, `AutorouteEngine`, `MazeShoveTraceAlgo`, `InsertFoundConnectionAlgo`, `LocateFoundConnectionAlgo45Degree/AnyAngle`.
  - `freerouter/designformats/specctra/` - DSN/SES parser/writer (JFlex scanner).
  - `worker/com/...`, `tests/com/...` (Java regression/lifecycle tests, `HungWorker`).
- Implementation observations:
  - **Where routing runs**: local service (WebSocket JSON) that wraps Freerouting; interchange DSN in, JSON result out (SES conversion utilities exist in `Specctra.java`/`DsnFileUtil.java`).
  - **Algorithm (from file names and AGENTS.md, Freerouting-derived)**: free-space expansion rooms/doors maze search (`ExpansionRoom`, `ExpansionDoor`, `ExpansionDrill`, `MazeListElement`), batch autorouter with rip-up passes (`BatchAutorouter`), shove-trace algorithm (`MazeShoveTraceAlgo`), 90/45/any-angle track location, fanout pass (`BatchFanout`), and post-route optimisation (`BatchOptRoute`: pull-tight, via reduction). So this is the only repo in the group with real push-and-shove and rip-up/retry, but it is the upstream Freerouting design.
  - **Cancellation / robustness**: modern engine runs in an isolated JVM; startup validates the Java runtime and artifact checksum and "never silently falls back to the legacy engine"; `HungWorker` test implies hung-worker handling. Timeout and progress interval are configurable.
  - **Doc caveat**: AGENTS.md states some protocol docs differ from actual behaviour, so docs should be checked against source.
  - **Build**: Ant + Ivy, pinned upstream JAR (~65 MB) fetched at first build.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Free-space-expansion maze router with shove and rip-up | requires native EAK implementation (GPL; concept only) | `freerouter/autoroute/` | Reference for what a high-quality interactive/auto router needs; do not copy |
  | Isolated worker process with pinned artifact + checksum | requires adaptation | `engine/upstream.json`, `AGENTS.md` | Pattern for any external optional engine in EAK: pinned, verified, sandboxed process, no silent fallback |
  | Progress streaming over WebSocket | requires adaptation | `WSHandler.java`, `RouterExecutor.java` | EAK uses Tauri events instead |
  | Post-route optimisation (pull-tight, via reduction) | requires native EAK implementation | `BatchOptRoute.java` | Add as a native post-pass after A* |

- Gaps/unknowns: Java sources not read line by line; algorithm statements are inferred from class names, AGENTS.md and README. `worker/` contents and the modern pipeline were not read.

---

### 22. eext-kirouting-integration
- URL: https://github.com/easyeda/eext-kirouting-integration
- Commit inspected: d682f9f99f018582450e216f75cd19672b09f031
- Licence: **no LICENSE file in the clone**; `package.json` and `extension.json` both say `"license": "Apache-2.0"`; README says kirouting-integration Apache-2.0 and KiCadRoutingTools MIT. Treat as metadata-only licence claim; reusable in EAK: only with caution (no licence file) - concepts only.
- Language / size: TypeScript extension (1,057-line `src/index.ts`) + a Python FastAPI bridge (`bridge_server/*.py`, ~1,700 lines: server, routing_runner, easyeda_to_kicad, kicad_diff, analysis, models, layer_mapping). Also committed `dist/index.js`, a built `.eext`, and `generate_html.py`. Python deps: fastapi, uvicorn, pydantic, numpy.
- What it is: Bridges EasyEDA Pro to the external KiCadRoutingTools project (Rust-accelerated A* router, driven from Python). The extension collects the whole PCB as JSON, posts it to a local FastAPI server (`localhost:8765`), which converts it to a `.kicad_pcb`, runs routing, diffs input vs output and returns new tracks/vias.
- Key source paths:
  - `CLAUDE.md`, `README_EN.md` - architecture and API table.
  - `src/index.ts` - `BridgeClient`, `collectFullPCBData`, `applyResults`, DRC pre-check, MessageBus bridge to the iframe UI.
  - `bridge_server/server.py` (job state machine, one job at a time, cancel flag), `routing_runner.py` (3 modes), `easyeda_to_kicad.py` (S-expression writer), `kicad_diff.py`, `analysis.py`, `layer_mapping.py`.
- Implementation observations:
  - **Where routing runs**: external tool via local HTTP bridge; **interchange = KiCad `.kicad_pcb` S-expression** (not DSN/SES). The actual router (KiCadRoutingTools, MIT) is not in this repo; it is downloaded by start scripts and a Rust `grid_router` is built with `python build_router.py`.
  - **Algorithm (per README, not verifiable in this repo)**: A* pathfinding with MPS net ordering, rip-up and reroute, bus detection; diff-pair routing (centerline + offset, polarity swap, GND via placement); BGA fanout mode. Routing modes: `single_ended`, `diff_pair`, `bga_fanout`.
  - **Job API**: `POST /api/route` -> `job_id`; `GET /api/status/{id}`; `GET /api/result/{id}`; `POST /api/cancel/{id}`; `/api/analyze/{board-summary,power-nets,diff-pairs,bus-groups,net-stats}`; state machine pending -> converting -> routing -> ... Cancellation is a flag checked by the job thread; a new job cancels the previous.
  - **Safety**: parameter minimums enforced server-side (e.g. clearance >= 0.05 mm, board-edge clearance >= 0.5 mm to avoid staircase artefacts); extension pre-checks values against `pcb_Drc.getCurrentRuleConfiguration()` and blocks submission on violations. Result application deletes existing lines/vias of target nets first, then creates in parallel batches of 50.
  - **Coordinate/layer handling**: EasyEDA mil, top-left origin vs KiCad mm, bottom-left; Y flip; dynamic inner-layer renumbering (In1.Cu..). Supports up to 30 inner layers (EasyEDA IDs 15-44).
  - **EAK-relevance warning**: requires Python at runtime. EAK's no-Python rule rules out shipping this; only the *pattern* (export -> route -> diff -> apply) is portable.
  - **Tests**: none in this repo (CLAUDE.md says tests live in the sibling KiCadRoutingTools repo).
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | KiCad-format bridge to external router | not applicable (EAK forbids KiCad as engine and Python) | `bridge_server/easyeda_to_kicad.py` | Do not adopt |
  | Diff input vs output to apply only new copper | applicable directly | `bridge_server/kicad_diff.py` | Maps to EAK diff-based apply: external result -> list of typed Commands |
  | Net/bus/diff-pair/power analysis endpoints | requires native EAK implementation | `bridge_server/analysis.py` | Feed to `eak-engineering` and to the AI tool layer |
  | Parameter minimums + DRC pre-check before job | applicable directly | `src/index.ts`, `routing_runner.py` | Validate router parameters against board rules before launch |
  | Rust A* router with rip-up/reroute (external, MIT) | future capability | README_EN.md | Study KiCadRoutingTools upstream (MIT, Rust core) as a design reference |

- Gaps/unknowns: Router internals live in an external repo that was not cloned. Only the bridge was read. Exact job state names beyond README/CLAUDE.md not verified.

---

### 23. eext-deeppcb-autorouter
- URL: https://github.com/easyeda/eext-deeppcb-autorouter
- Commit inspected: f519a70d17b99e60510abe93f0bdf15d1c1c08a8
- Licence: LICENSE = "Apache License Version 2.0"; `NOTICE`: unofficial port of instadeepai/deeppcb-kicad-plugin (Apache-2.0, InstaDeep Ltd); DeepPCB name/logo are InstaDeep trademarks; reusable in EAK: yes-with-attribution for the client code, but the routing itself is a paid proprietary cloud service.
- Language / size: JavaScript in `iframe/` (core.js 722, app.js 960, ui.js 879) + a 141-line TS entry + a Node CORS proxy (`tools/deeppcb-proxy.mjs`). 49 files.
- What it is: Sends the PCB (as Specctra DSN) to the DeepPCB cloud AI router (`https://api.deeppcb.ai/api/v1`), tracks credits/job status, and imports the SES solution back into EasyEDA.
- Key source paths:
  - `iframe/core.js` - `DP.request`, `getBalance`, `createBoard`, `submitBoard`, `stopBoard`, `resumeBoard`, `downloadArtifact`, `rateBoard`, `pollBoardUntil`, board status table.
  - `iframe/app.js` - `eda.pcb_ManufactureData.getDsnFile` (line ~212) and `eda.pcb_Document.importAutoRouteSesFile` (line ~333).
  - `tools/deeppcb-proxy.mjs` - loopback CORS proxy (127.0.0.1:8787) needed because the DeepPCB API only allows `https://app.deeppcb.ai` origin.
  - `README.md`, `NOTICE`.
- Implementation observations:
  - **Where routing runs**: **remote service** (DeepPCB cloud AI). Interchange = DSN up, SES down; REST endpoints include `/boards`, `/boards/{id}` (submit, stop, resume, artifacts, rate) and `/apiuser/credit-flow`.
  - **Lifecycle**: statuses Pending, Created, Starting, Running, ReceivingRevisions, Done, Stopped, Failed, StopRequested, StopFailed; revisions history, import latest or selected; session restore across window reopen.
  - **Cost/limits**: user-supplied API key, paid credits, allocated time 5-120 min (default 45), 5-minute unlock fee non-refundable; pre-flight limits: <= 8 copper layers, 1,200 airwires, 1,000 components, 2,200 pins.
  - **Privacy implication**: full board geometry leaves the machine; API key held in extension storage.
  - **Cancel/progress**: Stop endpoint plus polling; no local algorithm.
  - **Tests**: none.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Cloud AI autorouter via DSN/SES | future capability | `iframe/core.js`, `app.js` | Optional provider adapter only, with explicit user consent, key in OS keyring (EAK already has keyring support), never mandatory |
  | Board-size pre-flight limits | applicable directly | `README.md` | Generic pattern: validate before sending external job |
  | Job/revision history and resume | requires adaptation | `core.js` | Model as an EAK "routing job" with revisions that become Commands only on approval |

- Gaps/unknowns: DeepPCB algorithms are proprietary and not visible. Request/response schemas were only sampled.

---

### 24. eext-diff-pair-assistant
- URL: https://github.com/easyeda/eext-diff-pair-assistant
- Commit inspected: 99fc251f2a4a550d72609a54bd1d96dade84cf1f
- Licence: LICENSE = "Apache License Version 2.0"; package.json Apache-2.0; reusable in EAK: yes-with-attribution (concepts only per EAK policy).
- Language / size: HTML/JS inside iframes (`iframe/index.html` 1,846 lines, `rule-manager.html` 734) + a 39-line TS entry. 40 files.
- What it is: **Not a router.** It identifies differential pairs from net names (names equal except for a configurable P/N token set, in prefix/infix/suffix position), shows them in a grouped table, and writes them into the PCB design rules as differential-pair definitions; a rule manager edits/renames/deletes existing pairs.
- Key source paths:
  - `src/index.ts` - opens the two iframes only.
  - `iframe/index.html` - `normalizeRule`, `matchSuffix`, `normalizeNetByRule`, `buildExistingMap`, `buildGroups`.
  - `iframe/rule-manager.html`.
- Implementation observations:
  - **Matching**: two nets are a pair if they are identical after replacing the user-supplied tokens (case-insensitive, regex-escaped); rules can have multiple characters; custom rules and removable defaults.
  - **API used**: `eda.pcb_Drc.createDifferentialPair / deleteDifferentialPair / getAllDifferentialPairs / modifyDifferentialPairName`, `eda.pcb_Net.getAllNetsName / getNetLength / highlightNet / selectNet`, `eda.sys_FileManager.getDocumentSource/setDocumentSource`, `eda.dmt_EditorControl.zoomToSelectedPrimitives`; CSV export.
  - **Routing**: none. No coupling, gap or uncoupled-length computation; only net-length display.
  - **Tests**: none.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Name-based diff-pair detection with configurable tokens | requires adaptation | `iframe/index.html` | Implement in `eak-engineering` netlist resolution; combine with pin-function metadata from component definitions to reduce false positives; propose via a typed Command with approval |
  | Rule manager (rename/delete pairs batch, apply together) | requires adaptation | `iframe/rule-manager.html` | Batch edits as one transactional Command |
  | Net-length display next to pair | applicable directly | `iframe/index.html` | Use exact copper-length calculators |
  | Differential-pair routing | not applicable (absent) | - | Use 22/25 for ideas |

- Gaps/unknowns: The two HTML files were skimmed by function names and API usage, not fully read.

---

### 25. eext-quick-equal-length
- URL: https://github.com/easyeda/eext-quick-equal-length
- Commit inspected: 8c2b3de7a6b689ec1b0ced183ed97496e9c035b7
- Licence: LICENSE = "Apache License Version 2.0"; package.json Apache-2.0; reusable in EAK: yes-with-attribution (concepts only per EAK policy).
- Language / size: TypeScript, 74 files, ~3.9k lines in `src/` (largest `push-routes.ts` 759, `length-planner.ts` 674, `bundle-length.ts` 293). Many `scripts/preview-*.mjs` scenario scripts. v1.9.1.
- What it is: Length tuning for selected traces: generates single-sided serpentine, double-sided serpentine, "large fold-back" or mixed meanders to reach a common target length, with dynamic amplitude, automatic pushing of neighbouring routes, multi-trace joint planning, and a differential mode that requires a parallel coupled section.
- Key source paths:
  - `src/length-planner.ts` - `selectTargets`, `serpentCandidates` (generator), `planLengths`, `planLengthsSteps`, `measureGroups`; types `TuningPattern = 'single' | 'double' | 'large'`.
  - `src/push-routes.ts` - `Box`, `Obstacle`, `Route`, `RoutePlan`, route building (`buildRoutes`), distance helpers, offset-by-normal route pushing; `src/push-clearance.ts` (clearance by rule object type).
  - `src/adaptive.ts` - per-crest amplitude profile (`profileGeometry`, `fitAdaptiveLineSteps`); `src/mixed.ts`, `src/bundle-length.ts`, `src/differential.ts` (`differentialPair`, `pairCoupled`, `differentialSlideSteps`, `pairedMeanderSteps`).
  - `src/transaction.ts` - `replaceLayout`/`matchLayout` (write-back with before/after matching); `src/work.ts` - `type Steps<T> = Generator<...>`.
  - `src/controller.ts`, `panel.ts`, `iframe/`.
- Implementation observations:
  - **Algorithm**: candidate-generation search. Meander candidates are produced by generator functions (`serpentCandidates`), each is collision-tested against obstacles (`collision(...)` with clearance) and measured; amplitude profile per crest fitted to reach the length delta within a tolerance (default 0.1 mil). Differential mode slides/meanders both traces while preserving the coupled geometry (`PairGeometry` has normal, spacing, limits). "Auto find balance": if the longest net cannot be tuned, it adjusts the longest first, then the others.
  - **Data structures**: polyline `Line` segments, `Route` (ordered lines + points + fixed-end flags), axis-aligned `Box` obstacles with net, layer, terminal and arc flags; spatial checks through segment/box distance.
  - **Push-and-shove**: yes in a limited form: "automatic push" modifies surrounding movable routes (`RoutePlan.pushed`, `LineChange {before, after}`), with an option "gap-only" to avoid moving anything.
  - **Cooperative computation**: algorithms are `Generator`s (`Steps<T>` yields progress strings), so the UI can interleave work and cancel (`controller.ts` handles a `cancel` action). No Web Worker. Timers via `eda.sys_Timer`.
  - **Parameters**: spacing W 20 mil, min amplitude 10, max amplitude 100, clearance 6, tolerance 0.1, wave count 0 = auto (max 48), mode (single/double/large/mixed), direction auto/left/right.
  - **Pad/layer handling**: reads lines, arcs, vias, pads, regions, fills, polylines, components as obstacles; layer auto-detect; length of "complete trace" via connectivity (`connectivity.ts`) across vias.
  - **Transaction**: `replaceLayout` deletes old lines/arcs and creates the planned ones with `eda.pcb_PrimitiveLine.create/delete`, then reads back actual lengths and reports error.
  - **Tests**: no unit tests; many `scripts/preview-*.mjs` scenario scripts (differential, mixed, spacing45, seven-reflow ...) act as manual regression previews.
  - **EAK note**: EAK-IR uses integer nm; this code uses floating-point mils.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Serpentine/meander generation (single/double/large/mixed) | requires native EAK implementation | `src/length-planner.ts`, `adaptive.ts` | Add a length-tuning module to `eak-engineering` working on integer-nm geometry |
  | Dynamic per-crest amplitude | requires adaptation | `src/adaptive.ts` | Include in the native tuner |
  | Push neighbours to make room | future capability | `src/push-routes.ts` | Needs a proper shove engine; defer |
  | Differential coupled-pair length matching | future capability | `src/differential.ts` | Needs diff-pair rule support in EAK-IR constraints first |
  | Generator-based progress/cancel | applicable directly | `src/work.ts`, `controller.ts` | In Rust: iterator/yield points with a cancel token |
  | Length-match group read from rules (equal-length groups, net classes) | requires adaptation | README, `controller.ts` | EAK constraints/net classes should include length-match groups |

- Gaps/unknowns: Collision and push internals were read at signature level only; no measurements of performance.

---

### 26. eext-pad-fanout
- URL: https://github.com/easyeda/eext-pad-fanout
- Commit inspected: 7c88e571457e6d63463b7cb391e1e62bdbdd02d7
- Licence: LICENSE = "Apache License Version 2.0"; README "Apache License 2.0"; reusable in EAK: yes-with-attribution (concepts only per EAK policy).
- Language / size: TypeScript, 46 files; `src/index.ts` 1,978 lines (UI/event glue), `src/fanout-routing.ts` 435 lines (pure planner); a real test file `tests/fanout-routing.test.ts` (231 lines, ~12 tests) and `docs/DOC-001..010`.
- What it is: Creates fanout via-and-trace stubs from selected pads in 8 directions, in batch, staggered, or via cursor click, following the PCB design-rule via size and net track width, including blind/buried vias.
- Key source paths:
  - `src/fanout-routing.ts` - `planRoute(request)` returning `RouteResult {points, endOffset, length}`; obstacle union `CircleObstacle | SegmentObstacle | PolygonObstacle`; 8 octilinear `DIRECTIONS`.
  - `src/index.ts` - pad reading, UI messaging (`sys_MessageBus`), via/line creation, rule reading.
  - `tests/fanout-routing.test.ts`; `docs/DOC-003-technical-spec.md`, `DOC-010-params-pcb-rule-integration.md`.
- Implementation observations:
  - **Algorithm**: bounded candidate enumeration, not a graph search. For each endpoint on up to 8 rings around the desired via position (`MAX_ENDPOINT_RINGS = 8`), generate straight or two-segment (and bend-offset variants, at most 6 steps) octilinear paths with 2-4 points; accept the shortest path that clears all copper obstacles (clearance + track/via radius), board outline polygons and keepout polygons. Hard budgets: `maxCandidates` 20,000 and `timeLimitMs` default 120 ms.
  - **Rules**: `eda.pcb_Drc.getCurrentRuleConfiguration`, `getNetRules`; same-net copper is connectable; non-top copper blocks a through via but not top track segments; foreign-net pours reject candidates; board cutouts/slots rejected.
  - **Tests**: pure-function unit tests for the planner (unobstructed, diagonal detour 45 degrees, same-net, via moved to nearest ring, pour rejection, outline/cutout rejection). This is the best-tested plugin in group D.
  - **API**: `eda.pcb_PrimitiveVia.create`, `pcb_PrimitiveLine.create`, `pcb_PrimitiveVia/Line.delete` (replace on re-run), `pcb_Event.addMouseEventListener` for cursor mode.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Pad escape/fanout planner (octilinear, ring search, time budget) | requires adaptation | `src/fanout-routing.ts` | Add a native fanout command; also use as the escape step ahead of A* for dense/BGA parts |
  | Rule-following via/track defaults, per-net width | applicable directly | `src/index.ts`, `docs/DOC-010...` | Resolve from EAK net classes and constraints |
  | Blind/buried via fanout | future capability | README | Requires multi-layer upgrade of EAK layer enum |
  | Staggered fanout | requires adaptation | README | Small add-on to the planner |
  | Pure planner separated from UI with unit tests | applicable directly | `tests/fanout-routing.test.ts` | Same structure as EAK crates: pure function + tests |

- Gaps/unknowns: `index.ts` UI glue was not read in full.

---

### 27. eext-pcb-component-grouping
- URL: https://github.com/easyeda/eext-pcb-component-grouping
- Commit inspected: 95421773a0180b9c0f48b4fe382419c673bafe2a
- Licence: LICENSE = "Apache License Version 2.0"; package.json Apache-2.0; reusable in EAK: yes-with-attribution (concepts only per EAK policy).
- Language / size: TypeScript, 45 files, ~3.7k lines (`src/grouping/core.ts` 1,293; `src/source/source-grouping.ts` 713; `shape-regions.ts` 342; `schematic-source.ts` 262). Tests: `tests/shape-geometry.test.ts`, `tests/shape-regions.test.ts` with a fixture.
- What it is: Reads the schematic (rectangles, closed polylines, circles/ellipses, or sheet pages) and arranges the matching PCB components into groups (one per rectangle or sheet), drawn with frame and label overlays. This is *placement by schematic region*, not auto-placement; it is the closest thing to "functional grouping" in the set, driven by the user's schematic drawing instead of net analysis.
- Key source paths:
  - `src/grouping/core.ts` - `collectProjectGrouping(mode)`, `GroupingMode = 'rectangle' | 'page' | 'selection' | 'hybrid'`; `pickRectangleByPoint/BBox`, overlap helpers, `packTextRows`, `createPcbOverlays`.
  - `src/source/source-grouping.ts` - writes new PCB document source, readback verification, backup/restore (`getDocumentSource`/`setDocumentSource`).
  - `src/source/shape-regions.ts`, `shape-geometry.ts`, `schematic-source.ts`, `pcb-source.ts`, `footprint-source.ts`.
- Implementation observations:
  - **Algorithm**: assign each schematic component to the smallest containing region (by point or bbox overlap), then per group place components at a temporary origin using prefix-grouped shelf packing, then pack the group rectangles into rows with exact 120 mil gaps, trying several row widths and choosing the layout closest to square; finally translate to grid positions.
  - **Data flow**: edits the PCB *document source text* rather than calling per-primitive APIs, then reads back to verify, saves, and keeps one rolling backup for "revert grouping". Uses footprint bounding boxes (`getEffectiveFootprintBBox`) for spacing.
  - **Safety**: README warns to back up; verified on an x86 motherboard project.
  - **Tests**: geometry unit tests present; no placement-quality tests.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Schematic-region-to-PCB-group placement | requires adaptation | `src/grouping/core.ts` | EAK-IR has sheets/symbols; add "functional group" metadata on schematic regions and use it to seed the constructive placer |
  | Shelf packing of groups into a near-square layout | applicable directly | `core.ts` | Simple deterministic seed layout for placer |
  | Source-text write + readback + backup | not applicable | `source-grouping.ts` | EAK uses typed Commands with diff undo; no source-file hacking |
  | Group frame/label overlays | future capability | `createPcbOverlays` | Silkscreen/fab-layer group annotations |

- Gaps/unknowns: The 1,293-line `core.ts` was read at function-list/comment level only.

---

### 28. eext-balance-copper
- URL: https://github.com/easyeda/eext-balance-copper
- Commit inspected: 4a52f9021a9df487c9f3d2694f1aa67ef528d5ec
- Licence: LICENSE = "Apache License Version 2.0"; package.json Apache-2.0; reusable in EAK: yes-with-attribution (concepts only per EAK policy).
- Language / size: TypeScript, 40 files, ~2.9k lines (`core/polygonUtils.ts` 753, `sourceParser.ts` 696, `index.ts` 661, `clearanceEngine.ts` 355, `patternGenerator.ts` 190). Has `PRD.md` (Chinese) and a stray `_inspect.js`.
- What it is: Fills empty board areas with non-functional (no-net) copper patterns (9 shapes: circle, square, rectangle, diamond, oblong, triangle, pentagon, hexagon, trapezoid) on selected signal/mask layers to balance copper density, with obstacle avoidance and DRC clearance.
- Key source paths:
  - `src/core/clearanceEngine.ts` - `SpatialIndex` grid of obstacles with per-obstacle margins.
  - `src/core/patternGenerator.ts` - emits FILL primitives as document-source lines (`buildFillLine`, `{"type":"FILL"...}`, `fillStyle: 'SOLID'`, empty net).
  - `src/core/densityEstimator.ts` - `estimateDensity` (board area vs fill bounding boxes).
  - `src/core/polygonUtils.ts`, `sourceParser.ts` (footprint `.elibu` parsing for pads and MULTI-layer slot regions), `boardOutline.ts`, `obstacleCollector.ts`, `constants.ts`.
- Implementation observations:
  - **Algorithm**: regular lattice of shapes (size, second size, horizontal/vertical pitch, rotation, optional row stagger and adjacent-layer stagger), each candidate tested against a uniform-grid spatial index of obstacles (track capsules, real pad shapes with rotation, vias, pours, existing fills, slots) expanded by the DRC clearance matrix value for that object type; board outline may be polyline/fill/region, rounded-rect, with slots.
  - **Density**: `estimateDensity` sums *bounding-box* areas of existing fills only, so it is a rough estimate (an approximation; tracks/pads not counted in the lines read). Target density quoted in PRD is 40-60%.
  - **Stop/cancel**: user can stop generation; progress is reported; optional automatic DRC afterwards.
  - **Writes** via document-source injection (fills as source text) rather than per-primitive API.
  - **Limits**: no pour-based thieving, no thermal or antenna analysis; no tests found.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Copper-thieving pattern fill with clearance against all copper | requires native EAK implementation | `core/clearanceEngine.ts`, `patternGenerator.ts` | Build on EAK exact copper geometry; fill as zone-clipped pattern |
  | Per-layer density estimate | requires adaptation | `core/densityEstimator.ts` | Compute exact density from EAK copper geometry; report as limitation/estimate in manufacturing checks |
  | Stagger between adjacent layers | future capability | PRD.md | Needs multi-layer upgrade |
  | Thieving as a DFM aid | future capability | PRD.md | Beyond Phase 1-4; optional release-gate suggestion, not an auto-pass |

- Gaps/unknowns: Fill-ownership tracking (`__bc_generated_fills` global) was only noticed, not traced. Density is not validated against real copper area.

---

### 29. eext-jlc-order-dfm-checker
- URL: https://github.com/easyeda/eext-jlc-order-dfm-checker
- Commit inspected: afd538786d510f537ad4fa47c6329e6a99dc7625
- Licence: LICENSE = "Apache License Version 2.0"; package.json Apache-2.0; reusable in EAK: yes-with-attribution for the code. The *capability numbers* are derived from JLCPCB public process pages (cited in `standards.ts` comments); EAK must not hard-code them as universal DRC and must record provenance. Also: README.en.md is an unedited SDK template; the real README is in Chinese (`README.md`).
- Language / size: TypeScript, 55 files; `src/index.ts` is 3,566 lines (all 18 PCB + 7 SMT checks), `src/dfm/{standards,geometry,pad-spacing,locate,xlsx,types}.ts`; `doc/` planning notes. Dependency JSZip (xlsx writer).
- What it is: Checks the current PCB against JLCPCB process capability: 18 PCB DFM items, 7 SMT-assembly items, plus same-net pad spacing; shows an overview table, clickable violations that locate/zoom to the primitive, and exports TXT/XLSX reports.
- Key source paths:
  - `src/dfm/standards.ts` - `JLC_MATERIAL_STANDARDS` (FR4, HDI, high-frequency, aluminium, copper-core), `SMT_STANDARDS` (`economy`, `standard`), `resolveMaxSize/ViaSpec/SlotWithCopper/MinTrace/OuterCopper`, `COMMON_PACKAGES`, `normalizeEiaPackage`.
  - `src/dfm/types.ts` - `CheckResult`, `ViolationCoord`, `PcbDfmResult`, `SmtDfmResult`, `MaterialStandard`.
  - `src/index.ts` - `checkMaterialType, checkLayerCount, checkBoardSize, checkBoardThickness, checkOuterCopperThickness, checkInnerCopperThickness, checkViaType, checkHoleDiameter, checkViaPadDiameter, checkSlotWithCopper, checkSlotWithoutCopper, checkMinTraceWidth, checkMinTraceSpacing, checkPadToLineSpacing, checkPluginPadRingWith/WithoutCopper, checkBgaPad, checkString` (silkscreen) and SMT: `checkSolderingSides, checkSmtLayerCount, checkSmtBoardThickness, checkSmtBoardSize, checkSmtMinPackage, checkSmtIcPinPitch, checkSmtBgaPitch`.
  - `src/dfm/geometry.ts` (point/segment/parallel-line distances, pad ring width), `pad-spacing.ts`, `locate.ts`, `xlsx.ts`.
  - `doc/DFM需求.txt`, `doc/板材标准.txt`, `doc/板材标准重构方案.md`.
- Implementation observations:
  - **Capability tables: yes.** Typed tables keyed by board material, with layer-count tiers, copper-weight tiers (1/2/2.5/3.5/4.5/5/6 oz) giving min trace width/spacing (e.g. 1 oz: 0.10/0.10 mm, multilayer 0.09/0.09), via drill and pad ring minima by layer tier, slot minima with/without copper, max board size by layer count, thickness range. SMT: economy vs standard (min package 0402 vs 0201, IC pitch 0.4 vs 0.35, BGA pitch 0.5 vs 0.3).
  - **Geometry**: line-to-line and point-to-segment distance, parallel-line distance, pad bounding circle, oblong holes; copper-to-copper spacing is computed from primitives pulled via the EasyEDA API, not from a net/zone model (pours and fills only partly handled).
  - **Output**: results typed `success/warning/error`, each with primitive IDs and coordinates for locate; XLSX built by hand with JSZip; log written with `eda.sys_Log.add` (57 calls) to the bottom log panel.
  - **Limits**: JLC-only; no Gerber-based checks (works on the design database); the author notes custom parameters are provided natively in EasyEDA V4. Checks count 18+7 is as the README claims; not individually validated here. No tests.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Fab capability profile (typed table by material/layers/copper) | applicable directly (as a concept) | `src/dfm/standards.ts` | Introduce a versioned, provenance-tagged "fab profile" data file in `eak-manufacturing`; user-selectable; numbers are inputs, never baked into engine code |
  | DFM check catalogue (18 PCB + 7 SMT) | requires adaptation | `src/index.ts` | Use as a checklist for MECH/DFM rules in `eak-validation`; report unimplemented ones in `not_implemented` |
  | Clickable violation to primitive/coordinates | applicable directly | `src/dfm/locate.ts`, `types.ts` | EAK reports should carry primitive IDs + nm coordinates for canvas jump |
  | Same-net pad spacing check | requires adaptation | `src/dfm/pad-spacing.ts` | Add to DRC set |
  | SMT assembly (min package, pitch) from BOM/CPL | future capability | `checkSmt*` | Tie to BOM/CPL release gate; keep "not manufacturing-ready" wording |
  | XLSX/TXT report | requires adaptation | `src/dfm/xlsx.ts` | EAK report formats should be JSON + CSV/PDF later |

- Gaps/unknowns: Individual check thresholds and the 3.5k-line index.ts were not read line by line. Currency of JLC numbers is unknown (no date stamp inspected).

---

### 30. eext-design-rule-generator
- URL: https://github.com/easyeda/eext-design-rule-generator
- Commit inspected: 2cf9635b335bf8c15cd046a1c2cf08f8b9118f88
- Licence: LICENSE = "Apache License Version 2.0"; package.json Apache-2.0; reusable in EAK: yes-with-attribution for code. **Caveat**: the impedance model is a surrogate trained on samples from the JLCPCB web calculator (jlcpcb.com) snapshots, so the fixtures (46k-line `fixtures/calibration/template-process.json`, golden files) are derived from a third-party service whose terms were not checked; do not import the data without legal review.
- Language / size: TypeScript, 92 files; many test files in `src/core/*.test.ts` (rules, stackup, impedance, JLC surrogate/solver/oracle/W1-accuracy/holdout); large JSON fixtures; `requirements-calibration.txt` (numpy 2.5.2, scipy 1.18.1) and docs that reference `python scripts/train-jlc-calibration.py`. The training scripts are not in the clone (only `scripts/build-jlc-core.mjs` is), so that pipeline is not reproducible from this checkout.
- What it is: Classifies nets by naming (ground, power, impedance, differential, signal), picks a JLC stackup, solves trace width/spacing for target impedance with an offline model, and saves a *new* named design-rule configuration (not activated, existing rules untouched).
- Key source paths:
  - `src/core/rules.ts` - `classifyNets`, `buildRulePlan`, `mergeRuleConfiguration`, `mergeNetRules`, `mergeDifferentialPairs`, `summarizePlan`.
  - `src/core/stackup.ts` - `StackupTemplate`, `matchingStackups`, `templateToStack`, `evaluateStackups`, `selectStackup`, `resolveDifferentialGap`.
  - `src/core/impedance.ts` (analytic types/inputs), `jlc-surrogate.ts` (`JlcGeometry`, `JlcRbfModel` with RBF centres/weights), `jlc-offline.ts`, `jlc-impedance-solver.ts`, `jlc-modes.ts` (12 calculator modes), `jlc-current.ts` (trace current).
  - `src/eda/runtime.ts` - `createSnapshot`, `applyPlan`, `restoreBackup`, `readBackupSummary` (backup of rule configuration).
  - `docs/offline-impedance-calibration.md`; `iframe/stackups.json` (JLC stackup templates).
- Implementation observations:
  - **Impedance**: not a closed-form IPC/Hammerstad solver: an RBF (radial basis function) surrogate per mode, fitted to JLC calculator outputs, with a bisection width solver (stop when |dZ| <= 0.5 ohm) and exact-match "anchor" widths from independently sampled website results. Doc states verified schemes can be auto-recommended; unverified ones are estimate-only and cannot be applied; "verified" means only consistency with the website model, not a manufacturing tolerance guarantee. Er uses calculator defaults.
  - **Stack-up**: template DB by layer count/thickness/copper weight (`stackups.json`) matched to a filter, with visual preview; stack model converts to H1/H2/T1/W1/W2 geometry.
  - **Rule application**: via `eda.pcb_Drc.overwriteNetRules` plus saving a new rule configuration; backup/restore snapshot; differential-pair merge.
  - **AI**: README mentions "AI scan" of nets as an alternative to naming rules; not examined.
  - **Tests**: the best unit-test coverage of the group (accuracy and holdout tests on surrogate).
  - **EAK relevance**: EAK already has IPC-2221 calculators; impedance solvers (microstrip/stripline/coplanar, edge-coupled diff) are missing. Use closed-form or field-solver references with explicit stated accuracy, not a scraped surrogate.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Net classification by naming (ground/power/diff/impedance/signal) | requires adaptation | `src/core/rules.ts` | Use as a seed for net-class proposals; AI proposals must be re-verified by `eak-engineering` |
  | Impedance-driven width/spacing from stackup | requires native EAK implementation | `src/core/jlc-surrogate.ts`, `jlc-impedance-solver.ts` | Implement native analytic models; label estimates and tolerances; never claim verification against a fab |
  | Stackup template DB tied to fab | future capability | `iframe/stackups.json` | Stack-up data model in EAK-IR is a prerequisite (multi-layer upgrade) |
  | Save as new rule profile, do not activate, backup/restore | applicable directly | `src/eda/runtime.ts` | Matches EAK approval-gated Command model; propose rules as a draft revision |
  | RBF surrogate trained from scraped web calculator | not applicable | `docs/offline-impedance-calibration.md` | Avoid (provenance/licence, and Python training pipeline conflicts with EAK rules) |

- Gaps/unknowns: Fixtures and calibration JSON were not inspected beyond size; solver numerics and AI-scan code were not read.

---

### 31. eext-netlist-explorer
- URL: https://github.com/easyeda/eext-netlist-explorer
- Commit inspected: 6661961fc8780e13b97a9450a96afbaaf2960bf7
- Licence: LICENSE = "Apache License Version 2.0"; package.json Apache-2.0; reusable in EAK: yes-with-attribution (concepts only).
- Language / size: TypeScript entry (163 lines) + one 3,703-line HTML (`iframe/netlist.html`) holding all UI/analysis logic. 33 files.
- What it is: Pulls the schematic netlist from the editor and offers multiple views: full netlist, connection-reference netlist, connector pin mapping diagram, component topology diagram, raw JSON, BOM extraction with CSV export; it can draw the generated diagram back onto the schematic as polygons/text.
- Key source paths:
  - `src/index.ts` - MessageBus handlers (`netlist-analyzer:refresh`, `:get-noconnected`, `:apply-to-canvas`), `eda.sch_ManufactureData.getNetlistFile`, `eda.sch_PrimitiveComponent.getAll/getAllPinsByPrimitiveId`, `eda.sch_PrimitivePolygon.create`, `eda.sch_PrimitiveText.create`.
  - `iframe/netlist.html` - views and analysis (not read in detail).
- Implementation observations:
  - Netlist comes from the host's own export (`getNetlistFile`), so net resolution is delegated to EasyEDA; the plugin counts no-connect pins itself by walking pins.
  - Output files via `eda.sys_FileSystem.saveFile` (9 call sites).
  - No tests. No ERC; it is a browsing/analysis tool.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Netlist browser with grouped nets and connection direction | requires adaptation | `iframe/netlist.html`, README.en.md | Desktop panel fed by `eak-engineering` netlist resolution |
  | Connector pin-mapping diagram (auto-pair connectors) | future capability | README.en.md | Useful for harness/connector review; low priority |
  | No-connect count | applicable directly | `src/index.ts` | Feeds ERC unconnected-pin statistics |
  | Draw analysis result onto schematic | not applicable | `src/index.ts` | EAK should not write non-design annotation primitives through the netlist tool |

- Gaps/unknowns: Analysis logic inside the HTML was not read; claims rely on README and API calls.

---

### 32. eext-bom-compare
- URL: https://github.com/easyeda/eext-bom-compare
- Commit inspected: 4489dd9b857e19505a2f5a0dd383342bb77923d6
- Licence: LICENSE = "Apache License Version 2.0"; package.json Apache-2.0; deps `jschardet`, `papaparse`, `xlsx ^0.18.5`; reusable in EAK: yes-with-attribution (concepts only).
- Language / size: TypeScript (iframe app under `iframe/src/{core,ui,utils,locales}`), 77 files, vitest tests (`tests/comparator.test.ts`, `csv-parser.test.ts`) with real BOM samples from JLCPCB, KiCad, Altium, OrCAD, PADS, EasyEDA, with `tests/data/SOURCES.md` recording origins.
- What it is: Compares two BOM files (CSV/TXT/XLS/XLSX; encoding auto-detect UTF-8/GBK/GB2312; header/delimiter auto-detect; column mapping) keyed by designator and shows added/removed/changed/same rows in side-by-side virtual-scrolled tables, with CSV/XLSX export.
- Key source paths:
  - `iframe/src/core/comparator.ts` - `compare(oldFile, newFile)`; fields compared: footprint, quantity, manufacturer, partNumber, value, description (mapped-column intersection).
  - `iframe/src/core/parser/{csv-parser,excel-parser}.ts`, `column-mapper.ts`, `column-config.ts`, `exporter.ts`, `ui/table.ts` (807 lines, virtual scrolling).
- Implementation observations:
  - Pure file-vs-file tool: the only EasyEDA API calls are dialog/iframe/toast; it never reads the design BOM. Algorithm: build designator -> rows map for both files, pair by designator (and by occurrence index when duplicates), emit row diff with per-cell diffs; duplicate-designator warning.
  - Good test fixtures from multiple EDA vendors; useful as a corpus idea for EAK's BOM import.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | BOM diff by designator with column mapping | applicable directly | `iframe/src/core/comparator.ts` | Implement in `eak-manufacturing` for revision-to-revision BOM diff in release gate |
  | Multi-vendor BOM CSV ingestion with encoding detection | requires adaptation | `parser/*`, `tests/data/SOURCES.md` | Rust `csv` + encoding crate; keep sample corpus idea |
  | Virtual-scroll diff table UI | requires adaptation | `ui/table.ts` | React table for desktop |
  | XLSX I/O via `xlsx` package | not applicable | `package.json` | Avoid; CSV export suffices (SheetJS 0.18.5 is an old npm release) |

- Gaps/unknowns: Did not run tests. Grouped-by-value BOM (quantity-per-line) comparison semantics were not checked.

---

### 33. eext-export-gencad
- URL: https://github.com/easyeda/eext-export-gencad
- Commit inspected: aba4dff5b0fb8e1c5ad8288b07eb56b01dd0ab9e
- Licence: LICENSE = "Apache License Version 2.0"; package.json Apache-2.0; reusable in EAK: yes-with-attribution (concepts only). `docs/GenCAD-Specification-v1.4.{md,pdf}` and a sample `.cad` are included; the specification document's own licence is not stated in the repo (check before re-using any text).
- Language / size: TypeScript, 35 files; `src/index.ts` 1,148 lines, `footprintParser.ts`, `footprintExtractor.ts`; dep JSZip.
- What it is: Exports the open PCB as a GenCAD 1.4 `.cad` file in inches.
- Key source paths:
  - `src/index.ts` - `collectBoardData`, `generateGencadContent`, `exportGencad`, `padStackKey`, `parsePadShape`, `layerIdToGencad`, `mil2inch`.
  - `src/footprintParser.ts` (`parseFootprintFile`, elibz2 ZIP + JSON `.elibu` lines), `footprintExtractor.ts`.
  - `docs/GenCAD-Specification-v1.4.md`, `docs/video-gencad-example.cad`.
- Implementation observations:
  - Sections emitted: `$HEADER, $BOARD, $PADS, $PADSTACKS, $TRACKS, $SHAPES, $COMPONENTS, $DEVICES, $SIGNALS, $ROUTES, $LAYERS, $END`. Pad shapes ROUND/RECTANGULAR/OBLONG (oblong as POLYGON with ARC+LINE); native `CIRCLE` and `ARC` records (arcs normalised to CCW by swapping ends for CW); padstack deduplication; footprint data cached; rotation and mirror handled; designator/value TEXT.
  - Footprint geometry is obtained by fetching the footprint file (`eda.sys_FileManager.getFootprintFileByFootprintUuid`) and parsing the host's elibz2 format.
  - Not emitted (no hits in source): `$ARTWORK`, `$MECH`, `$FIDUCIAL`, `$TESTPINS`, zones/pours; so copper pours are likely not exported, which limits test/fixture usefulness. No tests; output validated only by an online viewer per README.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | GenCAD 1.4 writer | future capability | `src/index.ts` | EAK needs Gerber/Excellon first (done); GenCAD only if test-fixture/ICT export is requested; implement from public spec in Rust |
  | Padstack deduplication and arc-native output | requires adaptation | `index.ts` | Same technique for EAK manufacturing exporters |
  | Reading footprint from source-library format | not applicable | `footprintParser.ts` | EAK reads from its own component definitions |

- Gaps/unknowns: Output correctness not tested; zone/pour omission inferred from absence of keywords.

---

### 34. eext-simulation-with-ngspice
- URL: https://github.com/easyeda/eext-simulation-with-ngspice
- Commit inspected: 1ac297f92b79dca01c345de6616634cc18ff85da
- Licence: `LICENSE` = "BSD 3-Clause License, Copyright (c) 2026, chensiyu and contributors" (the text is BSD-3-Clause; the reason GitHub reports NOASSERTION was not determined, possibly the non-standard "Copyright ... chensiyu and contributors" header or the mixed bundle). **But the package is a mixed-licence bundle**:
  - `THIRD_PARTY_NOTICES.md` states plugin code is BSD-3-Clause and bundled components keep their own terms: ngspice 46 WASM + XSPICE code models; Apache ECharts 5.6.0 (Apache-2.0); zrender (BSD-3); Lucide (ISC); JSZip 3.10.1 (MIT or GPL-3.0-or-later); pako (MIT and Zlib); lie, etc. (`licenses/` folder holds 16 licence texts: ECharts, zrender, Lucide, JSZip, pako, lie, readable-stream, safe-buffer, inherits, immediate, setimmediate, string_decoder, util-deprecate, core-util-is, process-nextick-args).
  - `wasm-lib/NGSPICE-COPYING.txt` (also under `iframe/wasm/`): main ngspice licence "Modified BSD", with listed exceptions: `src/maths/KLU` LGPLv2, `tclspice.c` LGPLv2, `frontend/numparam` LGPLv2-or-newer, `src/osdi` MPLv2.0, `src/maths/sparse` MIT-like, `ndev` and `xspice` public domain **except `xspice/icm/table` which is GPLv2-or-newer**. The loader's code-model list includes `table.cm`, so the shipped bundle contains the GPLv2+ table model.
  - `wasm-build/sharedspice-wrapper/{main_stub.c, ngspice_wasm_stubs.c, ngspice_de_cpp.cpp, ngspice_de_h.h}` carry headers "Modified (C) 2026 by EasyEDA & JLC Technology Group ... GNU General Public License ... version 3 or (at your option) any later version" - **so the compiled `ngspice.wasm` contains GPL-3.0-or-later wrapper code** while the repo LICENSE and README claim BSD-3-Clause for the whole. This is an internal inconsistency to flag.
  - `third_party/ngspice-46.tar.gz` and `third_party/emsdk` (Emscripten SDK) are vendored.
  - Reusable in EAK: **do not copy any of it** (mixed GPL/LGPL/MPL); using upstream ngspice (BSD core) is a separate decision to be made on the ngspice project's own licence, and EAK should study this repo for architecture only.
- Language / size: TypeScript, 303 files incl. ~17 MB of bundled WASM (`ngspice.wasm` 6.6 MB, base64 copy 8.8 MB, XSPICE code models 0.5 MB, ngspice.js 1 MB). Clean layered `src/` (app, application/analysis, core, features, infrastructure/ngspice, integrations/eda, presentation, shared; 85 TS files).
- What it is: A waveform/analysis workbench that runs **ngspice 46 compiled to WebAssembly (with XSPICE) inside the extension iframe**, receives a netlist plus probe list from EasyEDA's simulation event, and renders waveforms; plus Monte Carlo, worst-case and logic-analyser views.
- Key source paths:
  - `src/infrastructure/ngspice/wasm-loader.ts` (loader, XSPICE code models installed into `/usr/lib/ngspice`: spice2poly, analog, digital, xtradev, xtraevt, table, tlines), `session.ts` (`NgspiceSession`: one WASM engine instance reused across Monte Carlo/worst-case runs), `engine.ts` (`WasmSpiceEngine`), `raw-result.ts`/`raw-normalize.ts` (JSON snapshot of plots, vectors, measurements, event nodes), `netlist-normalize.ts`.
  - `src/integrations/eda/{messages,netlist-client,simulation-event-adapter,host,plugin-entry,import-policy}.ts` and `protocol/probe.ts` - v3 netlist import message (strict schema, standard/monte-carlo/worst-case payloads).
  - `src/features/{monte-carlo,worst-case,waveform,logic}`, `src/presentation/waveform/*` (ECharts-based viewport, cursors, peak-preserving downsample), `src/shared/{probe*,variation,dc-sweep,netlist}.ts`.
  - `wasm-build/*` (build scripts, sharedspice wrapper), `NGSPICE_WASM_BUILD.md`, `tools/*` (Windows launcher for an optional local engine on port 51115).
- Implementation observations:
  - **Invocation**: WASM, in-iframe (no shared library, no subprocess). The wrapper emulates the `sharedspice` API; a JS bridge returns a JSON raw result (plots, vectors, measurements, diagnostics). A fresh module is created per analysis in `wasm-loader.ts` (comment says env logs are logged once because modules are rebuilt each run) but `NgspiceSession` reuses one engine for repeated sweeps. `timeoutMs` option exists.
  - **Netlist generation and model association are NOT in this repo**: the netlist and probe list arrive from the EasyEDA host through MessageBus events (v3 protocol); the repo only normalises dialects (selectable LTspice/PSpice/HSPICE compatibility via ngspice `ngbehavior`) and AC frequency suffixes (`1M` to `1Meg`). A separate Windows launcher (`jlc-ngspice-launch://start`, port 51115) refers to a local engine service (the other repo 36), but it is not called from `src/` (only tools scripts reference the port).
  - **Analyses**: `.tran`, `.ac`, `.dc`, Monte Carlo (per-sample resampling, mean/std/quantiles/yield with spec limits, histograms), worst-case (sensitivity sweeps and synthesised Worst Low/High corners; an exhaustive corner oracle `evaluateExhaustiveCornerOracle` exists for checking), logic analysis from XSPICE event nodes (0/1/U).
  - **Responsiveness**: runs yield to the event loop via `MessageChannel` between Monte Carlo samples, cooperative abort signal; memory budget thinning for waveform store.
  - **Waveform UI**: legend toggles, cursors (follow/fixed, snap), zoom/pan, peak-preserving downsampling with shared render-point budget, CSV/ZIP export, current probes synthesised from voltage difference (`XAM`).
  - **Tests**: no test files seen under `src/` in the listing (not confirmed with a search for `*.test.*`).
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | In-browser ngspice WASM with XSPICE | future capability | `wasm-loader.ts`, `NGSPICE_WASM_BUILD.md` | EAK is native desktop: link or spawn upstream ngspice (shared library API `ngSpice_Circ/Command`, or `-b` batch with rawfile) rather than WASM; licence of each linked part must be reviewed |
  | EAK-IR to SPICE netlist generator + model association | requires native EAK implementation | (absent here; host-provided) | Generate netlists from EAK-IR component definitions; store SPICE model ref + pin map with provenance; unmodelled parts must produce an explicit "not simulated" limitation |
  | Probe discovery and current-probe synthesis | requires adaptation | `src/shared/probe-*.ts`, `current-probe-traces.ts` | Useful UX concept |
  | Monte Carlo and worst-case on measurements | future capability | `src/features/monte-carlo`, `worst-case` | Natural fit for engineering verification; results must be recomputed by the engine, never AI-asserted |
  | Waveform viewer (ECharts, cursors, downsampling) | requires adaptation | `src/presentation/waveform/*` | Reuse ideas for a React/canvas viewer; downsampling (min-max/peak) matters for large rawfiles |
  | Dialect compatibility switch (LTspice/PSpice/HSPICE) | requires adaptation | `netlist-normalize.ts`, README_EN.md | Needed to ingest vendor SPICE models |

- Gaps/unknowns: No search for tests done; `engine.ts` and the loader were skimmed by comments/signatures; how the wasm wrapper exposes `bg_run`-style calls was not traced; whether `table.cm` is actually compiled in was inferred from the loader list.

---

### 35. eext-simulation-with-ltspice
- URL: https://github.com/easyeda/eext-simulation-with-ltspice
- Commit inspected: 0fb2afa1816481b50846f55c1c6dbacd5912569b
- Licence: LICENSE = "Apache License Version 2.0"; reusable in EAK: n/a (nothing to reuse).
- Language / size: 3 files only: `LICENSE`, `README.md`, `README.en.md` (162 lines). **No source code, no extension.json, no package.json in the clone.**
- What it is: README-only repository for an EasyEDA Pro "LTspice Simulation" extension; the actual implementation is not published here. README.en.md first lines: "EasyEDA Professional Edition Extension: LTspice Simulation".
- Key source paths: `README.en.md`, `README.md`.
- Implementation observations: README content beyond the title was not analysed in depth (162 lines of text; no code). Nothing can be said about how LTspice is invoked (LTspice is a proprietary freeware, not open source, and has no supported library/shared-lib API). Last commit message: "docs: 由AI自动化流程提交，完成README多语言优化" (AI-generated README polish).
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | LTspice-driven simulation | not applicable | `README.en.md` | LTspice is proprietary; EAK should not depend on it; consider only importing LTspice-dialect netlists/models (see 34 dialect handling) |

- Gaps/unknowns: Everything about the actual implementation (invocation, netlist export, `.raw` parsing). I did not read README.en.md beyond its heading lines, so any feature list in it is unverified here.

---

### 36. easyeda-simulation-engine
- URL: https://github.com/easyeda/easyeda-simulation-engine
- Commit inspected: f06a1dd20b60ea37284ab8033269ddf1212f31af
- Licence: LICENSE = "GNU AFFERO GENERAL PUBLIC LICENSE Version 3, 19 November 2007"; `copyright.txt`: files by Santiago Gonzalez (SimulIDE) are AGPL-3.0; code by others is GPL-3.0; source headers say "Modified (C) 2025/2026 by EasyEDA & JLC Technology Group ... GPL v3"; README says ngspice is BSD-3-Clause; reusable in EAK: **NO (AGPL-3.0 + GPL-3.0). Map structure only.**
- Language / size: C++20, CMake, 18,482 files of which ~317 are project `.h/.cpp` under `src/`; the rest is vendored third-party (`external/boost` 15,106 files, `asio` 633, `websocketpp` 94, `nlohmann` 45, `tinyxml`), plus **committed Windows binaries** (`config/ngspice.dll`, `libgcc_s_seh-1.dll`, `libstdc++-6.dll`, `libwinpthread-1.dll`, `libomp140.x86_64.dll`, `external/ngspice/ngspice.lib/.exp`) and ngspice runtime data (`config/spinit`, `config/ngspice`, `config/data` with 33 entries incl. AVR/Arduino XML, `diodes.model`).
- What it is: "LCEDA circuit simulation tool", a dual-engine local simulation server ("EasyEDA Simulation Engine" v1.12.x): (1) NGspice via the `sharedspice.h` shared-library API (`ngSpice_Init`, `ngSpice_Init_Evt`, `ngSpice_Circ`, `ngSpice_Command("run")` in `src/cirSim/src/ngspice_de.cpp`), (2) a modified SimulIDE event-driven simulator core (circuit element classes, matrix solver) with multithreading and real-time interaction. The editor talks to it over WebSocket (websocketpp, port 51115, console banner `LC_PRO_SIM:1.12.3` ... `STATE:READY` ... `PORT:`), matching the launcher in repo 34.
- Key source paths (structure only):
  - `CMakeLists.txt` - C++20, ccache, MSYS2 gcc 15.1 (README), Windows primary (`shlobj.h`, plus a `pwd.h` branch).
  - `src/server/server.cpp`, `src/server/include/{NgSpiceMessageHandler,SimMessageHandler,ServerMessageHandler,IMessageHandler,eventDispatcher,SimMessage}.h`, `src/server/src/{API/validators,memdata,memtable,config}` - WebSocket server, message handlers per engine, business-rule validators, JSON parsing (nlohmann).
  - `src/cirSim/src/{simulator,circmatrix,matrixsolver,ngspice_de}.cpp` + headers in `src/cirSim/include/` (`ngspice_de.h`, `sharedspice.h`, `simulator.h`, `memory/`).
  - `src/cirSim/src/CircuitElm/{elements,e-elements,elmData,properties}` - component library: `elements/components/{passive 16, active 10, logic 27, sources 11, meters 3, micro 4, outputs 14, switches 8, other 3}` .cpp files; `e-elements/{active,passive,outputs}` - electrical behaviour; `elements/subcircuits`, `componentselector`.
  - `src/public/include` - shared utility headers.
- Implementation observations:
  - **Invocation**: NGspice as a native shared library (DLL) driven through the sharedspice API with callbacks; EDA event-driven nodes via `ngSpice_Init_Evt`/`ngGet_Evt_NodeInfo` (names visible in `external/ngspice/ngspice.exp`: ngSpice_Circ, ngSpice_Command, ngSpice_AllVecs, ngGet_Vec_Info, ngSpice_SetBkpt, ngSpice_running, ngSpice_Reset ...). The vendored import library came from an `ngspice-44.2` visualc build per the `.exp` strings (the other repo bundles 46).
  - **Architecture**: client-server with a message-handler abstraction per engine; a validation layer before simulation; MCU/AVR models in config data (SimulIDE heritage).
  - **Not read**: simulator internals (MNA solver, element stamps), as only structure mapping was requested for this AGPL code.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Native ngspice via sharedspice API in a local sim process | requires native EAK implementation | `src/cirSim/src/ngspice_de.cpp` | Same integration style in Rust (`libloading`/FFI or process + rawfile), implemented from the public ngspice docs, not this code |
  | Separate sim server with WebSocket protocol | requires adaptation | `src/server/*` | EAK could run ngspice out-of-process under Tauri with typed IPC and a kill-tree guard (already in `eak-runtime`) |
  | Event-driven mixed-signal / MCU simulation (SimulIDE core) | future capability | `src/cirSim/src/CircuitElm` | Far beyond Phase 1-4; GPL/AGPL, do not reuse |
  | Pre-simulation business-rule validators | requires adaptation | `src/server/src/API/validators` | Pattern: validate the netlist/model completeness before running, report unmodelled parts |

- Gaps/unknowns: Internals deliberately not read (AGPL). Structure counts come from `find`/`ls`. The protocol message schema was not inspected.

---

### 37. eext-export-design-report
- URL: https://github.com/easyeda/eext-export-design-report
- Commit inspected: 31a8cfec95bcae13e981b912c6bc86025062dca0
- Licence: LICENSE = "Apache License Version 2.0"; package.json Apache-2.0; reusable in EAK: yes-with-attribution (concepts only).
- Language / size: JavaScript/TypeScript, 36 files; `src/index.ts` 924 lines; iframes `index.html`, `panel-utilization.html`, `pcb-info.html`. README title: "Export Equal Length Network Lengths" (the repo name says "design report" but the content is a high-speed-net length export).
- What it is: Exports CSV tables of net lengths, net classes, differential pairs, equal-length groups, pad-pair groups, pad coordinates, and basic PCB info (board area, component count, pad count, pin density) in mil and mm.
- Key source paths:
  - `src/index.ts` - `extractPcbInfo`, `extractAllNetDetails`, `extractNetClassLengthDetails`, `extractDifferentialPairsDetails`, `extractEqualLengthNetGroupsDetails`, `extractPadPairGroupsDetails`, `extractSelectPadDetails`, `computeBoardArea`, `openPanelUtilization`.
  - `iframe/panel-utilization.html`, `pcb-info.html` - panel utilisation view.
- Implementation observations:
  - Lengths come from the host (`eda.pcb_Net.getNetLength`); groups from `eda.pcb_Drc.getAllNetClasses / getAllDifferentialPairs / getAllEqualLengthNetGroups / getAllPadPairGroups / getPadPairGroupMinWireLength`.
  - **Board area**: own polygon logic: outline source -> rings (`tessellateArc`, `tessellateBezier`, `tessellateCircle` 72 steps, `stitchChains`, `shoelaceArea`), subtracts cut-outs; returns an `approximate` flag when it cannot compute exactly (honest limitation reporting).
  - Sorting: net class ascending, then length descending. Handles two host versions for pad designators (`getLegacyComponentDesignator` vs `getNewComponentDesignator`).
  - No stack-up, impedance, BOM or Gerber content; no tests.
- Features and classification:

  | Feature | Classification | Evidence (path) | Recommendation for EAK |
  |---|---|---|---|
  | Length report by net / class / diff pair / equal-length group / pad pair | applicable directly | `src/index.ts` | Add to EAK verification/manufacturing reports with exact nm lengths and revision binding |
  | Board area and pin density metrics | requires adaptation | `computeBoardArea`, `extractPcbInfo` | Compute exactly from EAK-IR outline; keep `approximate` flag idea for curves |
  | CSV export with mil/mm | applicable directly | `src/index.ts` | Trivial in Rust |
  | Pad-pair group min wire length | future capability | `extractPadPairGroupsDetails` | Requires pad-pair constraint type in EAK-IR |

- Gaps/unknowns: Not a full design report (no DRC results, BOM, or stack-up), despite the repo name.

---

## Summary: groups D and E
- **No repo in D contains a reusable, permissively licensed router.** The two real routing algorithms are GPL: eext-open-auto-router (PathFinder negotiated congestion + windowed A*, GPL-3.0, derived from OpenAutoRouter) and easyeda-pcb-router (Freerouting maze/shove/rip-up, GPL-3.0). EAK should re-implement the concepts natively: a negotiation loop with history cost on top of the existing 2-layer A*, per-net windows, and a final hard-clearance pass (repos 19, 21).
- **Optional external routers all use Specctra DSN out / SES in** (Freerouting local HTTP API at 37864, DeepPCB cloud) or a KiCad file bridge via Python. EAK can offer an *optional* DSN/SES adapter with an out-of-process, pinned, cancellable job model, but must keep it non-mandatory, keep GPL tools external, and gate any result through typed Commands (repos 20, 21, 22, 23). The KiCad/Python bridge (22) conflicts with EAK rules and its repo has no LICENSE file.
- **Long-running work pattern worth copying**: engine kept pure (no host API), run in a background worker with progress fractions and a cancel token; best-so-far result retained; failed/uncertain results prune rather than short (repos 19, 20, 25). Equal-length uses generator-style cooperative steps for cancel (25).
- **Useful, small, well-bounded algorithms for EAK**: octilinear ring-search pad fanout with time/candidate budgets and real unit tests (26); meander length tuning with adaptive per-crest amplitude (25); schematic-region group packing as placer seed (27); name-based diff-pair detection (24); copper-thieving pattern fill against a clearance index (28). All need a native integer-nm implementation; EAK diff-pair and length-match constraints must exist in EAK-IR before 24/25 are useful.
- **Fab capability tables are real in 29** (material/layer/copper-tiered min width/spacing, via, slot, SMT economy vs standard). EAK should model them as a versioned, provenance-tagged fab profile data file, not code constants, and report un-implemented DFM checks via `not_implemented` (29).
- **Impedance in 30 is a scraped RBF surrogate** with a Python training pipeline (not reproducible: scripts absent) and unclear data provenance; EAK should do analytic microstrip/stripline/coplanar calculators with stated tolerances rather than copy this approach (30).
- **Simulation**: the ngspice plugin (34) runs upstream ngspice 46 as WASM+XSPICE and delivers the best UX ideas (probe handling, Monte Carlo/worst-case, peak-preserving waveform viewer), but its netlist generation and model association live in the EasyEDA host, so EAK must build EAK-IR to SPICE generation and model association itself and flag unmodelled parts as not simulated. The native route in 36 (ngspice DLL via sharedspice API in a local server) is the closer analogue to a desktop EAK.
- **Licence hazards**: repo 34 claims BSD-3-Clause overall, yet bundles LGPL/MPL/GPLv2+ ngspice parts (KLU, numparam, osdi, xspice `table`) and its sharedspice wrapper sources are GPL-3.0-or-later; repo 36 is AGPL-3.0/GPL-3.0; repos 19 and 21 are GPL-3.0; repo 22 has no LICENSE file. Do not copy code from these; 34's LICENSE text itself is BSD-3-Clause (cause of GitHub's NOASSERTION not determined).
- **LTspice repo (35) is README-only** with no source; nothing to learn beyond the product claim. Repo 37 is a length/pad/board-area CSV exporter, not a design report; repo 32 is a pure file-vs-file BOM diff with a useful multi-vendor BOM fixture set; repo 33 is a GenCAD 1.4 writer without pours (all are Apache-2.0, concept-level only).

## Appendix: other repositories in the organisation (not inspected)

The 67 repositories below are outside the directive's list. They were **not cloned or read**; the
columns are GitHub API metadata (`gh api orgs/easyeda/repos`, 2026-11-14) and the licence column is the
licence-file lookup (`gh api repos/easyeda/<repo>/license`). Descriptions are the repositories' own.

| Repository | Licence (API) | Primary language | Archived | Fork | Description |
|---|---|---|---|---|---|
| [circuitjs1-easyeda](https://github.com/easyeda/circuitjs1-easyeda) | GPL-2.0 | Java | false | true | Electronic Circuit Simulator in the Browser |
| [easyeda-api-i18n](https://github.com/easyeda/easyeda-api-i18n) | Apache-2.0 | - | false | false | 嘉立创 EDA 专业版扩展 API 文档多语言翻译 / EasyEDA Pro API Document Multilingual Translation |
| [easyeda-enhanced-schematic-skill](https://github.com/easyeda/easyeda-enhanced-schematic-skill) | no licence file | - | false | false | 嘉立创EDA专业版原理图增强 SKILL，依赖于easyeda-api-skill增强原理图功能。 |
| [easyeda-eprj3-format](https://github.com/easyeda/easyeda-eprj3-format) | no licence file | - | false | false | 嘉立创EDA专业版新离线工程格式eprj3格式说明；Description of the new offline project format eprj3 for EasyEA pro; |
| [easyeda-file-format-v2](https://github.com/easyeda/easyeda-file-format-v2) | no licence file | - | false | false | 嘉立创EDA专业版V2的工程文件格式说明；Description of the project file format for EasyEDA Pro V2 |
| [easyeda-netlist-format](https://github.com/easyeda/easyeda-netlist-format) | no licence file | JavaScript | false | false | 嘉立创EDA专业版原理图网表格式说明；EasyEDA Pro schematic netlist format |
| [easyeda-std-file-format](https://github.com/easyeda/easyeda-std-file-format) | no licence file | - | false | false | 嘉立创EDA标准版文件格式；EasyEDA Std file format |
| [easyeda-std-i18n](https://github.com/easyeda/easyeda-std-i18n) | no licence file | JavaScript | true | false | This is EasyEDA editor and website i18n/tranlastion/locale files, the more language translation contribution will be welcome. -:) |
| [eext-ai-reuse-block-placement](https://github.com/easyeda/eext-ai-reuse-block-placement) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：AI复用模块放置 - 支持根据已有复用模块库数据，通过AI自动帮你放置适合的模块电路并生成原理图。EasyEDA Pro Extension: AI Reusable Module Placement - Supports automatically pl... |
| [eext-api-debug-tool](https://github.com/easyeda/eext-api-debug-tool) | Apache-2.0 | JavaScript | false | false | 嘉立创EDA专业版扩展：扩展API调试工具；EasyEDA Pro extension api debug tool |
| [eext-api-test-tool](https://github.com/easyeda/eext-api-test-tool) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：JLCEDA & EasyEDA 专业版扩展 API 开发和调试工具。EasyEDA Pro extension: JLCEDA & EasyEDA Professional Edition Extended API Development and Deb... |
| [eext-auto-copper-shape](https://github.com/easyeda/eext-auto-copper-shape) | Apache-2.0 | JavaScript | false | false | 嘉立创EDA专业版扩展：根据 PCB 中选中的过孔、焊盘或器件焊盘，自动生成局部铺铜或填充区域。EasyEDA Pro extension:Based on the selected vias, pads, or component pads in the PCB, automa... |
| [eext-batch-data-export](https://github.com/easyeda/eext-batch-data-export) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：用于批量导出工程数据。EasyEDA Pro extension: Used for batch exporting project data |
| [eext-batch-place-components](https://github.com/easyeda/eext-batch-place-components) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：通过导入一个坐标文件批量放置元件在原理图或PCB中。EasyEDA Pro extension:Batch place components in schematics or PCB by importing a coordinate file |
| [eext-board-outline-splitter](https://github.com/easyeda/eext-board-outline-splitter) | Apache-2.0 | JavaScript | false | false | About 嘉立创EDA专业版扩展：识别 PCB 内多个板框,将每个板框及其内部图元分别复制到独立的游离 PCB 文件中 |
| [eext-chat-with-ai-kimi](https://github.com/easyeda/eext-chat-with-ai-kimi) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：Kimi AI助手，支持查询元件和解析网表。EasyEDA Pro extension: Kimi AI helper  to support component query and netlist analysis. |
| [eext-circuitjs1-simulator](https://github.com/easyeda/eext-circuitjs1-simulator) | GPL-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：基于CircuitJS1的仿真工具。EasyEDA Pro Extension:  Support the use of CircuitJS1 simulation in easyeda. |
| [eext-coil-creator](https://github.com/easyeda/eext-coil-creator) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：快速生成适用于 NFC 卡片、PCB 电机和无线充电等项目的 PCB 线圈。EasyEDA Pro Extension: Quickly generate PCB coils for projects such as NFC cards, PCB moto... |
| [eext-color-silkscreen-generator](https://github.com/easyeda/eext-color-silkscreen-generator) | Apache-2.0 | JavaScript | false | false | 嘉立创EDA专业版扩展：基于本地ONNX模型的彩色丝印生成器。EasyEDA Pro Extension: Color Silkscreen Generator Based on Local ONNX Models. |
| [eext-delete-all-components](https://github.com/easyeda/eext-delete-all-components) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：删除全部元件。EasyEDA Pro extension：Delete all components |
| [eext-device-attribute-editor](https://github.com/easyeda/eext-device-attribute-editor) | Apache-2.0 | JavaScript | false | false | 嘉立创EDA专业版扩展：器件属性编辑器 - 用于搜索库中器件并批量编辑其扩展属性。EasyEDA Pro Extension: Device Attribute Editor - Used to search for components in the library and b... |
| [eext-docs-generator](https://github.com/easyeda/eext-docs-generator) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：基于通用LLM模型的项目文档生成扩展。EasyEDA Pro Extension: A project document generation tool for Easy EDA Professional Edition. |
| [eext-dwg-importer](https://github.com/easyeda/eext-dwg-importer) | GPL-3.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：DWG导入器 - 支持导入dwg格式到PCB。EasyEDA Pro extension: DWG importer - support dwg format to PCB |
| [eext-dynamic-fill-region-for-silkscreen](https://github.com/easyeda/eext-dynamic-fill-region-for-silkscreen) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：动态丝印填充 - 在丝印层绘制动态填充区域；EasyEDA Pro Extension: Dynamic Silkscreen Fill Region - Draw dynamic filling areas on the silkscreen layer... |
| [eext-easyeda-api-agent](https://github.com/easyeda/eext-easyeda-api-agent) | Apache-2.0 | JavaScript | false | false | 嘉立创EDA专业版扩展：基于大模型的扩展API助手，通过自然语言或语音交互，用户可以便捷地调用EDA扩展API功能。EasyEDA Pro extension: This large-scale model-based extended API assistant is desi... |
| [eext-excalidraw](https://github.com/easyeda/eext-excalidraw) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：用于制作流程图或自定义丝印的画板工具；EasyEDA Pro extension: A drawing board tool for creating flowcharts or custom silkscreens. |
| [eext-export-design-archive](https://github.com/easyeda/eext-export-design-archive) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：用于批量导出工程和库文件压缩包的工具。EasyEDA Pro extension: Tool for batch exporting compressed packages of projects and library files |
| [eext-export-hyperlynx](https://github.com/easyeda/eext-export-hyperlynx) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：导出HyperLynx - 将 PCB 设计导出为 HyperLynx (.hyp) 文件格式，用于信号完整性仿真分析。EasyEDA Pro Extension: Export HyperLynx - Export PCB design to Hyper... |
| [eext-export-pcb-to-svg](https://github.com/easyeda/eext-export-pcb-to-svg) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：PCB转SVG - 将当前 PCB 文档一键导出为多个按层切分的 SVG 文件。EasyEDA Pro Extension: PCB to SVG - Export the current PCB document as multiple SVG file... |
| [eext-extension-demo](https://github.com/easyeda/eext-extension-demo) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：五个扩展案例，对原理图和PCB都生效. EasyEDA Pro Extension：Five extension cases of EasyEDA Pro, valid for both schematics and PCB |
| [eext-external-tool-integration-demo](https://github.com/easyeda/eext-external-tool-integration-demo) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：外部工具集成演示案例，可以通过唤起外部工具推送文件进行各种交互操作，如果调用第三方仿真工具等。EasyEDA Pro Extension: Demonstration case of external tool integration, where var... |
| [eext-fpc-workshop](https://github.com/easyeda/eext-fpc-workshop) | Apache-2.0 | JavaScript | false | false | 嘉立创EDA专业版扩展：FPC 设计增强工具集，压PAD焊盘变换 · PI补强厚度计算 · 走线载流/线圈阻值/过孔电流快速估算 |
| [eext-generate-schematic-from-netlist](https://github.com/easyeda/eext-generate-schematic-from-netlist) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：通过导入网表生成原理图 EasyEDA Pro extension: Generate schematics by importing netlists |
| [eext-generate-silkscreen](https://github.com/easyeda/eext-generate-silkscreen) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：根据焊盘的网络名生成图片或文本丝印 EasyEDA Pro extension:Generate image or text silk screen based on the net name of the pad |
| [eext-graffiti-silkscreen](https://github.com/easyeda/eext-graffiti-silkscreen) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：涂鸦丝印插件是一款专为用户提供简单易用的彩色丝印设计工具，适用于设计简单的Logo和背景元素。EasyEDA Pro extension: The Tuya Silk Screen Plug-in is a simple and easy-to-use c... |
| [eext-image-contour-to-pcb](https://github.com/easyeda/eext-image-contour-to-pcb) | Apache-2.0 | JavaScript | false | false | 嘉立创EDA专业版扩展：提取图片中所有轮廓，用于绘制艺术PCB的EDA专业版插件。EasyEDA Pro Extension: Extracting all outlines from an image for drawing artistic PCBs. |
| [eext-import-project-library-folder](https://github.com/easyeda/eext-import-project-library-folder) | Apache-2.0 | HTML | false | false | 嘉立创EDA扩展：直接从未解压的文件夹导入嘉立创EDA专业版工程（.epro2）与库（.elibz2）。EasyEDA Pro extension: Directly import EasyEDA Pro Edition projects (.epro2) and librari... |
| [eext-interactive-bom](https://github.com/easyeda/eext-interactive-bom) | Apache-2.0 | JavaScript | false | false | 嘉立创EDA专业版扩展：交互式BOM - 交互式 BOM 查看器，提供实时 PCB 预览与 BOM 联动功能；EasyEDA Pro Extension: Interactive BOM - Interactive BOM Viewer, providing real-time ... |
| [eext-kipida-integration](https://github.com/easyeda/eext-kipida-integration) | Apache-2.0 | Python | false | false | 嘉立创EDA专业版扩展：KiPIDA  - KiPIDA直流（DC）电源完整性（PI）分析；EasyEDA Pro Extension: KiPIDA - Direct Current (DC) Power Integrity (PI) analysis |
| [eext-knowledge-base](https://github.com/easyeda/eext-knowledge-base) | Apache-2.0 | JavaScript | false | false | 嘉立创EDA专业版扩展：基于本地Embedding模型的RAG知识库。EasyEDA Pro Extension:  The local RAG knowledge base based on an embedding model. |
| [eext-line-equal-spacing](https://github.com/easyeda/eext-line-equal-spacing) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：画布两点取区，可选完整走线或独立线段，支持直接调整和几何推挤。EasyEDA Pro extension:The canvas allows for two-point zoning, with options for full routing or in... |
| [eext-mcad-integration-with-freecad](https://github.com/easyeda/eext-mcad-integration-with-freecad) | Apache-2.0 | Python | false | false | 嘉立创EDA专业版扩展：通过WebSocket将PCB 3D模型发到FreeCAD进行查看和编辑。EasyEDA Pro Extension:  Send PCB 3D models to FreeCAD for viewing and editing via WebSocket... |
| [eext-mcad-integration-with-fusion360](https://github.com/easyeda/eext-mcad-integration-with-fusion360) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：通过WebSocket将PCB 3D模型发到Fusion360进行查看和编辑，支持双向交互。EasyEDA Pro Extension:Send the PCB 3D model to Fusion360 via WebSocket for viewing... |
| [eext-mcad-integration-with-solidworks](https://github.com/easyeda/eext-mcad-integration-with-solidworks) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：通过WebSocket将PCB 3D模型发到SolidWorks进行查看和编辑。EasyEDA Pro Extension: Send PCB 3D models to SolidWorks for viewing and editing via WebS... |
| [eext-nc-pad-converter](https://github.com/easyeda/eext-nc-pad-converter) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：将非连接标识的焊盘转换到其他层。EasyEDA Pro Extension: Convert unconnected pads to other layers |
| [eext-note-tools](https://github.com/easyeda/eext-note-tools) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：支持Markdown和LaTeX表达式生成的文档工具。EasyEDA Pro extension: A document tool that supports Markdown and LaTeX expression generation. |
| [eext-paden-integration](https://github.com/easyeda/eext-paden-integration) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：从 EasyEDA 提取 PCB 数据并进行 PADEN 电源分配网络分析。EasyEDA Pro extension：Extract PCB data from EasyEDA and perform PADEN power distribution n... |
| [eext-paden-integration-wasm](https://github.com/easyeda/eext-paden-integration-wasm) | Apache-2.0 | Go | false | false | 嘉立创EDA专业版扩展：Go WASM后端版本，从 EasyEDA 提取 PCB 数据并进行 PADEN 电源分配网络分析。EasyEDA Pro extension：Go wasm backend version, Extract PCB data from EasyEDA a... |
| [eext-pad-expand-helper](https://github.com/easyeda/eext-pad-expand-helper) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：按焊盘生成阻焊外扩，禁止区域（禁止铺铜/填充）或阻焊层图形；支持连续点选与多选。EasyEDA Pro Extension: Generate solder mask extensions based on pads，prohibited areas (p... |
| [eext-pcb-heater-trace-generator](https://github.com/easyeda/eext-pcb-heater-trace-generator) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：根据发热区域、电气目标和工艺参数自动生成 PCB 电热膜蛇形线路。EasyEDA Pro Extension: Automatically generate serpentine circuitry for PCB heating film based o... |
| [eext-pcb-price-calculator](https://github.com/easyeda/eext-pcb-price-calculator) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：PCB价格计算器，可以自定义价格。EasyEDA Pro extension: PCB price calculator |
| [eext-pcb-render-with-blender](https://github.com/easyeda/eext-pcb-render-with-blender) | Apache-2.0 | Python | false | false | 嘉立创EDA专业版扩展：Blender渲染PCB。EasyEDA Pro Extension: Render PCB with Blender |
| [eext-plm-integration-demo](https://github.com/easyeda/eext-plm-integration-demo) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：用于嘉立创EDA专业版的 PLM 对接的参考案例扩展。EasyEDA Pro extension: Reference Case Extension for PLM Intergration with EasyEDA Pro |
| [eext-power-net-highlighter](https://github.com/easyeda/eext-power-net-highlighter) | AGPL-3.0 | HTML | false | false | 嘉立创EDA专业版扩展：一键识别项目原理图中的电源网络并高亮显示。支持内置规则与自定义通配符规则（如 *gnd*、vcc*、+*），基于网络标识符结构与规则双通道检测，并支持电感/磁珠/毫欧电阻电源贯通传播与短接标识符短路并组。 |
| [eext-qrcode-generator](https://github.com/easyeda/eext-qrcode-generator) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：二维码生成器。EasyEDA Pro extension: QR code generator. |
| [eext-rubberband-autorouter](https://github.com/easyeda/eext-rubberband-autorouter) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：橡皮筋布线器。EasyEDA Pro Extension: Rubberband Autorouter |
| [eext-run-api-gateway](https://github.com/easyeda/eext-run-api-gateway) | Apache-2.0 | TypeScript | false | false | 嘉立创EDA专业版扩展：Run API Gateway — 为 AI 编程工具（OpenCode、QwenCode、KimiCode 等）提供 WebSocket API 网关桥接服务的工具。EasyEDA Pro Extension: Run API Gateway — a t... |
| [eext-schematic-pdf-interaction](https://github.com/easyeda/eext-schematic-pdf-interaction) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：原理图PDF交互 - 方便原理图PDF对PCB进行快速元件定位；EasyEDA Pro Extension: Schematic PDF Interaction - Facilitates rapid component positioning from ... |
| [eext-signal-absence-detection](https://github.com/easyeda/eext-signal-absence-detection) | MulanPSL-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：检测当前图页所属板子的全部原理图页，识别单连接/悬空网络（有名网络器件数不足、悬空标签（单一网络名仅覆盖 1 个连通组，无对端）、无器件悬空标签/端口、未命名孤立导线、孤立引脚），沿导线描线高亮并生成检测报告，供设计评审参考。网络按单张原理图作用域判定（检... |
| [eext-simulation-with-simulide](https://github.com/easyeda/eext-simulation-with-simulide) | AGPL-3.0 | C++ | false | false | 嘉立创EDA专业版扩展：原理图SimulIDE仿真；EasyEDA Pro extension: schematic simulation with SimulIDE |
| [eext-timing-analysis](https://github.com/easyeda/eext-timing-analysis) | Apache-2.0 | HTML | false | false | 嘉立创EDA专业版扩展：PCB时序图分析工具，支持路径延迟计算、Setup/Hold时间分析、时序图可视化显示和Timing Budget输出。自动分析两个器件间的信号网络，实时计算时序余量，支持导出SVG/PNG/JPG格式时序图。 |
| [eext-timing-diagram-tool](https://github.com/easyeda/eext-timing-diagram-tool) | Apache-2.0 | JavaScript | false | false | 嘉立创EDA专业版扩展：时序图工具 - 基于 WaveDrom 的时序图绘制工具，支持在原理图中创建和放置专业的数字时序图。 EasyEDA proA timing diagram tool based on WaveDrom, supporting creation and p... |
| [eext-update-components-attributes](https://github.com/easyeda/eext-update-components-attributes) | Apache-2.0 | JavaScript | false | false | 嘉立创EDA专业版扩展：更新原理图元件属性参数。EasyEDA Pro extension：Update components attributes from library |
| [extension-dev-mcp-tools](https://github.com/easyeda/extension-dev-mcp-tools) | no licence file | TypeScript | false | false | 用于AI Agent自动调试扩展的MCP服务。MCP service for automated debugging extension of AI Agent |
| [jlc-mcli](https://github.com/easyeda/jlc-mcli) | MIT | TypeScript | false | false | A Node.js command-framework that serves one command tree through both shell execution and MCP protocol, giving AI agents progressive discove... |
| [kicad-to-easyeda-eprj3](https://github.com/easyeda/kicad-to-easyeda-eprj3) | Apache-2.0 | JavaScript | false | false | 将KiCAD工程文件转换为嘉立创EDA的eprj3工程格式，支持批量操作。Convert KiCAD project files to the eprj3 project format of EasyEDA Pro, supporting batch operations. |
| [spice-model-skill](https://github.com/easyeda/spice-model-skill) | Apache-2.0 | Python | false | false | Using this skill you can create component spice model to do simulation by AI. 通过使用这个skill可以让AI创建元器件的spice模型进行仿真。 |
