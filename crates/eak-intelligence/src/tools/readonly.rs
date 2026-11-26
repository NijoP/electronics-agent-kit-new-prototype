//! Read-only tools (Spec §6.1 "Read-only: inspect netlist, run report"): design inspection,
//! verification reports and deterministic calculators. None of them mutates the project.
//! Engineering numbers come from `eak-engineering`, never from the model (Execution
//! Instructions §9.1); reports come from `eak-validation` and carry their own
//! `not_implemented` and `limitations` lists.

use super::schema::*;
use crate::message::ToolSpec;
use eak_core::Project;
use eak_engineering::{calc, ipc, netlist};
use serde_json::{json, Value};

pub struct ReadTool {
    pub spec: ToolSpec,
    /// Whether the result reflects the current project revision.
    pub reads_project: bool,
}

fn tool(name: &str, description: &str, input_schema: Value, reads_project: bool) -> ReadTool {
    ReadTool {
        spec: ToolSpec {
            name: name.into(),
            description: format!("{description} [read-only]"),
            input_schema,
        },
        reads_project,
    }
}

fn num_or_value(desc: &str) -> Value {
    json!({"type": ["number", "string"], "description": format!("{desc}; a number or engineering notation such as 4k7, 100n")})
}

pub fn read_tools() -> Vec<ReadTool> {
    vec![
        tool(
            "get_design_summary",
            "Current project revision, sheets, symbols, footprints, board outline, stackup, constraints and routing counts, with object ids.",
            obj(&[], &[]),
            true,
        ),
        tool(
            "classify_domain",
            "Classify the current design against the 100 DESK domain classes (deterministic, no model). Returns ranked classes with reasons, DESK maturity labels and ceilings, overlays, missing information and the list of checks not implemented. A classification is a hypothesis, never a certification.",
            obj(&[], &[]),
            true,
        ),
        tool(
            "analyze_placement",
            "Placement intelligence (DESK-derived mental models, deterministic): infers each footprint's role, finds decoupling/crystal/ESD snaps with measured pad distances, functional clusters per IC, total and longest airwire length (MST), and findings (decap > 2 mm from its IC pad, connector far from the board edge, heat-generating parts too close). Lists what it does not evaluate; never a certification. AutoPlace applies the same rules.",
            obj(&[], &[]),
            true,
        ),
        tool(
            "analyze_routing",
            "Routing intelligence (DESK-derived, deterministic, nothing is placed): unrouted connections in routing-priority order, differential pairs found by name, per-net track width and via chosen from net class and declared current (IPC-2221) with reasons, and by default a dry-run of the native router (passes, rip-ups, stagnation, congestion hotspots, failures with causes). Heuristic; never a certification: call verify_routing after AutoRoute.",
            obj(
                &[(
                    "preview",
                    json!({"type": "boolean", "description": "dry-run the router (default true); false returns only the analysis"}),
                )],
                &[],
            ),
            true,
        ),
        tool(
            "verify_routing",
            "Independent check of the routed board with eak-validation: physical connectivity (unrouted connections, shorts) plus DRC, with what is NOT verified (diff-pair coupling, length matching, impedance, thermal sizing) listed. 'No errors' never means manufacturing-ready.",
            obj(&[], &[]),
            true,
        ),
        tool(
            "list_library_components",
            "Components available for placement (built-in and project library) with their pins.",
            obj(&[], &[]),
            true,
        ),
        tool(
            "get_netlist",
            "Resolved schematic nets (pins per net, power ports, label conflicts, dangling wires).",
            obj(&[("net", nullable(string("only this net")))], &[]),
            true,
        ),
        tool(
            "run_erc",
            "Run the electrical rules check on the current revision. The report lists checks not implemented; no errors does not prove the design correct.",
            obj(&[], &[]),
            true,
        ),
        tool(
            "run_drc",
            "Run the design rules check on the current revision. The report lists checks not implemented; no errors does not prove the design correct.",
            obj(&[], &[]),
            true,
        ),
        tool(
            "calc_trace_width",
            "IPC-2221 external/internal trace width for a current. Defaults: project temperature rise and outer copper weight.",
            obj(
                &[
                    ("current_a", number("continuous current, A")),
                    ("temp_rise_c", nullable(number("allowed temperature rise, °C"))),
                    ("copper_oz", nullable(number("copper weight, oz/ft²"))),
                    ("internal", nullable(boolean("internal layer"))),
                ],
                &["current_a"],
            ),
            false,
        ),
        tool(
            "calc_conductor_clearance",
            "IPC-2221B Table 6-1 minimum conductor spacing for a peak voltage.",
            obj(
                &[
                    ("voltage_v", number("peak working voltage, V")),
                    (
                        "condition",
                        enumeration(
                            &["B1", "B2", "B3", "B4", "A5", "A6", "A7"],
                            "B1 internal; B2 external uncoated ≤3050 m; B3 >3050 m; B4 coated; A5-A7 component leads",
                        ),
                    ),
                ],
                &["voltage_v", "condition"],
            ),
            false,
        ),
        tool(
            "calc_led_resistor",
            "Series resistor R = (Vs − Vf) / If and its dissipation.",
            obj(
                &[
                    ("supply_v", num_or_value("supply voltage, V")),
                    ("forward_v", num_or_value("LED forward voltage, V")),
                    ("current_a", num_or_value("LED current, A")),
                ],
                &["supply_v", "forward_v", "current_a"],
            ),
            false,
        ),
        tool(
            "calc_voltage_divider",
            "Unloaded divider output Vout = Vin·R2/(R1+R2).",
            obj(
                &[
                    ("vin_v", num_or_value("input voltage, V")),
                    ("r1_ohm", num_or_value("top resistor, Ω")),
                    ("r2_ohm", num_or_value("bottom resistor, Ω")),
                ],
                &["vin_v", "r1_ohm", "r2_ohm"],
            ),
            false,
        ),
        tool(
            "calc_linear_regulator",
            "Linear regulator dissipation (Vin−Vout)·Iout + Vin·Iq, and junction temperature when θJA is given.",
            obj(
                &[
                    ("vin_v", num_or_value("input voltage, V")),
                    ("vout_v", num_or_value("output voltage, V")),
                    ("iout_a", num_or_value("output current, A")),
                    ("iq_a", nullable(num_or_value("quiescent current, A"))),
                    ("ambient_c", nullable(number("ambient temperature, °C (default 25)"))),
                    ("theta_ja_c_per_w", nullable(number("junction-to-ambient θJA, °C/W"))),
                ],
                &["vin_v", "vout_v", "iout_a"],
            ),
            false,
        ),
        tool(
            "calc_rc_time_constant",
            "RC time constant τ = R·C.",
            obj(
                &[
                    ("r_ohm", num_or_value("resistance, Ω")),
                    ("c_f", num_or_value("capacitance, F")),
                ],
                &["r_ohm", "c_f"],
            ),
            false,
        ),
    ]
}

