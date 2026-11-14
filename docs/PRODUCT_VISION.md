# Product vision

EAK is an engineering workstation for electronics. It is not a chatbot with a schematic editor.

Three parts, one design:
1. **EDA workspace** — schematic and PCB editors over one canonical model (EAK-IR); usable by hand with no AI configured.
2. **Terminal workspace** — real shells and split panes in which a user runs the coding agents they already use, with those agents' own authentication.
3. **Engineering harness** — the `eak` CLI and an MCP server that give agents typed, revision-checked engineering tools (`docs/AGENT_HARNESS.md`).

Principle: an agent proposes; EAK's deterministic engines (`eak-engineering`, `eak-validation`,
`eak-manufacturing`) recompute and judge. An agent never certifies its own work, unimplemented
checks are never shown as passing, and nothing is called manufacturing-ready.

Intended flow: intent → requirements → component selection → schematic → PCB → validation →
manufacturing. Today the engines, editors, harness and terminal exist at the maturity listed in
`STATUS.md`; requirements capture, automated selection and the closed loop are planned.

The older assistant-centric direction (chat panel, Ask chips) was removed. Model-provider
settings, typed agent tools and approvals remain as optional infrastructure.
