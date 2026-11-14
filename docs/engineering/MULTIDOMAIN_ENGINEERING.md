# Multi-domain engineering model (deliverables A–F)

Code: `crates/eak-intelligence/src/systems/` (ADR-0027) and the harness adapter
`crates/eak-harness/src/intel.rs`. Everything below is **code-reviewed only**: never compiled,
run or tested (test code is written, unrun). Nothing here validates a design. Checks stay in
`eak-validation`, numbers come from `eak-engineering`, and the EAK kernel is the only owner of
engineering state.

## A. Engineering domain taxonomy
- **Engineering domains** (`systems::domains::DOMAINS`, 19 rows, open for extension): power, rf,
  analog, digital_hs, embedded, motor_drive, battery, mains_hv, industrial, automotive,
  audio_video, lighting, thermal, emc, harsh_env, medical, instrumentation, wearable_iot,
  security_access. Each row maps onto DESK domain ids and overlays, carries `high_risk`
  (motor_drive, battery, mains_hv, automotive, harsh_env, medical) and `coverage`: whether a
  discipline mental model exists or the domain is registry only.
- **Archetypes and overlays**: the 100 DESK archetypes and 17 overlays stay in
  `domain::registry` (ADR-0011). An archetype names a DESK domain, and that domain maps to
  engineering domains here. Overlays add domains, for example `switching_power` → power and
  `emi_emc` → emc.
- **Subsystem relationships** (`systems::profile`): the levels are product → subsystem → board →
  functional block → component → PCB region. Each node carries its own domain tags with
  reasons. `effective_domains` adds every ancestor's tags, marked `inherited_from`, so a block
  sees inherited product constraints without receiving another domain's local rules.

## B. Requirement and constraint architecture
- **Requirement** (`systems::model::Requirement`, every §4 field): id, description, category,
  domains, quantity key, `Value` (Unknown / Number / Range{min,nominal,max} / Text),
  `eak_core::units::Unit` and `Tolerance` (one unit model), priority, source (spec path, IR
  requirement, document, person, assumption, derived), evidence, `Knowledge`
  (Unknown < Assumed < Stated < Confirmed < Verified), affects (profile nodes), constraints,
  validation method, approval.
  `Requirement::promote` enforces the rules. An unknown value cannot be raised. An agent can
  only produce *Assumed*. Only a human with evidence reaches *Confirmed*. A deterministic
  check can reach *Verified* only from *Stated* or higher.
- **Requirement compiler** (`systems::compile::compile`): its inputs are the stored design-flow
  spec, mapped field by field in `eak-harness::intel::spec_input` (one spec schema, ADR-0020),
  plus the free-text IR requirements and the DESK classification. Criterion `params` such as
  `switching_hz` come from the agent, so they enter as *Assumed*.
- **Constraint graph**: constraint kinds are hard (a *must* requirement), soft, derived (for
  example a rail current → `trace_capacity_a:<net>` ≥ I) and conditional (for example mains
  creepage, safety, value unknown). Categories follow §5. Every constraint has a scope node,
  so descendants inherit it (`EngineeringModel::applicable`), and provenance: source, rule
  and applicability.
- **Conflicts** (`model::find_conflicts`): two constraints conflict when they bound the same
  quantity over overlapping scopes and their bounds are disjoint in SI units, or their
  dimensions are not comparable. Each conflict lists trade-offs, and EAK never picks one.
- **Precedence** (`model::weakens`): a lower-precedence constraint cannot loosen a safety or
  approved limit, and a limit in another dimension counts as weakening (fail closed).
- **Propagation / traceability** (`model::Dependencies`): edges run
  `req → con → derived con → check`, `req → node → descendants`, `req → calc`, and
  `req → interaction → affected nodes / reviews`. `impact(id)` returns the reachable set and
  every edge walked.
- **Calculation records** (§8): inputs with units, source and knowledge; formula, method and
  formula source; assumptions; margin; result; limit comparison; state (Computed /
  ComputedFromAssumptions / NotComputed); revision. Today the compiler records the buck duty
  cycle at Vin(min) through `eak_engineering::calc::buck_duty_cycle`.
- **Risks** (§12): 11 classes, each carried on the shared `eak_validation::RiskRecord` (ADR-0026):
  severity, likelihood and detectability stay separate axes and are never combined into one
  score. They come from a fixed rule table named in `basis`. High-risk domains and the safety
  class set `approval_required`. Domain checks and lifecycle criteria use the shared
  `CheckState`, so no second outcome or risk type exists.
- **Lifecycle** (`systems::lifecycle`, §7): 13 stages form a phase state machine. A stage is
  entered only when the previous stage exited. Exit criteria come from requirements,
  constraints, the IR and the fail-closed gates (ADR-0021). A criterion with no evidence is
  Unknown and never lets a stage exit, and release review never exits automatically.

