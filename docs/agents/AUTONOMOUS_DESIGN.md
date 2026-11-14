# Skill: autonomous design through the EAK harness

> Status: **code-reviewed only.** The flow tools (ADR-0020) have never been compiled or run, and
> the worked example below has **not** been executed. Treat every output shape here as the
> intended contract, not observed behaviour.

You (the external agent) do the reasoning. EAK keeps the design (one EAK-IR), executes typed
commands, recomputes the numbers, labels the evidence and refuses to self-certify. Read
`docs/AGENT_HARNESS.md` first (project context, revisions, error codes).

## The loop

| Step | CLI | MCP / `eak call` tool | Effect |
|---|---|---|---|
| 0 | `eak design schema` | `design_spec_schema` | spec schema, evaluable metrics, labels |
| 1 | `eak design spec spec.json` | `design_spec_set` | stores the spec (structural errors refuse it) |
| 2 | read tools (`list_library_components`, `get_component`, `list_symbols`, ...) | | understand parts and state |
| 3 | `eak design plan plan.json` | `design_plan` | decodes every step; nothing runs; returns `plan_id` + `base_revision` |
| 4 | `eak design apply <PLAN_ID> <REV>` | `design_apply` | one `Engine::execute` (Batch): all or nothing, one undo step |
| 5 | `eak design validate` | `design_validate` | ERC/DRC/MECH/DFM + spec evaluation; findings with `suggested_fix` |
| 6 | back to 3 with a plan whose `addresses` lists the finding ids it fixes | | iterate |
| 7 | `eak design claim claim.json` | `design_claim` | state a claim + basis; EAK labels it |
| 8 | `eak design report` | `design_report` | labelled evidence report + fingerprint |
| 9 | **human only**: `eak design approve <REV> <FP>` | (no tool) | interactive terminal + typed phrase |

`eak design log` (`design_iteration_log`) shows every spec/plan/apply/validate/claim event with
revision, session and findings: that is the iteration record.

Rules:
* Never write project files. The only design write is `design_apply` (or the single-command design
  tools); both go through `Engine::execute`.
* Plan against the revision you read. A stale plan is refused with `stale_revision`: re-read,
  re-plan.
* Ids created by a plan (symbols, footprints) are only known after it is applied: read them with
  `list_symbols` and use them in the next plan.
