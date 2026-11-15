# ADR-0012: Multilayer boards (2 to 16 copper layers) in EAK-IR
- **Date:** 2026-11-14
- **Context:** Owner directive: the PCB editor must handle 2/4/6/8-layer boards with signal and
  plane layers, per-layer routing, through/blind/buried vias, inner planes with thermal reliefs,
  per-layer DRC and Gerber/drill/stackup export. ADR-0008 deferred this (option 3).
- **Decision:**
  - `CopperLayer` gains `Inner(u8)` (1-based from the top). JSON: `"top"`, `"bottom"`,
    `{"inner": n}`; older files only contain the first two and load unchanged.
  - `StackupLayer.kind` (`signal` | `plane`, default `signal`). `Stackup::validate` accepts an even
    count 2..=16 listed top, In1..In(n-2), bottom; outer layers cannot be planes.
  - `Via.from` / `Via.to` (defaults top and bottom, so older files are through vias). The via kind
    (through, blind, buried) is derived from the span, never stored.
  - Commands: `AddVia` takes optional `from`/`to`; new `SetLayerCount { count, board_thickness }`
    rebuilds a default stackup (outer 1 oz, inner 0.5 oz, first and last inner layers planes).
    `AddTrack`, `AddZone`, `AddVia` and `SetStackup` refuse layers the stackup lacks, and a stackup
    change that would orphan existing copper is refused.
  - No schema bump: every new field has a serde default that reproduces the v2 meaning, so schema
    v2 files written before this change still load identically.
  - DRC: copper-vs-copper checks already compare per layer; DRC-016 (copper on a missing layer or
    invalid via span) and DRC-017 (plane layer without a zone) are new.
  - Manufacturing: one copper Gerber per layer (`Copper,Ln,Top|Inr|Bot`), the through drill file,
    one drill file per blind/buried span, and a `stackup.json`. A Fabricator multilayer stackup check
    (`eak_manufacturing::stackup::fabricator_check`) records every limit as unverified.
- **Consequences:** The autorouter still routes the two outer layers with through vias; inner
  routing and blind/buried vias are placed by the user or AI through commands. Vias connect
  solidly into planes (no per-via relief setting). Impedance is not modelled.
- **Source:** Owner directive 2026-11-14 (outcomes 1 and 6), section B.
