//! End-to-end checks of the ESP32-C3 dashboard board, built through the EAK command engine.

use dashboard_core::pins;
use eak_core::Project;
use eak_runtime::Engine;
use eak_validation::{drc, erc, mech};
use esp32_dashboard_design::{build_pcb, build_schematic, Driver};
use std::collections::BTreeMap;
use std::path::Path;

fn build() -> (Project, Vec<String>) {
    let mut e = Engine::in_memory(Project::new("ESP32-C3 Environment Dashboard"));
    let mut d = Driver::new(&mut e);
    build_schematic(&mut d).expect("schematic");
    let notes = build_pcb(&mut d).expect("pcb");
    (e.project().clone(), notes)
}

/// refdes.pin -> net, from EAK's own netlist resolver.
fn connectivity(p: &Project) -> BTreeMap<String, String> {
    let nl = eak_engineering::netlist::build(p);
    let mut m = BTreeMap::new();
    for n in nl.nets.values() {
        for pin in &n.pins {
            if n.pins.len() + n.label_ids.len() > 1 {
                m.insert(format!("{}.{}", pin.refdes, pin.pin), n.name.clone());
            }
        }
    }
    m
}

fn committed() -> Project {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../project/project.eak.json");
    let s = std::fs::read_to_string(&path).expect("committed native EAK project");
    eak_core::from_json(&s).expect("committed project parses, migrates and passes integrity")
}

#[test]
fn erc_drc_mech_report_no_errors_and_router_completes() {
    let (p, notes) = build();
    let e = erc::run(&p);
    let d = drc::run(&p);
    let m = mech::run(&p);
    for r in [&e, &d, &m] {
        for f in &r.findings {
            eprintln!("{} {} {:?} {}", r.check, f.rule_id, f.severity, f.message);
        }
        assert_eq!(r.errors, 0, "{} errors", r.check);
        assert!(
            !r.not_implemented.is_empty(),
            "{} must list not-implemented checks",
            r.check
        );
    }
    let router: Vec<_> = notes.iter().filter(|n| n.starts_with("router")).collect();
    assert_eq!(router.len(), 1);
    assert!(router[0].ends_with(", 0 failed"), "{router:?}");
    assert!(!notes
        .iter()
        .any(|n| n.starts_with("unrouted") || n.starts_with("could not place")));
}