* Labels come from EAK. Do not write "verified" yourself; report the label `design_report` gives.
* Never say "manufacturing-ready". The best label EAK gives is "human-approved release candidate".
* Missing critical inputs (current, voltage, a part's rating) stay missing: put them in the spec
  `assumptions` or leave the criterion `unverified`; do not invent values.

## Evidence labels

| Label | Meaning |
|---|---|
| `verified` | recomputed by `eak-engineering` from design data + spec, within bounds |
| `passed_deterministic_checks` | implemented ERC/DRC/MECH/DFM rules report no errors (not a proof) |
| `simulation_supported` | a successful `run_simulation` is logged at this revision |
| `datasheet_supported` | a library rating marked verified, with its datasheet source |
| `assumed` | rests on agent-supplied inputs or stated assumptions |
| `unverified` | failed, not evaluable, or no evidence |

## Worked example (not run): 12-24 V to 5 V / 3 A buck

Parts are the built-in buck-board set in `crates/eak-core/src/builtin.rs` (TI LMR33630
`LMR33630_HSOIC-8`, Bourns SRP1038A inductor, Phoenix 5.08 mm terminals `TERM_1x02_P5.08`).
Confirm ids with `list_library_components` before planning.

### 1. Spec (`spec.json`)

```json
{
  "title": "12-24 V to 5 V 3 A buck",
  "design_class": "buck_converter",
  "rails": [
    {"net": "VIN", "role": "input",  "v_min": 12, "v_nom": 24, "v_max": 24, "i_max_a": 1.5},
    {"net": "+5V", "role": "output", "v_min": 4.9, "v_nom": 5.0, "v_max": 5.1, "i_max_a": 3.0, "ripple_mv_pp": 50},
    {"net": "GND", "role": "internal", "v_min": 0, "v_nom": 0, "v_max": 0, "i_max_a": 3.0}
  ],
  "conditions": {"ambient_min_c": 0, "ambient_max_c": 50},
  "interfaces": [{"name": "J1", "kind": "screw_terminal_input", "nets": ["VIN", "GND"]},
                 {"name": "J2", "kind": "screw_terminal_output", "nets": ["+5V", "GND"]}],
  "protection": [{"kind": "reverse_polarity", "nets": ["VIN"], "notes": "not in this revision: assumption"}],
  "acceptance": [
    {"id": "erc", "description": "ERC clean", "metric": "erc_errors", "max": 0},
    {"id": "drc", "description": "DRC clean", "metric": "drc_errors", "max": 0},
    {"id": "duty", "description": "duty within LMR33630 range", "metric": "buck_duty_cycle", "max": 0.9},
    {"id": "ripple", "description": "inductor ripple <= 40% of 3 A", "metric": "buck_inductor_ripple_a",
     "refdes": "L1", "max": 1.2, "unit": "A", "params": {"switching_hz": 400000}},
    {"id": "out-trace", "description": "+5V copper carries 3 A", "metric": "net_trace_capacity_a", "net": "+5V", "min": 3.0, "unit": "A"},
    {"id": "isat", "description": "inductor saturation above load", "metric": "component_rating",
     "component_id": "L_SRP1038A", "rating": "saturation_current", "min": 3.0, "unit": "A"}
  ],
  "assumptions": ["no reverse-polarity protection", "400 kHz LMR33630 variant"]
}
```

### 2. Plan 1: parts (`plan1.json`)

```json
{"title": "place buck parts", "rationale": "spec rails VIN/+5V",
 "steps": [
  {"type": "ImportLibraryComponent", "component_id": "LMR33630_HSOIC-8"},
  {"type": "ImportLibraryComponent", "component_id": "L_SRP1038A"},
  {"type": "ImportLibraryComponent", "component_id": "TERM_1x02_P5.08"},
  {"type": "PlaceSymbol", "component_id": "LMR33630_HSOIC-8", "position": {"x": 50000000, "y": 40000000}, "refdes": "U1"},
  {"type": "PlaceSymbol", "component_id": "L_SRP1038A", "position": {"x": 80000000, "y": 40000000}, "refdes": "L1", "value": "10u"},
  {"type": "PlaceSymbol", "component_id": "TERM_1x02_P5.08", "position": {"x": 10000000, "y": 40000000}, "refdes": "J1"},
  {"type": "PlaceSymbol", "component_id": "TERM_1x02_P5.08", "position": {"x": 120000000, "y": 40000000}, "refdes": "J2"},
  {"type": "SetNetCurrent", "net": "+5V", "amps": 3.0},
  {"type": "SetNetVoltage", "net": "VIN", "volts": 24.0}
 ]}
```

`eak info` → revision R; `eak design plan plan1.json` → `plan-0001`; `eak design apply plan-0001 R`.
Add the input/output capacitors and FB divider the same way (values from the LMR33630 datasheet
equations; record them as `design_claim` with `basis: "datasheet"` where a library rating exists,
otherwise `assumption`).

### 3. Plan 2: connectivity

`eak call list_symbols` → symbol ids for U1/L1/J1/J2. Then a plan of `LabelPin` steps naming the
nets the spec uses (`VIN` on U1 pin 2 and J1 pin 1, `SW` on U1 pin 8 and L1 pin 1, `+5V` on L1
pin 2 and J2 pin 1, `GND` on U1 pins 1 and 9 and the terminals' pin 2, the FB divider to U1 pin 5).
Never guess pin numbers: read them from `get_component` (U1 pins: 1 PGND, 2 VIN, 3 EN, 4 PG,
5 FB, 6 VCC, 7 BOOT, 8 SW, 9 AGND).

### 4. Validate and iterate

`eak design validate` → expect findings such as `rail-current-undeclared:VIN`,
`rail-unrouted:+5V` and ERC errors for unconnected pins (EN, BOOT, VCC). Each has a
`suggested_fix`; write the next plan with `"addresses": ["rail-current-undeclared:VIN", ...]`.
Continue with `SyncSchematicToPcb`, `SetBoardOutline`, `AutoPlace` / `MoveFootprint`, `AddTrack`
(or `AutoRoute`), `AddZone`, re-validating after each apply until `errors` is 0.

### 5. Claims, report, approval

```json
{"statement": "L1 does not saturate at the LMR33630 current limit", "basis": "datasheet",
 "component_id": "L_SRP1038A", "rating": "saturation_current"}
```

`eak design report` → each criterion and claim labelled (expected here: `duty` and `ripple`
`verified` because L comes from the schematic value; `erc`/`drc` `passed_deterministic_checks`
when clean; `isat` `datasheet_supported` only if the library marks the rating verified; protection
`assumed`). Hand the human the `approve_with` line; you cannot approve.
