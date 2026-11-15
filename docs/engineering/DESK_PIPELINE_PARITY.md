# DESK pipeline parity (schematic → PCB)

Source: NijoP/DESK at `5ad8492` (MIT, attribution in `third_party/licenses/DESK/`), read from code, not
README. DESK drives EasyEDA Pro through a JS bridge (`desk/bridge.py`) and routes with Freerouting;
EAK re-implements the *method* natively in Rust over the one EAK-IR, through typed `Command`s,
with `eak-validation` as the judge. Nothing is ported as an engine dependency (AGENTS.md).

Markers: **PARITY** (EAK already does it) · **CLOSED** (closed by ADR-0030 in this change) ·
**GAP** (still open) · **N/A** (DESK does not do it in code either, or does it via EasyEDA only).
All EAK behaviour below is code-reviewed only (no build since Phase 1).

## 1. Intent → blocks → netlist
| Step | DESK | EAK | |
|---|---|---|---|
| Intent → blocks | none in code: "block planning is a manual DESK step" (`desk/weft/contracts.py:324-325`); block JSON hand-written (`projects/UPS_1000/design/gen_blocks.py:1-7`) | spec → plan → `design_apply` (`crates/eak-harness/src/design.rs:1178,1259`); the agent writes the `Batch` | N/A |
| Block spec | `{id,parts:[{ref,mpn,pins:{num:NET},role}],ports}` (`desk/spec.py:136-177`) | typed commands `PlaceSymbol`/`LabelPin`/`ConnectPins` (`crates/eak-runtime/src/command.rs:115`) | PARITY (different form) |
| Netlist | spec pins → nets, lint + offline ERC (`desk/spec.py:245-251`) | wires+labels → nets (`crates/eak-engineering/src/netlist.rs:77`) | PARITY |

## 2. Schematic
| Step | DESK | EAK | |
|---|---|---|---|
| Block layout | rows from (100, 3200), wrap at 5200, gap 150 (`tools/build.py:28,48-52`); ICs in a row, passives 2-5 col grid, 100-mil snap (`desk/draw.py:11-63`) | symbol positions chosen by the caller (`PlaceSymbol`); no block-row auto-layout | GAP (cosmetic; no failure in run A/B) |
| Wiring style | every pin: short stub wire + net label, never pin-to-pin wires (`desk/draw.py:20,38-43`) | `LabelPin` puts a label on the pin (`command.rs:115`); run B used 54 labels, 0 wires: same style | PARITY |
| NC pins | `setState_NoConnected` (`desk/draw.py:49-50`) | `SetNoConnect` | PARITY |
| Power source / PWR_FLAG | none: no power symbols or pin overrides; VBUS/GND are plain labels, EasyEDA ERC decides by library pin types (`tools/erc.py:8-19`) | power port label (`LabelKind::Power`, `crates/eak-core/src/ir.rs:114`) satisfies ERC-003 (`crates/eak-validation/src/erc.rs:224`). Root cause of run B ERC-003: the agent did not know it; the ERC-003 fix now names the exact `LabelPin … "kind":"power"` on the connector pin where the rail enters; the guide documents it (`docs/AGENT_HARNESS.md`) | CLOSED |
| Offline ERC extras | GND pins on GND, supply-on-GND error, decap warning, cap voltage vs rail (`desk/erc.py:93-130`) | ERC-012 supply-on-ground, ERC-010 decoupling, cap rating rule (`erc.rs`) | PARITY |

## 3. Board outline
| DESK | EAK | |
|---|---|---|
| explicit `--width/--height` (`tools/pcb_board.py:22-26`); fallback parts bbox + 2 mm (`desk/routing/density.py:673-688`); utilisation > 55 % is an error (`desk/placement.py:164,414-416`) | explicit `SetBoardOutline` (`command.rs:136`); no utilisation gate | GAP (utilisation gate) |

## 4. Placement
| Rule | DESK | EAK | |
|---|---|---|---|
| Mounting holes | corners, 4 mm inset (`desk/placement_intel.py:289`) | `MOUNT_HOLE_INSET_MM = 4.0` (`crates/eak-engineering/src/placement_intel.rs:25`) | PARITY |
| Connectors | bottom edge, cursor from 10 mm, step w+4 mm, rot 0, locked (`desk/placement_intel.py:307-333`); check ≤ 1.0 mm from edge (`desk/placement.py:306-326`) | nearest edge, check ≤ 8 mm (`placement_intel.rs:23`) | GAP (DESK edge rule is tighter) |
| Decoupling | snapped at anchor + w/2 + 1.2 mm; check ≤ 2.0 mm (`desk/placement_intel.py:357-373`, `desk/placement.py:356-365`) | snap + `DECAP_MAX_MM = 2.0` (`placement_intel.rs:21`) | PARITY |
| Crystal caps / ESD | load caps to crystal pin 1, ESD to its connector (`desk/placement_intel.py:209-261`) | same snap roles (`placement_intel.rs`) | PARITY |
| Primary ICs | √N grid in a 10 mm inset (`desk/placement_intel.py:336-354`) | connectivity-first constructive placement (`crates/eak-engineering/src/placer.rs:46`) | PARITY (different heuristic) |
| Regulator / crystal-near-MCU | not in code | not special-cased | N/A |