fn arg_num(args: &Value, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => Ok(n.as_f64()),
        Some(Value::String(s)) => calc::parse_value(s)
            .map(Some)
            .ok_or_else(|| format!("{key}: cannot parse value {s:?}")),
        Some(other) => Err(format!("{key}: expected a number, got {other}")),
    }
}

fn req_num(args: &Value, key: &str) -> Result<f64, String> {
    arg_num(args, key)?.ok_or_else(|| format!("missing required argument {key}"))
}

const CALC_NOTE: &str = "computed by eak-engineering (deterministic); not a design verification";

fn summary(p: &Project) -> Value {
    json!({
        "project_id": p.id,
        "name": p.name,
        "revision": p.revision,
        "requirements": p.requirements,
        "sheets": p.sheets_in_order().iter().map(|s| json!({"id": s.id, "name": s.name})).collect::<Vec<_>>(),
        "symbols": p.schematic.symbols.values().map(|s| json!({
            "id": s.id, "refdes": s.refdes, "component_id": s.component_id, "value": s.value,
            "sheet_id": s.sheet_id, "position": s.position, "rotation": s.rotation,
        })).collect::<Vec<_>>(),
        "wires": p.schematic.wires.len(),
        "labels": p.schematic.labels.values().map(|l| json!({"id": l.id, "name": l.name, "kind": l.kind})).collect::<Vec<_>>(),
        "footprints": p.pcb.footprints.values().map(|f| json!({
            "id": f.id, "refdes": f.refdes, "symbol_id": f.symbol_id, "position": f.position,
            "rotation": f.rotation, "side": f.side, "locked": f.locked,
        })).collect::<Vec<_>>(),
        "board_outline": p.pcb.board.outline,
        "stackup": p.pcb.board.stackup,
        "tracks": p.pcb.tracks.len(),
        "vias": p.pcb.vias.len(),
        "zones": p.pcb.zones.values().map(|z| json!({"id": z.id, "net": z.net, "layer": z.layer})).collect::<Vec<_>>(),
        "constraints": p.constraints,
        "manufacturing": p.manufacturing,
        "library": p.library.keys().collect::<Vec<_>>(),
        "units": "lengths are integer nanometres (1 mm = 1000000)",
    })
}

