# Roadmap

States: DONE (implemented + tested), PARTIAL, TODO. Source: Execution Instructions §13–14.

| ID | Milestone | Depends on | State | Acceptance |
|---|---|---|---|---|
| P0 | Discovery, PDFs, research register, ADRs | - | DONE | docs/ present, traceability mapped |
| P2 | EAK-IR, persistence, migration, recovery | P0 | DONE | round-trip, schema gate, crash tests |
| P3 | Command engine, revisions, undo/redo, journal | P2 | DONE | engine contract tests |
| P5 | Library, schematic connectivity, ERC | P3 | PARTIAL | engine done; editor UI TODO |
| P6 | PCB: outline, place, tracks/vias, zones, router, DRC | P5 | PARTIAL | engine done; editor UI TODO |
| P10 | Manufacturing compiler + release gate | P6 | PARTIAL | outputs + manifest done; external DFM TODO |
| P1 | Tauri 2 + React desktop shell, packaging | P3 | TODO | app launches on Win/Linux/macOS |
| P4 | Workspaces: Home, Schematic, PCB, Library, Monitor, Terminal, Manufacturing, Agent Studio | P1 | TODO | each drives real commands |
| P7 | 3D workspace + cross-probing | P4 | PARTIAL | scene data + MECH checks done; viewport TODO |
| P8 | Intelligence Engine (providers, typed tools, approvals, modes) | P3 | TODO | agent completes bounded task via commands |
| P9 | Research browser, LCSC/JLCPCB integrations | P8 | TODO | optional, provenance-tagged |
| P11 | Hardening, performance, cross-platform verification, review | all | TODO | acceptance report |