## 5. Routing
| Step | DESK | EAK | |
|---|---|---|---|
| Engine | Freerouting jar (`desk/routing/orchestrator.py:177-181`) | native A* grid router (`crates/eak-engineering/src/router.rs:288`) | PARITY (no Freerouting, by rule) |
| Net order | GND, connector, critical, control, power, rest (`desk/router.py:126-158`; printed, not passed to Freerouting) | criticality: power 8, ground 7, other 4 (`crates/eak-engineering/src/priority.rs:20`) | PARITY |
| Widths | name-guessed current, IPC-2221 10 °C, cap 120 mil (`desk/router.py:175-193`) | class width raised by declared current, IPC-2221 (`crates/eak-engineering/src/routing_intel.rs:285`) | PARITY |
| Passes / recovery | chunked passes, stop after 2 no-progress chunks; DRC converge loop ×3 (`desk/routing/pipeline.py:353-368`, `tools/drc.py:73-128`) | passes ≤ 3 with stagnation and rip-up (`router.rs:217`) | PARITY |
| Failed nets reported | exit 3 PARTIAL (`pipeline.py:531`) | per-connection reasons in warnings, **lost when nothing routed** (empty change → bare "no change", run A/B E2). Now `precondition` with the reasons; `nothing_to_do` names the still-unrouted nets (`crates/eak-runtime/src/command.rs:1281`) | CLOSED |

## 6. Ground pour and stitching
| Step | DESK | EAK | |
|---|---|---|---|
| Pour layers | GND on layers 1 and 2 (`tools/copper_fill.py:17-19`) | `AddZone` per layer; `StitchZones` refuses a net poured on one layer and says to add the other (`crates/eak-engineering/src/stitch.rs:208`) | CLOSED |
| Thermal relief | THT GND pads: 0.3 mm gap, 4 × 0.35 mm spokes; SMD solid (`desk/copper_fill.py:83-152`) | 4 spokes on every same-net pad (`crates/eak-engineering/src/copper.rs:303`) | PARITY (EAK relieves SMD too) |
| Dead islands | removed if < 2 mm² or touching no GND via/THT pad (`desk/copper_fill.py:83-152`) | removed when touching no copper of the net on the layer (`copper.rs:642`, applied by `fill_zone` `copper.rs:448`); run B had 43 islands | CLOSED (area rule not ported) |
| Orphan SMD pad | ring-search via 0.6-3.0 mm × 24 + pad-to-via track (`tools/stitch_orphans.py:79-90`) | same, with exact clearance and no via-in-pad (`stitch.rs`), as typed `StitchZones` (`command.rs:1403`) | CLOSED |
| Orphan island | one via at pole of inaccessibility (`tools/stitch_orphans.py:55-61`) | via at the deepest cell, only where it lands in connected same-net fill on another layer (`stitch.rs`) | CLOSED |
| 10 mm bottom link track | `stitch_orphans.py:91-96` | not drawn (the refilled pour joins the via) | GAP (deliberate) |
| Regular stitching grid | benchmark only (`desk/copper.py:406-427`) | none | N/A |

## 7. Silkscreen and outputs
| DESK | EAK | |
|---|---|---|
| no silk rules in code | stroke-font legend, mask clip (`crates/eak-engineering/src/silk.rs:127`), DRC-015 | PARITY (EAK does more) |
| Gerber zip, BOM, PnP, IPC-D-356A, netlist, PDF via EasyEDA (`tools/release.py:60-89`) | `crates/eak-manufacturing/src/{gerber,drill,bom,fabpack}.rs`; manifest DRAFT unless gates pass | PARITY |

## 8. Gates
| DESK | EAK | |
|---|---|---|
| release blocked unless ERC 0, DRC 0, spec = board (`tools/release.py:41-82`) | `run_gates` fail-closed (`crates/eak-validation/src/gates.rs:306`), ERC/DRC list `not_implemented` | PARITY |

## Run A/B failures → root cause → fix
| Failure | Root cause | Fix |
|---|---|---|
| ERC-003 VBUS/GND undriven | connector pins are passive; the power port was undiscoverable | ERC-003 fix names the `LabelPin` power-port command; guide flow step 1 |
| DRC-004 GND unrouted | SMD GND pads not reached by a single-layer pour, no escape vias | `StitchZones` escape vias into a second-layer pour |
| 43 pour islands (`copper_continuity` Fail) | dead islands were kept; split islands had no stitching | dead-copper removal + stitching vias |
| AutoRoute `nothing_to_do` ×2 with nets open | the empty change hid the router's reasons | `precondition` with reasons; still-unrouted nets named |
| `eak guide \| head` panic | `println!` on a closed pipe | quiet `BrokenPipe` (`crates/eak-harness/src/cli.rs`) |
| DRC-004 IO4 | a signal connection the router could not fit (board-specific) | now reported with the reason; not otherwise addressed |
