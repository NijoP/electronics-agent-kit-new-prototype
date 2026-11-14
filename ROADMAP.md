# Roadmap

No dates. States: DONE (implemented and tested when last run), PARTIAL, CODE-REVIEWED (written, never run), TODO.

| Stage | State | Notes |
|---|---|---|
| Core engineering foundation (EAK-IR, commands, journal, recovery) | DONE | |
| Validation (ERC, DRC, MECH, sourcing) | DONE | more rules and external review TODO |
| Manufacturing outputs (Gerber, drill, BOM, CPL, JLCPCB prep) | PARTIAL | no external viewer or fabricator check |
| Integrated EDA workspace (schematic, PCB, explorer, projects) | PARTIAL | real-window testing TODO; 3D viewer, free-form zones TODO |
| Terminal workspace | CODE-REVIEWED | engine PTY-tested on Linux; UI and Windows/macOS unverified |
| Engineering harness (`eak` CLI, MCP, app endpoint) | CODE-REVIEWED | compile, run, agent trials TODO |
| Agent context wiring (session env, approvals UI, activity view) | TODO | |
| Requirements capture, component selection, autonomous loop | TODO | |
| Cross-platform distribution | PARTIAL | Linux v0.1.4 only; Windows/macOS, signing TODO |
| Ecosystem integrations (more suppliers, simulators) | TODO | |