## C. Engineering mental-model inventory
The DESK and prototype mapping is in `INTELLIGENCE_INTEGRATION.md`, including the EAK-layer
column, and is not repeated here. The models split two ways:
- **Process models**: DESK M1–M6 (schematic, placement, routing, copper, manufacturing,
  review), in `eak-intelligence::mental`, owned by the design flow.
- **Discipline models**: power, rf, analog, digital_hs, motor_drive and battery, in
  `data/discipline_models.json` loaded by `systems::discipline`. They hold knowledge
  (11 sections), required inputs, procedure, calculations (with the eak-engineering function
  or `null`), constraint keys, verification (EAK check or `null`), standards (metadata only)
  and AI guidance. Each section is tagged with its EAK layer: calculations and verification
  → 1, knowledge, standards, constraints and inputs → 2, procedure → 3, AI guidance → 5. All
  models are DRAFT.
- **Typed domain models** (`systems::domain_models`, Track B): field sets for power (14), rf (13),
  analog (10), digital_hs (10) and embedded (10). Each field has a physical dimension and is
  filled only from a requirement with a known value in a unit of that dimension. Otherwise it
  is Unknown, or UnitMismatch, which fails.

## D. Domain-to-validation matrix
| Domain | Deterministic today (EAK) | Domain check outcomes (`domain_models`) | Not implemented → Unknown |
|---|---|---|---|
| power | ERC, DRC, `net_trace_capacity_a`, buck duty/ripple, ratings, gates (copper continuity, electrical constraints, return path) | `buck_ratio` (when the topology is a buck), `current_paths_sized` | losses, loop stability, thermal |
| rf | ERC, DRC (keepout and clearance) | warnings from the `power_noise_into_rf` and `ground_split_breaks_return_paths` interactions | impedance, antenna keepout, emissions |
| analog | ERC, simulation | warnings from the `heat_near_precision_analog` and `motor_transients_into_sensing` interactions | noise and error budget, ADC settling |
| digital_hs | ERC, DRC, connectivity gates | warning from the `connector_placement_limits_high_speed` interaction | length, skew, impedance, SI |
| embedded | ERC | `debug_access` (declared or Unknown) | pin-mux, boot and reset review |
| motor_drive, battery, mains_hv | as for power, plus ratings | `human_review` = RequiresHumanApproval | protection trip, creepage tables |

Applicability: a domain's checks run only when the domain is present on the product or one
of its nodes. Every domain model also reports `fields_known` (Unknown while any field is
unknown) and `field_units` (Fail on a dimension mismatch).

## E. Cross-domain interaction matrix (`systems::interaction::RULES`)
| Rule | From → to | Triggers (quantity keys) | Re-evaluate |
|---|---|---|---|
| power_noise_into_rf | power, motor_drive → rf | switching_frequency, output/rail current, input voltage | separation, RF ground, RF supply filtering |
| heat_near_precision_analog | power, battery, motor_drive, thermal → analog | currents, ambient | distance to references, drift budget |
| motor_transients_into_sensing | motor_drive → analog, embedded | phase current, bus voltage, switching frequency | Kelvin sense, ADC timing, return separation |
| connector_placement_limits_high_speed | mechanical → digital_hs | board size, connector position, enclosure | length and skew, vias, impedance at the connector |
| battery_heat_in_enclosure | battery → thermal, mechanical | charge current, ambient, enclosure | charger dissipation, enclosure rise |
| ground_split_breaks_return_paths | analog, power, mains_hv → rf, digital_hs | ground strategy, plane split, isolation | plane continuity, stitching |
| isolation_boundary_constrains_layout | mains_hv → mechanical, power, embedded | isolation, working voltage, pollution degree, altitude | creepage and clearance, slots, crossing routes |

A rule is active when both sides are present, and the mechanical and thermal categories are
always present. A triggered rule adds edges into the impact set, and every unimplemented
re-evaluation becomes an Integration risk.

## F. Roadmap (mapped to crates)
| Step | Crate | Acceptance criteria | Risk |
|---|---|---|---|
| Done: domain registry, profiles, requirement model, constraint graph, conflicts, precedence, impact, calc records, risks, discipline models, typed domain models, lifecycle state machine | `eak-intelligence::systems` | unit tests written (unrun); unknown never passes; no stored state | M: unrun code |
| Done: spec → requirement adapter and the `intel_*` harness tools | `eak-harness::intel` | read-only; one spec schema | L |
| Done: `intelligence_summary` read-only IPC for the UI worker | `apps/desktop/src-tauri` | only computed values; revision included | L |
| Next: persist human confirmations and waivers as typed `Command`s | `eak-core` IR + `eak-runtime::Command` (ADR) | promotion only via `Requirement::promote` rules; journaled | H: IR change |
| Next: multi-board products and PCB-region derivation | `eak-core` IR, `systems::profile` | region nodes from zones and keepouts | M |
| Next: more evaluators (impedance, loss, drift budget), each replacing an Unknown | `eak-engineering` | formula source cited; units typed | M |
| Next: per-domain stage applicability in the lifecycle | `systems::lifecycle` | not-applicable stages explained | L |
