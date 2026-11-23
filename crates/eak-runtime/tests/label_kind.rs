//! `SetNetLabelKind` (ADR-0048): a label changes between a net label and a power port in place.
//! It keeps its id, name, position, scope, power style and group; one revision, one undo step.

use eak_core::{LabelKind, NetScope, Point, PowerStyle, Project};
use eak_runtime::command::Command as C;
use eak_runtime::engine::*;
use eak_runtime::CommandError;

fn run(e: &mut Engine, cmd: C) -> CommandResponse {
    let rev = e.project().revision;
    e.execute(CommandRequest {
        expected_revision: rev,
        actor: Actor::human(),
        command: cmd,
        approval: None,
    })
}

fn ok(e: &mut Engine, cmd: C) -> CommandResponse {
    let r = run(e, cmd);
    assert!(r.accepted, "{:?}", r.error);
    r
}

fn add_label(e: &mut Engine, sheet: &str, x: f64, kind: LabelKind, scope: NetScope) -> String {
    let add = C::AddNetLabel {
        sheet_id: Some(sheet.into()),
        name: "VCC".into(),
        position: Point::mm(x, 0.0),
        kind,
        scope,
        power_style: None,
    };
    ok(e, add).created[0].clone()
}

#[test]
fn kind_change_keeps_id_group_and_style_and_is_one_undo_step() {
    let mut e = Engine::in_memory(Project::new("label-kind"));
    let s = e.project().first_sheet_id().unwrap().clone();
    let l = add_label(&mut e, &s, 0.0, LabelKind::Power, NetScope::Global);
    let other = add_label(&mut e, &s, 20.0, LabelKind::Net, NetScope::Global);
    ok(
        &mut e,
        C::SetPowerPortStyle {
            label_id: l.clone(),
            style: Some(PowerStyle::Rail),
        },
    );
    let grp = C::Group {
        members: vec![l.clone(), other],
        name: None,
    };
    let g = ok(&mut e, grp).created[0].clone();
    let before = e.project().clone();

    ok(
        &mut e,
        C::SetNetLabelKind {
            label_id: l.clone(),
            kind: LabelKind::Net,
        },
    );
    let p = e.project();
    assert_eq!(p.revision, before.revision + 1, "one revision");
    let now = &p.schematic.labels[&l];
    let old = &before.schematic.labels[&l];
    assert_eq!(now.kind, LabelKind::Net);
    assert_eq!(
        (&now.id, &now.name, now.position, now.scope, now.power_style),
        (&old.id, &old.name, old.position, old.scope, old.power_style),
        "only the kind changes; the power style is kept"
    );
    assert_eq!(
        p.schematic.group_of(&l).map(|x| &x.id),
        Some(&g),
        "still grouped"
    );

    ok(
        &mut e,
        C::SetNetLabelKind {
            label_id: l.clone(),
            kind: LabelKind::Power,
        },
    );
    let back = &e.project().schematic.labels[&l];
    assert_eq!(
        (back.kind, back.power_style),
        (LabelKind::Power, Some(PowerStyle::Rail))
    );

    let r = e.undo(e.project().revision, &Actor::human());
    assert!(r.accepted, "{:?}", r.error);
    let r = e.undo(e.project().revision, &Actor::human());
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(e.project().schematic, before.schematic);
}

#[test]
fn kind_change_warns_for_local_labels_and_refuses_unchanged_and_unknown() {
    let mut e = Engine::in_memory(Project::new("label-kind"));
    let s = e.project().first_sheet_id().unwrap().clone();
    let l = add_label(&mut e, &s, 0.0, LabelKind::Net, NetScope::Local);
    let r = ok(
        &mut e,
        C::SetNetLabelKind {
            label_id: l.clone(),
            kind: LabelKind::Power,
        },
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("local label")),
        "{:?}",
        r.warnings
    );
    let r = run(
        &mut e,
        C::SetNetLabelKind {
            label_id: l,
            kind: LabelKind::Power,
        },
    );
    assert!(matches!(r.error, Some(CommandError::NothingTo(_))));
    let r = run(
        &mut e,
        C::SetNetLabelKind {
            label_id: "lbl_missing".into(),
            kind: LabelKind::Net,
        },
    );
    assert!(matches!(r.error, Some(CommandError::NotFound(_))));
}
