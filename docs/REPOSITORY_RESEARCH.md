# Reference repository research register

Inspection date: 2026-11-13. Each repository was verified via the GitHub API (default branch, HEAD,
licence) and shallow-cloned read-only into a scratch directory. No reference code was executed or
copied into EAK. **Depth is stated per repository**; README-only reviews are labelled as such.

Classification: REUSE DATA/RULE · REWRITE IN RUST · ADAPT CONCEPT · DEVELOPMENT-ONLY RESEARCH · REJECT/DEFER.

| Repository | Branch @ commit | Licence (from API / LICENSE file) | Depth | EAK action |
|---|---|---|---|---|
| [NijoP/DESK](https://github.com/NijoP/DESK) | main @ `5ad8492c37a5` | MIT | Source: `desk/ipc.py`, `desk/erc.py`, `desk/manufacturing/jlcpcb/capabilities.py`, `desk/drc_engine.py` (header), `library/pcb_domains/rule_sets/core.json`, `library/devices.json`, `tests/test_ipc_models.py` | REWRITE IN RUST (sizing rules, ERC rules) + TEST VECTORS; EasyEDA parts REJECT |
| [storytold/craft-anything](https://github.com/storytold/craft-anything) | main @ `52f1daa66802` | Apache-2.0 (repo also ships LICENSE-MIT) | Crate map; `crates/evidence/src/{lib,unknown}.rs` | ADAPT CONCEPT (evidence + limitations + coverage, optimistic revisions) |
| [kunchenguid/firstmate](https://github.com/kunchenguid/firstmate) | main @ `427056c4181c` | MIT | Local installation `AGENTS.md`, session-start behaviour observed | ADAPT CONCEPT (one lead, bounded delegation); REJECT crew model inside EAK |
| [kunchenguid/backpass](https://github.com/kunchenguid/backpass) | main @ `c8d60c24d4dc` | MIT | Repository layout only | DEFER (evidence-backed, human-approved instruction improvement — Phase 8) |
| [earendil-works/pi](https://github.com/earendil-works/pi) | main @ `275dde410065` | MIT | `packages/agent/src/agent-loop.ts` (loop, abort signal, tool-call execution, truncated-message handling) | ADAPT CONCEPT (small agent loop, cancellation, typed tools) — Rust re-implementation; no Node runtime |
| [pyagent-core/pyagent](https://github.com/pyagent-core/pyagent) | main @ `67111f7e70ca` | MIT | Layout only. **Identity unconfirmed** as the intended PyAgent | DEFER until identity confirmed; Python never enters EAK |
| [SoloJiang/weft](https://github.com/SoloJiang/weft) | main @ `12303b30e3cc` | **No licence file / API 404** | Crate list (`weft-brief`, `weft-bus`, `weft-scheduler`, `weft-slug`, `weft-worktree`) | Concepts only; **no code reuse** (no licence) |
| [WeaveMindAI/weft](https://github.com/WeaveMindAI/weft) | main @ `61fdacd44b69` | NOASSERTION | Metadata only | Different project ("a programming language for AI orchestrations"); not a substitute — REJECT |
| [herdrdev/herdr](https://github.com/herdrdev/herdr) | master @ `ee5bfb5041bb` | Apache-2.0 | Layout; files handling process kill identified (`src/app/api/panes.rs`, `src/cli/agent.rs`) | ADAPT CONCEPT → `eak-runtime/src/process.rs` (timeouts, tree kill, bounded logs) |
| [Abil-Shrestha/tracer](https://github.com/Abil-Shrestha/tracer) | main @ `aa09d018f6a8` | MIT | `src/types.rs` status model | ADAPT CONCEPT → `eak-runtime/src/tasks.rs` (ready queue, claims) |
| [lightpanda-io/browser](https://github.com/lightpanda-io/browser) | main @ `97e034d34d0f` | **AGPL-3.0** | Metadata; Execution Instructions §4.1 notes no native Windows binary | DEFER: optional external process only, never bundled, pending legal review |
| [morluto/rea](https://github.com/morluto/rea) | main @ `5cddc69c51a0` | MIT | Layout only | DEVELOPMENT-ONLY RESEARCH; never shipped |
| [unstablebuild/rune](https://github.com/unstablebuild/rune) | main @ `3fe56b3ff15e` | **GPL-3.0** | Not inspected beyond metadata | Visual/interaction study only; **no code** |
| [DietrichGebert/ponytail](https://github.com/DietrichGebert/ponytail) | main @ `96608e078b2d` | MIT | Layout | Methodology only → AGENTS.md working loop |
| [rust-lang/rust](https://github.com/rust-lang/rust) | main @ `75bb7a374ff3` | Apache-2.0/MIT | Reference only | Stable toolchain; no compiler work |
| Cursor browser docs | documentation | proprietary docs | Not yet read in this session | Pending (Agent Studio / research browser design) |

## DESK findings (detail)
- **Trace ampacity** (`desk/ipc.py`): IPC-2221 power law, k = 0.048 external / 0.024 internal,
  exponents 0.44 / 0.725, area in mil². Unknown current raises an error. → `eak-engineering/src/ipc.rs`
  with DESK's test vectors (1 A → 11.9 mil, 3 A → 54.1 mil, 10 A → 284.9 mil, 1 A internal → 30.9 mil).
- **Clearance** IPC-2221B Table 6-1 B1–B4, A5–A7 with a per-volt increment above 500 V → ported.
- **Creepage** IEC 60664-1 Table F.4 (PD1, PD2 groups I/II/III) → ported **with a flag**: the PD2
  rows jump from 0.04 mm (≤ 40 V) to 0.60/0.85/1.20 mm at 50 V. This looks like printed-wiring and
  "other material" columns were merged, so it needs engineering review against the standard text.
- **Via current**: DESK's module header says "external k", but its code uses k = 0.024 (internal).
  EAK follows the conservative code behaviour and documents the inconsistency.
- **ERC** (`desk/erc.py`): ground/supply pin misconnection, missing decoupling cap, capacitor
  voltage rating < rail (error) / < 1.5× (warning), non-numeric resistor values → ERC-010…013.
- **JLCPCB capabilities** (`capabilities.py`): 3.5 mil track/space, 0.2 mm drill. Not adopted as
  defaults (EAK defaults are more conservative: 0.15 mm / 0.3 mm); candidate for a fab profile once
  verified against JLCPCB's current published capabilities.
- `library/devices.json`: EasyEDA UUIDs → REJECT (EasyEDA-specific). LCSC codes could migrate with provenance.
- `drc_engine.py`: parses EasyEDA live DRC → REJECT (EasyEDA-specific). Its acceptance targets
  (0 unconnected pads, orphan vias, shorts, clearance) map onto EAK DRC-003/004/012.

## Unresolved questions
- PyAgent: which repository did the product owner intend?
- Creepage table values: need confirmation against purchased IEC 60664-1 text.
- Lightpanda: distribution model and Windows availability need legal and platform review.
