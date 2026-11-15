# EasyEDA EDA interaction audit

Milestone-0 deliverable of the EDA transformation directive (2026-11-16). It is a **focused
interaction audit** of the ten `easyeda/*` repositories the directive names. It reuses, and does not
repeat, the broader [ecosystem audit](EASYEDA_ECOSYSTEM_AUDIT.md) (37 repositories, 2026-11-14),
[feature matrix](EASYEDA_FEATURE_MATRIX.md), [API/format analysis](EASYEDA_API_AND_FORMAT_ANALYSIS.md),
[competitive architecture](EAK_COMPETITIVE_ARCHITECTURE.md) and
[licence audit](LICENCE_AND_ATTRIBUTION_AUDIT.md).

## Method and limits

- Source read through the GitHub API (`gh api repos/easyeda/<repo>/git/trees/<sha>?recursive=1` and
  `contents/<path>` raw reads) on 2026-11-16; scratch copies were kept outside this repository.
  Nothing was built or executed and **no third-party code or constants were copied** into EAK.
- The "Read" column says what was read in this pass. Findings for files not re-read here are taken
  from the ecosystem audit, which names the commit it inspected.
- Everything below is a *pattern* note. EAK keeps one EAK-IR and one typed `Command` boundary.

## Summary table

| Repository | HEAD (2026-11-16) | Licence | Read | Reusable code? | Use for EAK |
|---|---|---|---|---|---|
| easyeda-viewer | `fd69dd1613` | Apache-2.0 | `docs/ARCHITECTURE.md` (sections 5-11), tree, `qa/` | no; re-implement | picking/layer/camera rules, regression strategy |
| easyeda-file-format | `9186c4ac43` | none (unlicensed docs) | tree; ecosystem audit read `en/*` | **no** (facts only) | project/document separation, ids |
| easyeda-eprj3-format | `211ebee8cb` | none stated | tree and example layout | **no** (facts only) | project = folder of per-document files |
| eext-generate-schematic-from-netlist | `35679a0f1e` | Apache-2.0 | `src/index.ts` structure, `rebuildSchematic` | not needed | netlist-to-schematic flow, import diagnostics |
| eext-open-auto-router | `2a308fdd2d` | **GPL-3.0** | README, tree; ecosystem audit | **no** | progress/cancel/evaluate shape only |
| easyeda-pcb-router | `1f230c15ec` | **GPL-3.0** | tree, `AGENTS.md`; ecosystem audit | **no** | DSN/SES boundary, hung-worker handling |
| eext-batch-place-components | `558e253085` | Apache-2.0 | `src/index.ts` structure, CSV placement | not needed | batch placement with per-item failure list |
| eext-gerber-viewer | `085eb879b6` | Apache-2.0 (bundle claims MIT/tracespace) | ecosystem audit | no | layer-type detection, stroke batching |
| eext-design-rule-generator | `2cf9635b33` | Apache-2.0 (calibration data from a web calculator) | ecosystem audit | no (data provenance) | net classification, saved non-active rule set |
| easyeda-api-sdk | `f83525eb54` | Apache-2.0 | ecosystem audit | no | boundary contrast only |

## Per-repository findings

### easyeda-viewer (read-only viewer; Apache-2.0)
- **Files:** `docs/ARCHITECTURE.md`; tree shows `qa/annotations/*.cjs` (browser QA scripts),
  `docs/PRD.md` (tagged decisions), `desktop/*.go` (native shell).
- **Patterns observed**
  1. One flat **object table** (`bbox`, hit callback, layer key, path points, selection polygon,
     pierced-layer set) shared by picking, the object tree, the properties panel and camera fit.
     Tree-to-canvas selection is one lookup, not a second model.
  2. **Pick algorithm**: filter hidden layers, active-layer exclusivity, segment-distance hit test for
     line objects (tolerance scales with zoom, bbox only a coarse filter), line objects beat areas.
  3. **Through-hole objects carry the copper layers they pierce**, so any active copper layer picks them.
  4. **Constant-pixel annotations** (text, selection dashes) rescaled in the camera callback.
  5. **Units** are display-only (mm/mil), storage and canvas never change.
  6. **Regression strategy**: parser unit tests, a smoke test rendering every sample document, pixel diff
     against official exports, interaction QA through a window hook.
  7. Schematic page switch avoids the loading mask and diffs the object list by row signature.
