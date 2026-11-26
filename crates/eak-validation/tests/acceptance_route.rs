//! Workstream A acceptance fixture (eak-route-dp): a constrained 4-layer USB device built only
//! through typed engine commands (schematic, sync, placement, rules, AutoRoute, planes), then
//! auto-routed and checked. It has:
//! - a USB 2.0 pair (`USB_DP`/`USB_DN`, declared 90 ohm +-10 %, max skew 0.05 mm, gap derived
//!   from the stack-up) from the ESP32-C3 module's native USB pads to a header;
//! - power nets with declared currents (+5V 1 A, +3V3 0.5 A) sized by the router;
//! - a dense fine-pitch area: the ESP32-C3-MINI-1 land pattern (0.4 mm pads at 0.8 mm pitch and
//!   a 3x3 ground-pad grid, 26 GND pads), with UART, EN and decoupling nets escaping from it.
//!
//! Accepted when every connection is routed (empty ratsnest, no DRC-004), the pair is routed
//! coupled with its skew and Zdiff estimate within tolerance (measured again on the committed
//! copper), and DRC reports no error (DRC-024/025/026 judge the pair's skew, gap and Zdiff
//! independently of the router). Complements `autoroute_multilayer.rs` (no pair, no declared
//! currents there). Written without running (no local builds): CI decides.

use eak_core::{CopperLayer, DiffPairRule, Id, LabelKind, NetClass, Point, Project, Rotation};
use eak_engineering::copper::{self, ItemKind};
use eak_engineering::ipc;
use eak_engineering::router::{self, RouterOptions};
use eak_runtime::command::Command as C;
use eak_runtime::engine::*;
use eak_validation::drc;
use eak_validation::report::Severity;

const DP: &str = "USB_DP";
const DN: &str = "USB_DN";

fn run(e: &mut Engine, cmd: C) -> CommandResponse {
    let name = cmd.name();
    let r = e.execute(CommandRequest {
        expected_revision: e.project().revision,
        actor: Actor::human(),
        command: cmd,
        approval: None,
    });
    assert!(r.accepted, "{name}: {:?}", r.error);
    r
}

fn place(e: &mut Engine, comp: &str, value: Option<&str>) -> Id {
    let at = Point::mm(
        20.0 * (e.project().schematic.symbols.len() as f64 + 1.0),
        50.0,
    );
    run(
        e,
        C::PlaceSymbol {
            sheet_id: None,
            component_id: comp.into(),
            position: at,
            rotation: Rotation::R0,
            refdes: None,
            value: value.map(String::from),
        },
    )
    .created[0]
        .clone()
}

fn label(e: &mut Engine, sym: &Id, pin: &str, net: &str) {
    run(
        e,
        C::LabelPin {
            symbol_id: sym.clone(),
            pin: pin.into(),
            name: net.into(),
            kind: LabelKind::Net,
            scope: Default::default(),
            power_style: None,
        },
    );
}

/// The footprint realising schematic symbol `sym`.
fn footprint_of(p: &Project, sym: &Id) -> Id {
    p.pcb
        .footprints
        .values()
        .find(|f| &f.symbol_id == sym)
        .map(|f| f.id.clone())
        .expect("synced footprint")
}

fn move_to(e: &mut Engine, sym: &Id, x: f64, y: f64, rotation: Rotation) {
    let id = footprint_of(e.project(), sym);
    run(
        e,
        C::MoveFootprint {
            footprint_id: id,
            position: Point::mm(x, y),
            rotation: Some(rotation),
            side: None,
        },
    );
}

/// Centre of the top-layer pad of footprint `fp` on `net`.
fn pad_on(p: &Project, fp: &Id, net: &str) -> Point {
    copper::items(p)
        .into_iter()
        .find(|i| {
            i.kind == ItemKind::Pad
                && &i.object_id == fp
                && i.net == net
                && i.layer == CopperLayer::Top
        })
        .map(|i| i.shape.bbox().center())
        .unwrap_or_else(|| panic!("{fp} has no pad on {net}"))
}

fn net_length_mm(p: &Project, net: &str) -> f64 {
    p.pcb
        .tracks
        .values()
        .filter(|t| t.net == net)
        .map(|t| t.a.dist(t.b))
        .sum::<f64>()
        / 1e6
}

