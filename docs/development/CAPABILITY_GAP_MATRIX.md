# Capability gap matrix (2026-11-15)

Source: repository evidence (`STATUS.md`, `FEATURE_MATRIX.md`, crate/module layout, ADRs 0020-0029,
`docs/development/NEXT_ACTION.md`), read at the Project Home redesign. No build, test or app run happened
for this document or the changes it describes.

**Rule applied:** nothing is marked *Implemented and verified* here. Earlier statements in `STATUS.md`
of "build/test-verified" refer to runs before the testing freeze; the integrated tree has since been
changed and never rebuilt, so those are carried as *Implemented, not verified (re-run needed)*.
A Rust module or React component existing is not evidence that the capability works.

States: **V** implemented and verified · **U** implemented, not verified · **P** partially implemented ·
**A** architecture only · **M** missing.

| Capability | State | Evidence and gap |
|---|---|---|
| Product Home | U | `ProjectHome.tsx` rebuilt as a launchpad (header, Open/New via native folder dialog, Continue Working, searchable/sortable recent list, typed lock recovery, ADR-0029). Code-reviewed only; never seen in a live Tauri window. The binary the owner launched predates it. |
| Application shell | U | Activity bar, doc tabs, explorer, bottom panel, command palette, workspace registry exist (`shell/`, `CommandPalette.tsx`). Compact identity/project context per directive section 5 not yet done. Pre-freeze jsdom tests only. |
| Schematic editor | U | `SchematicView.tsx`, typed edit commands, grid/align; jsdom + engine tests before the freeze; never exercised in a real window. |
| PCB editor | U | `PcbView.tsx`, tracks, vias, outline, interactive routing helpers (`routeTool.ts`); same evidence level. Manual routing and outline work are landing in parallel. |
| Project architecture | P | Product/board/block profiles and requirements (`eak-intelligence::systems`, ADR-0027) are code-reviewed only; no dedicated Requirements/Architecture workspace in the desktop beyond `intelligence_summary`. |
| Multi-domain engineering | P | Domain registry and constraint graph exist as data and harness `intel_*` tools; domain-specific engines (RF, thermal, power integrity) are not implemented; gates fail closed to UNKNOWN. |
| Component intelligence | P | Component identity vs supplier offers (ADR-0028), roles, placement heuristics. Heuristic, DESK-derived; no thermal/RF/escape analysis. Code-reviewed only. |
| LCSC and JLCPCB | P | LCSC search/import with resilient cache (`parts.rs`), JLCPCB preparation output marked DRAFT. Never checked against the live service or a fabricator. |
| Datasheet retrieval | P | Knowledge import exists (`eak-intelligence::knowledge`, ADR-0024); no automatic datasheet fetch pipeline, PDF/table extraction missing. |
| RAG and evidence provenance | P | BM25 over text with citations and evidence labels; no embeddings, rerank or MPN-exact lookup. Code-reviewed only. |
| 3D component models | U | `viewer3d/` (STEP/WRL/GLB loader, quality states per ADR-0025, read-only board view). Never rendered; context-loss handling unrun. |
| Physical assembly validation | P | MECH checks and courtyard/body-height use exist; no collision/clearance against real 3D bodies, enclosure or assembly constraints. |
| Engineering validation | U | ERC/DRC/MECH, fail-closed gates (ADR-0021) with `not_implemented` and `limitations` listed. Gates and power/units code never compiled. IPC tables unchecked against the standard. |
| Terminal | U | `eak-terminal` real-PTY tests passed on Linux before the override (the one runtime-verified item in `STATUS.md`); later edits and the workspace UI (`terminal/`) unrun. Tabs/splits/launchers are code-reviewed only. |
| Agent integration | U | `eak` CLI, MCP server and app endpoint (`eak-harness`) written; e2e tests never run; no agent trialled through it (`NEXT_ACTION.md` item 4). |
| Agent approval workflow | P | TTY-only human approval for autonomous design (ADR-0020) and AI proposals/rollback commands; no per-call approval UI for agent edits in the desktop. |
| Manufacturing | U | Gerber/drill/BOM/CPL export and JLCPCB preparation exist, labelled DRAFT; never validated by an external viewer or fabricator. Never called manufacturing-ready. |
| Cross-platform installation | P | Install scripts and a v0.1.4 Linux package exist; no install verified on real hardware; Windows/macOS packages unavailable (`docs/PLATFORM_SUPPORT.md`). |
| Adaptive performance | U | ResourceProfile (AUTO/LOW/BALANCED/HIGH/CUSTOM), bounded blocking pool, frame coalescing. Nothing measured (`PERFORMANCE_VERIFICATION_PLAN.md` not performed). |
| Recovery and reliability | U | Atomic writes, journal, crash reconciliation, typed lock state with takeover only for a provably gone holder (ADR-0029). Pending-operation recovery and unknown-outcome blocking pre-date the freeze; lock changes unrun. Stale pid reuse reads as alive (safe side). |

## Largest gaps against "Intent in, manufactured board out"
1. Nothing has been compiled since the freeze: every U row can fail at build time.
2. No agent has driven a real design end to end; autonomous design is harness-only.
3. Existing-project rework (`NEXT_ACTION.md` item 6) not started.
4. Domain engines beyond ERC/DRC/MECH and power are missing; most domains report UNKNOWN.
5. No real-window verification of any UI, including this redesign.