fn library(p: &Project) -> Value {
    let mut defs: Vec<(eak_core::ComponentDef, bool)> =
        p.library.values().cloned().map(|d| (d, true)).collect();
    for d in eak_core::builtin::components() {
        if !p.library.contains_key(&d.id) {
            defs.push((d, false));
        }
    }
    Value::Array(
        defs.into_iter()
            .map(|(d, in_project)| {
                json!({
                    "id": d.id, "name": d.name, "category": d.category,
                    "refdes_prefix": d.refdes_prefix, "default_value": d.default_value,
                    "in_project": in_project,
                    "pins": d.symbol.pins.iter().map(|pin| json!({
                        "number": pin.number, "name": pin.name, "type": pin.electrical_type,
                        "offset": pin.position,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

fn nets(p: &Project, only: Option<&str>) -> Value {
    let nl = netlist::build(p);
    let nets: Vec<Value> = nl
        .nets
        .values()
        .filter(|n| only.is_none_or(|o| o == n.name))
        .map(|n| {
            json!({
                "name": n.name,
                "pins": n.pins.iter().map(|r| json!({
                    "symbol_id": r.symbol_id, "refdes": r.refdes, "pin": r.pin,
                    "pin_name": r.pin_name, "type": r.electrical_type,
                })).collect::<Vec<_>>(),
                "has_power_port": n.has_power_port,
                "label_names": n.label_names,
            })
        })
        .collect();
    json!({
        "revision": p.revision,
        "nets": nets,
        "dangling_wire_ends": nl.dangling_wire_ends.iter().map(|(id, pt)| json!({"wire_id": id, "at": pt})).collect::<Vec<_>>(),
    })
}

/// Execute a read-only tool. `Ok(None)` means `name` is not a read-only tool.
pub fn run(name: &str, args: &Value, p: &Project) -> Option<Result<Value, String>> {
    let r = match name {
        "get_design_summary" => Ok(summary(p)),
        "classify_domain" => {
            serde_json::to_value(crate::domain::classify_project(p)).map_err(|e| e.to_string())
        }
        "analyze_placement" => {
            let mut v = serde_json::to_value(eak_engineering::placement_intel::analyze(p))
                .map_err(|e| e.to_string());
            if let Ok(o) = v.as_mut() {
                o["revision"] = json!(p.revision);
            }
            v
        }
        "analyze_routing" => Ok(analyze_routing(
            p,
            args.get("preview").and_then(Value::as_bool).unwrap_or(true),
        )),
        "verify_routing" => Ok(verify_routing(p)),
        "list_library_components" => Ok(json!({"revision": p.revision, "components": library(p)})),
        "get_netlist" => Ok(nets(p, args.get("net").and_then(Value::as_str))),
        "run_erc" => serde_json::to_value(eak_validation::erc::run(p)).map_err(|e| e.to_string()),
        "run_drc" => serde_json::to_value(eak_validation::drc::run(p)).map_err(|e| e.to_string()),
        "calc_trace_width" => (|| {
            let current = req_num(args, "current_a")?;
            let rise = arg_num(args, "temp_rise_c")?.unwrap_or(p.constraints.temp_rise_c);
            // From the argument or the project's own top-layer copper; never a silent 1 oz.
            let oz = match arg_num(args, "copper_oz")? {
                Some(v) => v,
                None => p
                    .pcb
                    .board
                    .stackup
                    .copper_oz(eak_core::CopperLayer::Top)
                    .ok_or("copper_oz is required: the stackup has no top copper layer")?,
            };
            let internal = args
                .get("internal")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mm = ipc::trace_width_mm(current, internal, oz, rise).map_err(|e| e.to_string())?;
            Ok(json!({
                "width_mm": mm, "width_mil": mm * ipc::MIL_PER_MM,
                "inputs": {"current_a": current, "temp_rise_c": rise, "copper_oz": oz, "internal": internal},
                "method": "IPC-2221 I = k·ΔT^0.44·A^0.725",
                "verified_against_standard": ipc::VERIFIED_AGAINST_STANDARD,
                "note": CALC_NOTE,
            }))
        })(),
        "calc_conductor_clearance" => (|| {
            let v = req_num(args, "voltage_v")?;
            let cond: ipc::ClearanceCondition =
                serde_json::from_value(args.get("condition").cloned().unwrap_or(Value::Null))
                    .map_err(|e| format!("condition: {e}"))?;
            Ok(json!({
                "clearance_mm": ipc::clearance_mm(v, cond), "voltage_v": v, "condition": cond,
                "method": "IPC-2221B Table 6-1",
                "verified_against_standard": ipc::VERIFIED_AGAINST_STANDARD,
                "note": CALC_NOTE,
            }))
        })(),
        "calc_led_resistor" => (|| {
            let (vs, vf, i) = (
                req_num(args, "supply_v")?,
                req_num(args, "forward_v")?,
                req_num(args, "current_a")?,
            );
            let r = calc::led_resistor_ohm(vs, vf, i).map_err(|e| e.to_string())?;
            let pw = calc::resistor_power_w(i, r).map_err(|e| e.to_string())?;
            Ok(json!({"resistance_ohm": r, "resistor_power_w": pw, "note": CALC_NOTE}))
        })(),
        "calc_voltage_divider" => (|| {
            let vout = calc::divider_vout(
                req_num(args, "vin_v")?,
                req_num(args, "r1_ohm")?,
                req_num(args, "r2_ohm")?,
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({"vout_v": vout, "note": CALC_NOTE}))
        })(),
        "calc_linear_regulator" => (|| {
            let pd = calc::linear_reg_dissipation_w(
                req_num(args, "vin_v")?,
                req_num(args, "vout_v")?,
                req_num(args, "iout_a")?,
                arg_num(args, "iq_a")?.unwrap_or(0.0),
            )
            .map_err(|e| e.to_string())?;
            let tj = match arg_num(args, "theta_ja_c_per_w")? {
                Some(theta) => Some(
                    calc::junction_temp_c(arg_num(args, "ambient_c")?.unwrap_or(25.0), pd, theta)
                        .map_err(|e| e.to_string())?,
                ),
                None => None,
            };
            Ok(json!({"dissipation_w": pd, "junction_temp_c": tj, "note": CALC_NOTE}))
        })(),
        "calc_rc_time_constant" => (|| {
            let tau = calc::rc_time_constant_s(req_num(args, "r_ohm")?, req_num(args, "c_f")?)
                .map_err(|e| e.to_string())?;
            Ok(json!({"tau_s": tau, "note": CALC_NOTE}))
        })(),
        _ => return None,
    };
    Some(r)
}

/// Routing analysis: priority order, pairs, per-net width/via, and (optionally) a dry-run route.
fn analyze_routing(p: &Project, preview: bool) -> Value {
    use eak_engineering::{copper, priority, router, routing_intel as ri};
    let conn = copper::analyse(p);
    let mut todo = conn.ratsnest.clone();
    priority::order_connections(p, &mut todo);
    let nets: std::collections::BTreeSet<String> = todo.iter().map(|(n, _, _)| n.clone()).collect();
    let terminals = priority::terminal_counts(p);
    let fast = priority::high_speed_nets(p);
    let order: Vec<Value> = todo
        .iter()
        .take(40)
        .map(|(n, a, b)| {
            json!({
                "net": n,
                "priority": priority::net_priority(
                    n,
                    terminals.get(n).copied().unwrap_or(2),
                    p.constraints.net_currents_a.get(n).copied(),
                    fast.contains(n),
                ),
                "length_mm": (a.dist(*b) / 1e3).round() / 1e3,
            })
        })
        .collect();
    let specs: Vec<ri::NetSpec> = nets.iter().map(|n| ri::net_spec(p, n)).collect();
    let mut out = json!({
        "revision": p.revision,
        "unrouted_connections": todo.len(),
        "priority_order": order,
        "differential_pairs": ri::diff_pairs(&nets),
        "net_specs": specs,
        "shorts": conn.shorts.len(),
        "limitations": [
            "differential pairs are detected by net name only",
            "the router routes on every signal layer with through vias only (no blind or buried spans); plane layers are not routed on",
            "widths are IPC-2221 current sizing only; no impedance, no length matching, no return-path analysis",
            "a dry run is not applied: run AutoRoute, then verify_routing",
        ],
    });
    if !preview {
        return out;
    }
    if p.pcb.board.outline.len() < 3 {
        out["preview"] = json!({"skipped": "set a board outline before routing"});
        return out;
    }
    let res = router::route(p, &router::RouterOptions::default());
    out["preview"] = json!({
        "routed_connections": res.routed_connections,
        "failed_connections": res.failed_connections.iter().take(20).map(|(net, a, b, why)| {
            json!({"net": net, "from": a, "to": b, "why": why})
        }).collect::<Vec<_>>(),
        "tracks": res.tracks.len(),
        "vias": res.vias.len(),
        "passes": res.passes,
        "ripups": res.ripups,
        "stagnated": res.stagnated,
        "congestion_hotspots": res.congestion,
        "pairs": res.pairs,
        "notes": res.notes,
        "grid_pitch_nm": res.grid_pitch,
        "applied": false,
    });
    out
}

/// Independent verification of the routed board: connectivity plus DRC, never a certification.
fn verify_routing(p: &Project) -> Value {
    let conn = eak_engineering::copper::analyse(p);
    let drc = eak_validation::drc::run(p);
    let split: Vec<Value> = conn
        .net_groups
        .iter()
        .filter(|(_, g)| **g > 1)
        .map(|(n, g)| json!({"net": n, "copper_groups": g}))
        .collect();
    let drc_errors: Vec<Value> = drc
        .findings
        .iter()
        .filter(|f| f.severity == eak_validation::Severity::Error)
        .take(30)
        .map(|f| json!({"rule_id": f.rule_id, "message": f.message, "objects": f.objects}))
        .collect();
    let clean = conn.ratsnest.is_empty() && conn.shorts.is_empty() && drc.errors == 0;
    let mut not_implemented = drc.not_implemented.clone();
    not_implemented.extend(
        [
            "differential-pair coupling, spacing and length matching",
            "controlled impedance",
            "high-speed return-path analysis",
            "thermal verification of current-sized widths and vias",
        ]
        .map(String::from),
    );
    json!({
        "revision": p.revision,
        "outcome": if clean { "NO_ERRORS_IN_IMPLEMENTED_CHECKS" } else { "ERRORS" },
        "unrouted_connections": conn.ratsnest.len(),
        "nets_with_split_copper": split,
        "shorts": conn.shorts.iter().map(|(a, b, ia, ib)| json!({"net_a": a, "net_b": b, "item_a": ia, "item_b": ib})).collect::<Vec<_>>(),
        "drc_errors": drc.errors,
        "drc_warnings": drc.warnings,
        "drc_error_findings": drc_errors,
        "not_implemented": not_implemented,
        "limitations": drc.limitations,
        "note": "no errors in implemented checks does not mean the board is correct or manufacturing-ready",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_tools_work_on_an_empty_project() {
        let p = Project::new("t");
        let a = run("analyze_routing", &json!({}), &p).unwrap().unwrap();
        assert_eq!(a["unrouted_connections"], json!(0));
        assert!(a["preview"]["skipped"].is_string());
        let v = run("verify_routing", &json!({}), &p).unwrap().unwrap();
        assert!(v["not_implemented"].as_array().unwrap().len() >= 4);
    }

    #[test]
    fn every_listed_tool_dispatches_and_names_are_unique() {
        let p = Project::new("t");
        let tools = read_tools();
        let mut names: Vec<_> = tools.iter().map(|t| t.spec.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), tools.len());
        for t in &tools {
            assert!(
                run(&t.spec.name, &json!({}), &p).is_some(),
                "{} not dispatched",
                t.spec.name
            );
        }
        assert!(run("PlaceSymbol", &json!({}), &p).is_none());
    }

    #[test]
    fn calculators_match_the_engineering_core() {
        let p = Project::new("t");
        let r = run(
            "calc_led_resistor",
            &json!({"supply_v": 5, "forward_v": "2", "current_a": "10m"}),
            &p,
        )
        .unwrap()
        .unwrap();
        assert!((r["resistance_ohm"].as_f64().unwrap() - 300.0).abs() < 1e-9);
        let w = run("calc_trace_width", &json!({"current_a": 1.0}), &p)
            .unwrap()
            .unwrap();
        let expect = ipc::trace_width_mm(1.0, false, 1.0, 10.0).unwrap();
        assert!((w["width_mm"].as_f64().unwrap() - expect).abs() < 1e-12);
        assert_eq!(w["verified_against_standard"], false);
        let reg = run(
            "calc_linear_regulator",
            &json!({"vin_v": 5, "vout_v": 3.3, "iout_a": 0.5, "theta_ja_c_per_w": 90}),
            &p,
        )
        .unwrap()
        .unwrap();
        assert!((reg["junction_temp_c"].as_f64().unwrap() - 101.5).abs() < 1e-9);
        // Non-physical inputs are refused, never guessed.
        assert!(run(
            "calc_led_resistor",
            &json!({"supply_v": 3, "forward_v": 3.3, "current_a": 0.01}),
            &p
        )
        .unwrap()
        .is_err());
        assert!(run(
            "calc_conductor_clearance",
            &json!({"voltage_v": 50, "condition": "Z9"}),
            &p
        )
        .unwrap()
        .is_err());
    }

    #[test]
    fn reports_carry_not_implemented_lists() {
        let p = Project::new("t");
        for t in ["run_erc", "run_drc"] {
            let r = run(t, &json!({}), &p).unwrap().unwrap();
            assert_eq!(r["revision"], 0);
            assert!(r["not_implemented"].is_array());
            assert!(r["limitations"].is_array());
        }
    }
}