fn build() -> Engine {
    let mut e = Engine::in_memory(Project::new("eak-route-dp acceptance"));
    let j1 = place(&mut e, "CONN_1x04_P2.54", Some("USB"));
    let u1 = place(&mut e, "AMS1117-3.3_SOT-223", None);
    let c1 = place(&mut e, "C_0805", Some("10u"));
    let c2 = place(&mut e, "C_0805", Some("10u"));
    let u2 = place(&mut e, "ESP32-C3-MINI-1", None);
    let c3 = place(&mut e, "C_0603", Some("100n"));
    let r1 = place(&mut e, "R_0603", Some("10k"));
    let j2 = place(&mut e, "CONN_1x04_P2.54", Some("UART"));
    for (sym, pin, net) in [
        // USB connector: VBUS, D-, D+, GND.
        (&j1, "1", "+5V"),
        (&j1, "2", DN),
        (&j1, "3", DP),
        (&j1, "4", "GND"),
        // LDO and its capacitors.
        (&u1, "3", "+5V"),
        (&u1, "1", "GND"),
        (&u1, "2", "+3V3"),
        (&c1, "1", "+5V"),
        (&c1, "2", "GND"),
        (&c2, "1", "+3V3"),
        (&c2, "2", "GND"),
        // Module: supply, ground (all 26 GND pads), enable pull-up, native USB, UART.
        (&u2, "3", "+3V3"),
        (&u2, "1", "GND"),
        (&c3, "1", "+3V3"),
        (&c3, "2", "GND"),
        (&r1, "1", "+3V3"),
        (&r1, "2", "EN"),
        (&u2, "8", "EN"),
        (&u2, "26", DN),
        (&u2, "27", DP),
        (&u2, "30", "RXD"),
        (&u2, "31", "TXD"),
        // UART header.
        (&j2, "1", "+3V3"),
        (&j2, "2", "GND"),
        (&j2, "3", "TXD"),
        (&j2, "4", "RXD"),
    ] {
        label(&mut e, sym, pin, net);
    }

    run(
        &mut e,
        C::SetLayerCount {
            count: 4,
            board_thickness: Some(1_600_000),
        },
    );
    run(
        &mut e,
        C::SetBoardOutline {
            points: vec![
                Point::mm(0.0, 0.0),
                Point::mm(60.0, 0.0),
                Point::mm(60.0, 45.0),
                Point::mm(0.0, 45.0),
            ],
        },
    );
    for (net, amps, volts) in [("+5V", 1.0, 5.0), ("+3V3", 0.5, 3.3)] {
        run(
            &mut e,
            C::SetNetCurrent {
                net: net.into(),
                amps: Some(amps),
            },
        );
        run(
            &mut e,
            C::SetNetVoltage {
                net: net.into(),
                volts: Some(volts),
            },
        );
    }
    run(
        &mut e,
        C::UpsertNetClass {
            class: NetClass {
                name: "USB".into(),
                track_width: 500_000,
                clearance: 150_000,
                via_drill: 300_000,
                via_diameter: 600_000,
                nets: vec![DP.into(), DN.into()],
                target_z0_ohm: None,
            },
        },
    );
    run(
        &mut e,
        C::SetDiffPair {
            rule: DiffPairRule {
                positive: DP.into(),
                negative: DN.into(),
                width: Some(500_000),
                gap: None,
                target_zdiff_ohm: Some(90.0),
                zdiff_tolerance_pct: Some(10.0),
                max_skew: Some(50_000),
            },
        },
    );
    run(&mut e, C::SyncSchematicToPcb);

    // Module on the left, USB header on the right edge, power along the bottom, UART on top.
    move_to(&mut e, &u2, 18.0, 24.0, Rotation::R0);
    move_to(&mut e, &j1, 52.0, 18.0, Rotation::R0);
    move_to(&mut e, &u1, 40.0, 8.0, Rotation::R0);
    move_to(&mut e, &c1, 48.0, 8.0, Rotation::R0);
    move_to(&mut e, &c2, 32.0, 8.0, Rotation::R0);
    move_to(&mut e, &c3, 7.0, 25.0, Rotation::R0);
    move_to(&mut e, &r1, 7.0, 19.0, Rotation::R0);
    move_to(&mut e, &j2, 34.0, 41.0, Rotation::R90);
    // Keep the pair's polarity uncrossed: D+ on the same side at both ends.
    let (fu, fj) = (
        footprint_of(e.project(), &u2),
        footprint_of(e.project(), &j1),
    );
    let p = e.project();
    let module_dp_above = pad_on(p, &fu, DP).y > pad_on(p, &fu, DN).y;
    let header_dp_above = pad_on(p, &fj, DP).y > pad_on(p, &fj, DN).y;
    if module_dp_above != header_dp_above {
        move_to(&mut e, &j1, 52.0, 18.0, Rotation::R180);
    }
    e
}

