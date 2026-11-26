# ADR-0055: Schema v8: differential pairs, via spans, target impedance and engineering-check inputs
- **Date:** 2026-11-26
- **Status:** Accepted (approved by the first mate for workstreams A and B). It is one additive
  migration that carries the IR needs of three workers: eak-route-dp (diff pairs, target impedance),
  eak-route-ml (via spans) and eak-verify (engineering-check inputs).
- **Context:** The production-grade engineering directive (2026-11-26) asks for (A) a router that
  routes declared differential pairs at a gap and length tolerance, sized for impedance from the
  stack-up and allowed to use blind/buried spans, and (B) verification engines that report
  UNKNOWN or BLOCKED when an input is missing. Before v8, EAK-IR had no place to declare any of these
  inputs. Pairs were detected by name only (`routing_intel::diff_pairs`), and a check could only guess or give up.
  `Constraints`, `NetClass` and `Command` are shared contracts, so the change is recorded here.

## Decision
New `eak_core::Constraints` fields are all `#[serde(default)]` and omitted from the file when
empty or `None`:

| Field | Type | Meaning when absent |
|---|---|---|
| `diff_pairs` | `Vec<DiffPairRule{positive, negative, width: Option<Nm>, gap: Option<Nm>, target_zdiff_ohm: Option<f64>, zdiff_tolerance_pct: Option<f64>, max_skew: Option<Nm>}>` | no declared pair; each absent figure is undeclared and its check is UNKNOWN |
| `via_spans` | `Vec<ViaSpanRule{from, to: CopperLayer, drill, diameter: Nm}>` | through vias only |
| `ambient_temp_c` | `Option<f64>` | thermal and junction estimates UNKNOWN |
| `via_plating_um` | `Option<f64>` | via current capacity UNKNOWN (no assumed plating) |
| `net_max_drop_v` | `BTreeMap<net, f64>` | voltage-drop limit undeclared |
| `part_dissipation_w` | `BTreeMap<refdes, f64>` | dissipation undeclared |
| `insulation` | `Option<Insulation{standard: ipc2221b \| iec60664_1, pollution_degree: 1..=4, material_group: I \| II \| IIIa \| IIIb, altitude_m, coated}>` | creepage / clearance per a standard UNKNOWN |
| `enclosure` | `Option<Enclosure{max_height_bottom: Option<Nm>, mounting_holes: Vec<MountingHoleSpec{position, drill, tolerance, keepout_radius}>}>` | enclosure checks UNKNOWN (the top-side limit stays `max_component_height`) |

`NetClass.target_z0_ohm: Option<f64>` is the single-ended target impedance of a class.

- **Commands** (all through `Engine::execute`, one undo step each, class `EngineeringImpacting`):
  - `SetDiffPair { rule }` replaces any pair that shares either net, so a net is never in two pairs.
  - `DeleteDiffPair { net }` removes the pair containing `net`. If no pair contains it, it returns "nothing to do".
  - `SetViaSpans { spans }` replaces the list.
  - `SetEngineeringInputs { ambient_temp_c, via_plating_um, net_max_drop_v, part_dissipation_w, insulation, enclosure }`
    replaces the whole set. It is one command, as eak-verify asked, and an omitted field is cleared
    (undeclared). Callers read the current set and send it back with their change.
  - Agent tool schemas exist for all four (`eak-intelligence` tool count 89 → 93). The harness
    counts them as rule or constraint changes for task invalidation and rework.
- **Validation** is in `eak_core::integrity` (the engine runs it after every apply, so an invalid
  value is refused and nothing changes; a file with one fails to load). Pairs need two different,
  non-empty nets, each net in at most one pair, and a positive width, gap, target and tolerance with a
  non-negative skew. Spans need two different layers, a positive drill, a diameter larger than the drill,
  and no duplicate span. Inputs must be finite, with plating and drop limits positive and dissipation
  non-negative. Pollution degree must be 1..=4, the material group one of the four, altitude
  non-negative, and holes must have a positive drill with non-negative tolerance and keep-out.
- **Net rename** (`rename_nets`) carries `diff_pairs` and `net_max_drop_v`. A pair that a rename
  merges into one net is dropped.
- **Migration v7 → v8:** only the version number changes, because absent means undeclared, which is
  exactly the v7 meaning. A v8 file is refused by v7 builds, which would otherwise drop the
  declared rules and report checks against inputs that were silently lost.

## Consequences
- Checks that read these inputs must report UNKNOWN or BLOCKED with the missing input named, and
  never assume a default (for example 25 µm plating) without saying so.
- Limitations: `via_spans` is not cross-checked against the stack-up in integrity, because a layer-count
  change would otherwise refuse to load the board; the router and DRC decide what an unusable span
  means. `part_dissipation_w` is keyed by reference designator and does not follow a re-annotation;
  a stale key names a part that no longer exists. The desktop types and dialogs for these fields are
  not part of this record (the desktop IPC surface is out of scope for workstreams A and B).