- **Applies to EAK:** selection from tree/canvas through one object index; a revision refresh must not
  reset camera or selection (the viewer's page-switch behaviour). EAK has `pcbFocus.ts`,
  `viewport.ts`, `shell/viewState.ts`.
- **Cannot reuse:** it is a viewer over exported files, with no edit semantics or command boundary.
- **EAK equivalent:** per-document camera and selection kept across a revision event (ADR-0035).

### easyeda-file-format / easyeda-eprj3-format (no licence)
- **Files:** trees. The example project is a descriptor, one folder per schematic with per-page files
  plus config/variable files, one board file, panel files and simulation documents.
- **Patterns:** project/document separation; documents addressed by id with parent pointers;
  incremental changes as records with eventual consistency.
- **Applies:** confirms EAK's folder layout and the need for stable document ids in revision events.
- **Cannot reuse:** no licence, so facts only. EAK-IR is not replaced and no importer is built here.

### eext-generate-schematic-from-netlist (Apache-2.0)
- **Files:** `src/index.ts`: `parseNetlistData`, `rebuildSchematic`, `findDeviceInfo`, `placeComponent`,
  `createNetWiresForSingleComponent`, `createNetWiresForComponents`.
- **Patterns:** component lookup by library search; fixed-pitch grid layout with row wrap; placement
  followed by **per-net labels or short wires** rather than routed wires; progress per component; a
  **not-found list** shown at the end as the import diagnostic.
- **Applies:** Milestone 2 uses EAK's own placer and wirer and reports unresolved parts in the sync
  report (`SyncReportDialog.tsx`). Net-label-first wiring is a sound fallback when routed wires cross.
- **Cannot reuse:** it calls the closed host `eda.*` API.

### eext-batch-place-components (Apache-2.0)
- **Files:** `src/index.ts`: library resolution, CSV parsing with unit conversion, footprint placement loop.
- **Patterns:** each failed item is recorded (not found, name mismatch) and reported together instead
  of aborting the batch.
- **Applies:** EAK batch results report per-item failures and label partial success as partial, never
  as success (task states `Partial`/`Blocked` in ADR-0035).

### eext-open-auto-router / easyeda-pcb-router (GPL-3.0)
- **Files:** trees, README, `AGENTS.md`; algorithm detail from the ecosystem audit (negotiated
  congestion with A*, island placement; Freerouting-derived maze search behind a local service).
- **Patterns:** background worker with progress and cancel; an evaluate pass (open nets, clearance,
  vias, length) after routing; a service boundary exchanging Specctra DSN/SES that never silently
  falls back between engines; hung-worker handling.
- **Applies:** progress, cancel and evaluate-after-route map to the task phases `Routing` and `RunningDrc`.
- **Licence rule:** GPL-3.0 sources are not copied or ported; EAK's router stays its own implementation.

### eext-gerber-viewer (Apache-2.0; bundled HTML states MIT/tracespace)
- **Patterns:** filename and file-function based layer typing, drill-offset voting, stroke batching by
  width into chunked paths, no per-object picking. Relevant to the later manufacturing preview.
- **Licence:** inconsistent notices, so no code is taken.

### eext-design-rule-generator (Apache-2.0, data provenance caveat)
- **Patterns:** classify nets (ground/power/impedance/differential/signal), build a rule plan, save it as
  a **new named rule set that is not activated**, with snapshot and restore. Applies to a future EAK
  rule-suggestion command that stays inactive until applied. Its impedance fixtures derive from a
  third-party calculator, so they are not used.

### easyeda-api-sdk (Apache-2.0)
- Extension scaffolding for the closed editor. Contrast only: its mutation API is per-primitive with no
  undo and no typed command envelope; EAK keeps `Engine::execute`.

## What EAK takes
1. One object index for picking, tree selection, properties and camera fit.
2. Revision refresh that preserves camera and selection.
3. Per-item diagnostics for batch operations; partial results labelled partial.
4. Background routing with progress, cancel and an evaluation pass; no silent engine fallback.
5. New rule sets saved inactive.
6. Regression strategy: parser unit tests, render smoke over fixtures, interaction QA through a hook.

## Licence summary
GPL-3.0 (open-auto-router, pcb-router): concepts only. No licence (file-format, eprj3-format): facts
only. Apache-2.0 repositories: nothing copied; any future copy needs a notice under
`third_party/licenses/`.
