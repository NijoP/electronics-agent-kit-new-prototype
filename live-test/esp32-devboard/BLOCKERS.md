# EAK live test — ESP32 dev board — Blocker log

Each entry is written when the problem is hit. Severity: blocks / degrades / cosmetic.

## B-001 — No way to tell which EAK build is under test
- Time: 2026-11-15 18:16 · Phase: start-up
- Tried: `eak --version`, `eak version`
- Expected: version + git commit of the installed CLI. Actual: `eak: unknown command --version` (help printed). The only identification available is the binary mtime/size.
- Severity: degrades (test results cannot be tied to a commit)
- Workaround: recorded mtime/size of `$EAK_CLI` in RUN_LOG.md.
- Suspected cause: `crates/eak-harness/src/main.rs` / cli has no version command.
- Status: open

## B-002 — CLI cannot reach the terminal session's own project (stale lock, dead endpoint record)
- Time: 2026-11-15 18:20 · Phase: start-up
- Tried: `eak info --json` inside the EAK terminal session (EAK_PROJECT_DIR=`C:\test of eak\hi`, EAK_PROJECT_ID=prj_584abdcafa5242c3).
- Expected: route "app" through the running app (pid 32064, which listens on `$EAK_HARNESS_SOCKET` tcp:127.0.0.1:63632), or recovery of a lock left by a dead process.
- Actual: `{"code":"project_locked","message":"project is locked: the previous session was interrupted; recover and open to continue"}`. `.harness.json` names pid 4028 / tcp:127.0.0.1:62156 and `.lock` holds 4028: that process no longer exists. The CLI treats the unreachable record as stale (correct) but then has no way to recover the lock, and never tries the session's `EAK_HARNESS_SOCKET`.
- Severity: blocks (for the session's bound project; an agent cannot work in the project the user opened the terminal for)
- Workaround: created a fresh project `C:\eak-dev\esp32-devboard` (prj_6f642417cc134504) with `eak new` and overrode `EAK_PROJECT_DIR` / `EAK_PROJECT_ID` per call. Side effect: the desktop app does not show the agent's edits live.
- Suspected cause: `crates/eak-harness/src/route.rs:57-91` (record unreachable → direct open → `store.rs:224` refuses a dead-pid lock; no fallback to `EAK_HARNESS_SOCKET`, no `recover` tool in the harness).
- Status: open

## B-003 — No LCSC lookup / part import in the harness
- Time: 2026-11-15 18:24 · Phase: start-up / library
- Tried: `eak tools --json` (106 tools) and a repo search for an LCSC tool.
- Expected: a harness tool equivalent to the desktop Library panel's `parts_search` / `parts_import` (`apps/desktop/src-tauri/src/parts.rs:125,193`).
- Actual: no such tool. LCSC lookup exists only in the desktop app (`eak-manufacturing/src/lcsc.rs`); agents cannot look up stock, price, basic/extended or confirm an LCSC number.
- Severity: degrades (LCSC numbers must be entered from the agent's own knowledge and are not verified by EAK)
- Workaround: LCSC numbers set with `SetComponentProperties` / `UpsertComponentDefinition`, marked as unverified in ASSUMPTIONS.md.
- Status: open

## B-004 — Built-in library lacks every key part of an ESP32 dev board; LCSC import cannot add them
- Time: 2026-11-15 18:25 · Phase: library
- Tried: `eak call list_library_components '{}'` (48 parts).
- Expected: ESP32-WROOM-32E, USB-C 2.0 receptacle, USBLC6-2SC6, CP2102N or CH340C, 1x19 2.54 mm header, SMD tactile switch, AP2112K/ specific LDO, SS14-class Schottky.
- Actual: none present (only ESP32-C3-MINI-1, ATtiny85, generic LDO_SOT-23-5, AMS1117 SOT-223, headers up to 1x10, THT 6x6 switch, generic D_SMA). Even in the app, `lcsc.rs:317 import_plan` refuses a part with no existing template ("no footprint is ever guessed"), so these parts cannot be imported at all.
- Severity: blocks (without the workaround) / degrades
- Workaround: define each part with `UpsertComponentDefinition` (symbol pins, footprint pads, pin map, MPN, LCSC), geometry entered by the agent from the manufacturer's published land patterns. EAK cannot verify these pads against a datasheet → recorded as assumptions.
- Status: open

## B-005 — No differential pairs or impedance control
- Time: 2026-11-15 18:32 · Phase: 1 (design rules)
- Tried: looked for a pair/impedance field in `UpsertNetClass`, `SetDesignRules`, `SetStackup`, and any pair command in `eak tools`.
- Expected: declare USB_DP/USB_DN as a 90 Ω differential pair (width/gap from the stackup, length-match tolerance) and have DRC check it.
- Actual: net classes carry only track_width/clearance/via sizes. STATUS.md "Known gaps": "Differential pairs, impedance control ... not implemented". No calc tool for differential impedance either.
- Severity: degrades (the 90 Ω requirement cannot be entered, routed or verified; USB FS at 12 Mbit/s tolerates this on a short run, but the requirement is unmet)
- Workaround: a `USB` net class with one width, both nets routed side by side by hand with equal lengths; recorded as NOT VERIFIED.
- Status: open

## B-006 — No keepout areas (antenna keepout cannot be entered)
- Time: 2026-11-15 18:33 · Phase: 1 / 4
- Tried: `AddZone` schema (copper pour on a net only), `SetDesignRules`.
- Expected: rule area with "no copper on any layer, no footprints, no vias", as Espressif requires under the WROOM antenna.
- Actual: no keepout / rule-area object exists in EAK-IR or the command set.
- Severity: degrades
- Workaround: put the module's antenna section past the board edge (antenna overhangs the outline), so no board copper can be under it; GND zones drawn so their outline stops before the antenna. Not checked by EAK.
- Status: open

## B-007 — Footprints cannot express slotted or non-plated holes (USB-C shell), silkscreen art or paste control
- Time: 2026-11-15 18:34 · Phase: 2 (library)
- Tried: `UpsertComponentDefinition` footprint schema: pads are smd/tht with rect/circle/oval shape and one round `drill`.
- Expected: oval/slotted plated holes, NPTH holes (USB-C locating pegs, mounting holes), per-pad paste on/off, footprint silkscreen lines.
- Actual: not representable. `crates/eak-core/src/builtin.rs:776` documents it: "no USB-C receptacle; its shell and locating pegs need slotted / non-plated holes that FootprintDef cannot express". Courtyard is a symmetric rectangle about the origin only.
- Severity: degrades (USB-C footprint is an approximation)
- Workaround: USB-C shell slots modelled as round plated holes large enough for the shell legs, locating pegs as plated holes; mounting holes as plated THT pads on GND. Recorded in ASSUMPTIONS.md.
- Suspected cause: `crates/eak-core/src/library.rs:59-80` (PadDef / FootprintDef).
- Status: open

## B-008 — No free silkscreen text (board name, version, header pin labels)
- Time: 2026-11-15 18:35 · Phase: 1 / 4
- Tried: searched the command set for a text/graphic command.
- Expected: add silkscreen text on F/B legend.
- Actual: none. Legend output (`eak-manufacturing/src/gerber.rs:211`) is courtyard outlines, pin-1 dots and refdes only.
- Severity: degrades (spec asks for pin labels, board name and version on silkscreen)
- Workaround: none possible through EAK. Header pin labels recorded in the BOM/README only.
- Status: open

## B-009 — Domain classifier reports "operating environment and temperature" missing although the spec gives it
- Time: 2026-11-15 18:44 · Phase: 1
- Tried: `eak design spec live-test/esp32-devboard/plans/design_spec.json --json` with `conditions.ambient_min_c=0, ambient_max_c=40`.
- Expected: no missing-information entry for temperature.
- Actual: `classification.missing_information: ["operating environment and temperature"]`.
- Severity: cosmetic (misleading prompt; no gate blocked)
- Suspected cause: `crates/eak-intelligence/src/domain/classify.rs:309-321` CHECKLIST keys (`max_ambient_c`, `environment`) are text features extracted from free text; the typed spec's `conditions.ambient_max_c` is not fed into the feature set.
- Status: open

## B-010 — MPN / LCSC number is per library component, not per value
- Time: 2026-11-15 18:52 · Phase: 2
- Tried: planned `SetComponentProperties` on `R_0603` for 10 kΩ, 5.1 kΩ and 1 kΩ resistors.
- Expected: per-value (or per-instance) MPN/LCSC so the BOM gets one correct LCSC number per line.
- Actual: `SetComponentProperties` takes only `component_id`; `eak-manufacturing/src/bom.rs:57-146` groups BOM lines by (component_id, value) but takes `lcsc`/`mpn` from the component definition, so every R_0603 line would carry the same LCSC number whatever its value.
- Severity: degrades (silently wrong BOM if the obvious path is used)
- Workaround: one project component per value (`R_0603_10K`, `R_0603_5K1`, ...) via `UpsertComponentDefinition` with the built-in 0603/0805 land geometry copied from `crates/eak-core/src/builtin.rs:128-135`.
- Status: open

## B-011 — `design_plan` schema omits `intent`, which the design procedure requires
- Time: 2026-11-15 19:05 · Phase: 3
- Tried: `eak tools --json` → `design_plan` input schema = {title, steps, rationale, addresses}, `additionalProperties: false`. `design_procedure` step S8 says "State the intended net of every pin in the plan's intent" (checked_by `design_plan: intent`).
- Expected: `intent` (`{connections:[{net, pins:["U1.2",...]}]}`) in the published schema.
- Actual: field missing from the schema; the CLI accepted it anyway (`intent_pins: 143`) because `crates/eak-harness/src/design.rs:1211` reads `args.get("intent")`. An MCP client that validates against `tools/list` cannot send it, so the intent read-back (S11/S12) silently never happens.
- Severity: degrades
- Workaround: sent `intent` through `eak design plan <file>` (CLI).
- Suspected cause: the `design_plan` tool spec in `crates/eak-harness/src/design.rs` / `tools.rs` lacks the `intent` property.
- Status: open

## B-012 — Derating check applies the ×0.8 stress derating to `supply_voltage_max` (operating limit): every 3.3 V IC fails
- Time: 2026-11-15 19:08 · Phase: 3
- Tried: `eak design validate --json` at rev 4.
- Expected: a recommended-operating supply limit (ESP32 3.0–3.6 V) compared with the rail max (3.4 V) directly → pass.
- Actual: `error derating:U1:supply_voltage_max  U1 supply_voltage_max 3.6 V derated to 2.880 (x0.8) < applied 3.4 from the spec rails`. With the generic `*` voltage derating of 0.8, no 3.3 V part on a 3.3 V rail can ever pass; the check is a guaranteed false error.
- Severity: degrades (wrong engineering result; trains users to ignore derating errors)
- Workaround: none applied (rating kept honest; finding left visible until the fix).
- Suspected cause: `crates/eak-intelligence/src/mental.rs:104-112` lists `supply_voltage_max` among stress ratings; `crates/eak-harness/src/design.rs:1017-1030` derates every listed rating. Related: the current check applies the full rail current to every part touching the rail (fine for series D1, wrong for shunt parts).
- Status: open

## B-013 — No schematic / PCB image export (no screenshots possible from the CLI)
- Time: 2026-11-15 19:10 · Phase: 3
- Tried: searched the harness/manufacturing crates and tool list for SVG/PNG/PDF output.
- Expected: a plot of the schematic and board (SVG/PDF/PNG) for review and for the report.
- Actual: none; only the desktop window can show the design, and the agent cannot see it (also B-002: the app is not attached to this project).
- Severity: degrades (schematic neatness cannot be reviewed by the agent; reviewers need the app)
- Workaround: neatness checked from coordinates in `list_symbols` only.
- Status: open