#[test]
fn constrained_usb_device_is_auto_routed_complete_coupled_and_drc_clean() {
    let mut e = build();

    // What the router decides for the pair (AutoRoute below makes the same decisions).
    let plan = router::route(e.project(), &RouterOptions::default());
    assert!(
        plan.failed_connections.is_empty(),
        "{:?}",
        plan.failed_connections
    );
    let pair = plan
        .pairs
        .iter()
        .find(|r| r.positive == DP)
        .expect("the USB pair is reported");
    assert!(pair.declared && pair.coupled, "{pair:?}");
    assert_eq!(pair.skew_within_tolerance, Some(true), "{pair:?}");
    assert_eq!(pair.impedance_within_tolerance, Some(true), "{pair:?}");
    assert!(pair.note.contains("IPC-2141A"), "{}", pair.note);

    run(&mut e, C::AutoRoute { nets: None });
    let plane = vec![
        Point::mm(0.5, 0.5),
        Point::mm(59.5, 0.5),
        Point::mm(59.5, 44.5),
        Point::mm(0.5, 44.5),
    ];
    for (layer, net) in [
        (CopperLayer::Inner(1), "GND"),
        (CopperLayer::Inner(2), "+3V3"),
    ] {
        run(
            &mut e,
            C::AddZone {
                layer,
                net: net.into(),
                outline: plane.clone(),
                clearance: Some(300_000),
            },
        );
    }
    let p = e.project();

    // Every connection is proven by copper connectivity.
    let conn = copper::analyse(p);
    assert!(conn.ratsnest.is_empty(), "unrouted: {:?}", conn.ratsnest);
    assert!(conn.shorts.is_empty(), "{:?}", conn.shorts);

    // The committed pair is length matched within the declared skew.
    let (lp, ln) = (net_length_mm(p, DP), net_length_mm(p, DN));
    assert!(lp > 0.0 && ln > 0.0);
    assert!(
        (lp - ln).abs() <= 0.05 + 1e-6,
        "skew {:.4} mm (DP {lp:.3}, DN {ln:.3})",
        (lp - ln).abs()
    );

    // Every routed segment of every power net is at least the IPC-2221 width its declared current
    // needs on that segment's layer (the DRC-008 calculation: `ipc::trace_width_mm` with the
    // layer's copper weight and the board's temperature rise), and the 1 A +5V rail is wider
    // than the 0.25 mm default everywhere.
    let c = &p.constraints;
    for (net, amps) in [("+5V", 1.0), ("+3V3", 0.5)] {
        assert_eq!(
            c.net_currents_a.get(net),
            Some(&amps),
            "{net} current declared"
        );
        let tracks: Vec<_> = p.pcb.tracks.values().filter(|t| t.net == net).collect();
        assert!(!tracks.is_empty(), "{net} has no routed track");
        for t in tracks {
            let oz = p
                .pcb
                .board
                .stackup
                .copper_oz(t.layer)
                .expect("track layer in stack-up");
            let need = ipc::trace_width_mm(amps, !t.layer.is_outer(), oz, c.temp_rise_c)
                .expect("IPC-2221 width computable");
            let have = t.width as f64 / 1e6;
            assert!(
                have + 1e-9 >= need,
                "{net} segment {} on {} is {have:.3} mm, below the {need:.3} mm IPC-2221 needs for {amps} A",
                t.id,
                t.layer.name()
            );
            if net == "+5V" {
                assert!(
                    t.width > 250_000,
                    "+5V segment {} is not wider than 0.25 mm",
                    t.id
                );
            }
        }
    }

    let errors: Vec<String> = drc::run(p)
        .findings
        .into_iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| format!("{}: {}", f.rule_id, f.message))
        .collect();
    assert!(errors.is_empty(), "DRC errors: {errors:#?}");
}
