//! Slice B4 (place tools) acceptance: drawing with the sheet graphics and text tools leaves the
//! ERC result of the example sheet unchanged, because graphics never take part in connectivity.
//! The commands are the ones the desktop Line, Rectangle, Circle and Text tools send.

use eak_core::*;
use eak_runtime::command::Command as C;
use eak_runtime::{fixtures, Actor, CommandRequest, Engine};

/// Every finding of the ERC run, as sorted text (id, rule, message, objects).
fn erc_result(p: &Project) -> Vec<String> {
    let r = eak_validation::erc::run(p);
    let mut v: Vec<String> = r.findings.iter().map(row).collect();
    v.sort();
    v
}

fn row(f: &eak_validation::Finding) -> String {
    format!("{} {} {} {:?}", f.id, f.rule_id, f.message, f.objects)
}

fn first_pin(p: &Project, s: &SymbolInstance) -> Option<Point> {
    let def = p.library.get(&s.component_id)?;
    def.symbol.pins.first().map(|pin| s.place(pin.position))
}

#[test]
fn graphics_and_text_tools_leave_the_example_sheet_erc_unchanged() {
    let mut e = Engine::in_memory(Project::new("b4"));
    fixtures::build_schematic(&mut e).unwrap();
    let before = erc_result(e.project());
    let unchecked = eak_validation::erc::run(e.project()).not_implemented;

    // Drawn over the parts on purpose: a line or a text lying on a pin must not connect it.
    let p = e.project();
    let pins: Vec<Point> = p
        .schematic
        .symbols
        .values()
        .filter_map(|s| first_pin(p, s))
        .collect();
    assert!(pins.len() >= 2, "the example sheet has parts");
    let (a, b) = (pins[0], pins[1]);
    let stroke = Stroke::default();
    let margin = 1_000_000;
    let graphics = [
        Graphic::Line { a, b, stroke },
        Graphic::Rectangle {
            min: Point::new(a.x.min(b.x) - margin, a.y.min(b.y) - margin),
            max: Point::new(a.x.max(b.x) + margin, a.y.max(b.y) + margin),
            fill: false,
            stroke,
        },
        Graphic::Circle {
            center: a,
            radius: 2_540_000,
            fill: false,
            stroke,
        },
        Graphic::Text {
            position: b,
            text: "B4 note".into(),
            size: 1_270_000,
            rotation: Rotation::R0,
        },
    ];
    let commands = graphics
        .into_iter()
        .map(|graphic| C::AddSheetGraphic {
            sheet_id: None,
            graphic,
        })
        .collect();
    let rev = e.project().revision;
    let r = e.execute(CommandRequest {
        expected_revision: rev,
        actor: Actor::human(),
        command: C::Batch {
            commands,
            label: Some("B4 graphics".into()),
        },
        approval: None,
    });
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(e.project().schematic.graphics.len(), 4);
    assert_eq!(erc_result(e.project()), before, "ERC changed by drawings");
    let unchecked_after = eak_validation::erc::run(e.project()).not_implemented;
    assert_eq!(unchecked_after, unchecked);
}