#[test]
fn manufacturing_package_compiles_and_reverifies() {
    let (p, _) = build();
    let reports = [erc::run(&p), drc::run(&p), mech::run(&p)];
    let dir = std::env::temp_dir().join(format!("eak-esp32-mfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let m = eak_manufacturing::compile(&p, &reports.iter().collect::<Vec<_>>(), &dir).unwrap();
    assert_eq!(
        m.status,
        eak_manufacturing::PackageStatus::InternallyVerifiedReleaseCandidate,
        "{:?}",
        m.blocking_reasons
    );
    for kind in ["gtl", "gbl", "gm1", "drl", "BOM.csv", "CPL.csv"] {
        assert!(
            m.files.iter().any(|f| f.name.ends_with(kind)),
            "missing {kind}"
        );
    }
    assert!(eak_manufacturing::verify_package(&dir).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn committed_project_matches_a_fresh_build() {
    // Object ids are random per build; the connectivity and placement must be identical.
    let (fresh, _) = build();
    let saved = committed();
    assert_eq!(connectivity(&fresh), connectivity(&saved));
    let place = |p: &Project| -> BTreeMap<String, (i64, i64, u32)> {
        p.pcb
            .footprints
            .values()
            .map(|f| {
                (
                    f.refdes.clone(),
                    (f.position.x, f.position.y, f.rotation.degrees()),
                )
            })
            .collect()
    };
    assert_eq!(place(&fresh), place(&saved));
    // Auto-routed copper is not compared: EAK iterates objects in id order and ids are
    // random, so the router may pick different (equally valid) paths. Both must be complete.
    for p in [&fresh, &saved] {
        assert!(!drc::run(p).findings.iter().any(|f| f.rule_id == "DRC-004"));
    }
}

#[test]
fn committed_reports_are_bound_to_the_committed_revision() {
    let saved = committed();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../outputs");
    for check in ["erc", "drc", "mech"] {
        let s = std::fs::read_to_string(dir.join(format!("reports/{check}.json"))).unwrap();
        let r: eak_validation::Report = serde_json::from_str(&s).unwrap();
        assert_eq!(r.revision, saved.revision, "{check}");
        assert_eq!(
            r.input_sha256,
            eak_validation::report::project_hash(&saved),
            "{check}"
        );
        assert_eq!(r.errors, 0, "{check}");
    }
    assert!(
        eak_manufacturing::verify_package(&dir.join("manufacturing"))
            .unwrap()
            .is_empty()
    );
}

/// The firmware pin map, the module datasheet pin names and the saved EAK netlist agree.
#[test]
fn firmware_pin_map_matches_the_eak_netlist() {
    let p = committed();
    let conn = connectivity(&p);
    let u1 = p
        .schematic
        .symbols
        .values()
        .find(|s| s.refdes == "U1")
        .expect("U1");
    let def = &p.library[&u1.component_id];
    for a in pins::ALL {
        assert_eq!(
            conn.get(&format!("U1.{}", a.module_pin)),
            Some(&a.net.to_string()),
            "{}",
            a.signal
        );
        let pin = def.pin(a.module_pin).expect("module pin exists");
        assert_eq!(
            pin.name, a.module_pin_name,
            "datasheet pin name for {}",
            a.signal
        );
    }
    for nc in pins::UNUSED_MODULE_PINS {
        assert!(
            !conn.contains_key(&format!("U1.{nc}")),
            "U1.{nc} should be unconnected"
        );
        assert!(p
            .schematic
            .no_connects
            .values()
            .any(|n| n.symbol_id == u1.id && n.pin == nc));
    }
    // Power: module 3V3 and both GND pins.
    assert_eq!(conn["U1.1"], "+3V3");
    assert_eq!(conn["U1.9"], "GND");
    assert_eq!(conn["U1.19"], "GND");
    // Headers carry the bus the firmware drives, on the same 3.3 V rail as the module.
    for (hdr, table) in [
        ("J2", pins::J2_SENSOR_HEADER),
        ("J3", pins::J3_DISPLAY_HEADER),
    ] {
        for (pin, net) in table {
            assert_eq!(conn[&format!("{hdr}.{pin}")], net);
        }
    }
    // Pull-ups the firmware relies on: I2C (4.7 k), button and strapping pins to +3V3.
    for (r, net) in [
        ("R8", pins::I2C_SDA.net),
        ("R9", pins::I2C_SCL.net),
        ("R4", pins::USER_BUTTON.net),
        ("R5", pins::STRAP_GPIO8.net),
        ("R6", pins::STRAP_GPIO2.net),
        ("R3", pins::CHIP_EN.net),
    ] {
        assert_eq!(conn[&format!("{r}.1")], "+3V3", "{r}");
        assert_eq!(conn[&format!("{r}.2")], net, "{r}");
    }
    let value = |r: &str| {
        p.schematic
            .symbols
            .values()
            .find(|s| s.refdes == r)
            .unwrap()
            .value
            .clone()
    };
    let ohm = |r: &str| eak_engineering::calc::parse_value(&value(r)).unwrap() as u32;
    assert_eq!(ohm("R8"), pins::I2C_PULLUP_OHM);
    assert_eq!(ohm("R9"), pins::I2C_PULLUP_OHM);
    assert_eq!(ohm("R10"), pins::VBUS_DIV_TOP_OHM);
    assert_eq!(ohm("R11"), pins::VBUS_DIV_BOTTOM_OHM);
    assert_eq!(ohm("R7"), pins::LED_SERIES_OHM);
    // Divider topology: VBUS -> R10 -> VBUS_SENSE -> R11 -> GND.
    assert_eq!(conn["R10.1"], "VBUS");
    assert_eq!(conn["R10.2"], pins::VBUS_SENSE.net);
    assert_eq!(conn["R11.2"], "GND");
    // USB data lines reach the receptacle on both plug orientations (A6/B6, A7/B7).
    let j1 = p
        .schematic
        .symbols
        .values()
        .find(|s| s.refdes == "J1")
        .unwrap();
    let jdef = &p.library[&j1.component_id];
    assert_eq!(jdef.pin_map["5"], vec!["A6", "B6"]);
    assert_eq!(jdef.pin_map["6"], vec!["A7", "B7"]);
    assert_eq!(conn["J1.5"], pins::USB_DP.net);
    assert_eq!(conn["J1.6"], pins::USB_DM.net);
}

/// Every PCB pad net agrees with the schematic (forward annotation is complete).
#[test]
fn pcb_pad_nets_follow_the_schematic() {
    let p = committed();
    let conn = connectivity(&p);
    for fp in p.pcb.footprints.values() {
        let def = &p.library[&fp.component_id];
        for (pin, pads) in &def.pin_map {
            let want = conn.get(&format!("{}.{}", fp.refdes, pin));
            for pad in pads {
                assert_eq!(
                    fp.pad_nets.get(pad),
                    want,
                    "{}.{} pad {}",
                    fp.refdes,
                    pin,
                    pad
                );
            }
        }
    }
}
