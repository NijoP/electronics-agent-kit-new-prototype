//! Design-for-manufacture check of the board against a fabrication profile (a fabricator's
//! published capabilities, kept as a data file with its source URL). DFM complements DRC:
//! DRC enforces the project's own rules, DFM asks whether the board and those rules fit what
//! the chosen fabricator can build. Profile values are unverified transcriptions.

use crate::holes;
use crate::report::{finding, rule, Finding, Report, Severity::*};
use eak_core::geom::to_mm;
use eak_core::{Nm, PadKind, Project, Rect};
use eak_engineering::silk;
use serde::{Deserialize, Serialize};

/// A fabricator's capabilities; lengths in millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FabProfile {
    pub id: String,
    pub name: String,
    pub source_url: String,
    pub retrieved: String,
    pub verified: bool,
    pub note: String,
    pub min_layers: usize,
    pub max_layers: usize,
    pub min_track_width_mm: f64,
    pub min_clearance_mm: f64,
    pub min_drill_mm: f64,
    pub max_drill_mm: f64,
    pub min_via_annular_ring_mm: f64,
    pub min_pth_annular_ring_mm: f64,
    pub min_hole_to_hole_mm: f64,
    pub min_copper_to_edge_mm: f64,
    pub min_board_mm: f64,
    /// Largest board, as (short side, long side).
    pub max_board_mm: (f64, f64),
    pub min_thickness_mm: f64,
    pub max_thickness_mm: f64,
    pub min_silk_line_mm: f64,
    pub min_silk_text_height_mm: f64,
}

/// JLCPCB standard 1-2 layer capabilities (`data/fab-jlcpcb-standard.json`).
pub fn jlcpcb_standard() -> FabProfile {
    serde_json::from_str(include_str!("../data/fab-jlcpcb-standard.json"))
        .expect("built-in fabrication profile is valid JSON")
}

fn nm(mm: f64) -> Nm {
    (mm * 1e6).round() as Nm
}

/// Rule-value check: a project design rule looser than the fabricator allows.
fn rule_below(f: &mut Vec<Finding>, id: &str, what: &str, project: Nm, fab_mm: f64, fix: &str) {
    if to_mm(project) + 1e-9 < fab_mm {
        f.push(
            finding(
                id,
                Error,
                format!(
                    "project {what} {:.3} mm is below the fabricator minimum {fab_mm:.3} mm",
                    to_mm(project)
                ),
                vec![],
                None,
            )
            .key(what)
            .mm_min(to_mm(project), fab_mm)
            .fix(fix.to_string()),
        );
    }
}

pub fn run(p: &Project, fab: &FabProfile) -> Report {
    let c = &p.constraints;
    let mut f: Vec<Finding> = Vec::new();

    // DFM-001: track widths as drawn.
    for t in p.pcb.tracks.values() {
        if to_mm(t.width) + 1e-9 < fab.min_track_width_mm {
            f.push(
                finding(
                    "DFM-001",
                    Error,
                    format!(
                        "track {} is {:.3} mm wide; {} minimum is {:.3} mm",
                        t.id,
                        to_mm(t.width),
                        fab.name,
                        fab.min_track_width_mm
                    ),
                    vec![t.id.clone()],
                    Some(t.a),
                )
                .mm_min(to_mm(t.width), fab.min_track_width_mm)
                .fix(format!(
                    "Widen track {} to at least {:.3} mm.",
                    t.id, fab.min_track_width_mm
                )),
            );
        }
    }
    // DFM-002: clearance rules (DRC enforces geometry against these rules).
    rule_below(
        &mut f,
        "DFM-002",
        "minimum clearance",
        c.min_clearance,
        fab.min_clearance_mm,
        "Raise the project minimum clearance to the fabricator minimum and re-run DRC.",
    );
    for nc in std::iter::once(&c.default_class).chain(&c.net_classes) {
        rule_below(
            &mut f,
            "DFM-002",
            &format!("net class {} clearance", nc.name),
            nc.clearance,
            fab.min_clearance_mm,
            "Raise the net class clearance to the fabricator minimum and re-run DRC.",
        );
    }
    // DFM-003: drill sizes.
    for h in holes::holes(p) {
        let d = to_mm(h.drill);
        let (bad, bound, limit) = if d + 1e-9 < fab.min_drill_mm {
            (true, crate::Bound::Min, fab.min_drill_mm)
        } else if d > fab.max_drill_mm + 1e-9 {
            (true, crate::Bound::Max, fab.max_drill_mm)
        } else {
            (false, crate::Bound::Min, 0.0)
        };
        if bad {
            f.push(
                finding(
                    "DFM-003",
                    Error,
                    format!(
                        "hole of {} is {d:.3} mm; {} drills {:.3}-{:.3} mm",
                        h.label, fab.name, fab.min_drill_mm, fab.max_drill_mm
                    ),
                    vec![h.object_id.clone()],
                    Some(h.center),
                )
                .key(&h.label)
                .measured(d, "mm")
                .required(bound, limit, "mm")
                .fix(format!(
                    "Change the drill of {} to between {:.3} and {:.3} mm (or route a slot/cut-out for larger holes).",
                    h.label, fab.min_drill_mm, fab.max_drill_mm
                )),
            );
        }
    }
    // DFM-004: annular rings of vias and plated pads.
    let mut rings: Vec<(String, String, eak_core::Point, Nm, f64)> = p
        .pcb
        .vias
        .values()
        .map(|v| {
            (
                v.id.clone(),
                format!("via {}", v.id),
                v.position,
                (v.diameter - v.drill) / 2,
                fab.min_via_annular_ring_mm,
            )
        })
        .collect();
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        for pad in def.footprint.pads.iter().filter(|p| p.kind == PadKind::Tht) {
            let Some(drill) = pad.drill else { continue };
            rings.push((
                fp.id.clone(),
                format!("{}.{}", fp.refdes, pad.number),
                p.pad_position(fp, pad),
                (pad.size_x.min(pad.size_y) - drill) / 2,
                fab.min_pth_annular_ring_mm,
            ));
        }
    }
    for (id, label, at, ring, min) in rings {
        if to_mm(ring) + 1e-9 < min {
            f.push(
                finding(
                    "DFM-004",
                    Error,
                    format!(
                        "annular ring of {label} is {:.3} mm; {} minimum is {min:.3} mm",
                        to_mm(ring),
                        fab.name
                    ),
                    vec![id],
                    Some(at),
                )
                .key(&label)
                .mm_min(to_mm(ring), min)
                .fix(format!(
                    "Grow the copper diameter of {label} or reduce its drill to leave a {min:.3} mm ring."
                )),
            );
        }
    }
    // DFM-005: hole-to-hole spacing.
    f.extend(holes::hole_to_hole(
        p,
        nm(fab.min_hole_to_hole_mm),
        "DFM-005",
        &fab.name,
    ));
    // DFM-006: copper-to-edge rule (DRC-005 enforces geometry against it).
    rule_below(
        &mut f,
        "DFM-006",
        "board edge clearance",
        c.board_edge_clearance,
        fab.min_copper_to_edge_mm,
        "Raise the project board-edge clearance to the fabricator minimum and re-run DRC.",
    );
    // DFM-007: board size.
    match Rect::from_points(&p.pcb.board.outline).filter(|_| p.pcb.board.outline.len() >= 3) {
        None => f.push(
            finding(
                "DFM-007",
                Error,
                "board size cannot be checked: no board outline",
                vec![],
                None,
            )
            .fix("Draw the board outline."),
        ),
        Some(r) => {
            let (w, h) = (to_mm(r.max.x - r.min.x), to_mm(r.max.y - r.min.y));
            let (short, long) = (w.min(h), w.max(h));
            if short + 1e-9 < fab.min_board_mm
                || short > fab.max_board_mm.0 + 1e-9
                || long > fab.max_board_mm.1 + 1e-9
            {
                f.push(
                    finding(
                        "DFM-007",
                        Error,
                        format!(
                            "board is {w:.2} x {h:.2} mm; {} builds {:.1} mm to {:.0} x {:.0} mm",
                            fab.name, fab.min_board_mm, fab.max_board_mm.0, fab.max_board_mm.1
                        ),
                        vec![],
                        None,
                    )
                    .fix("Resize the board outline, or panelise small boards."),
                );
            }
        }
    }
    // DFM-008: layer count.
    let layers = p.pcb.board.stackup.layers.len();
    if layers < fab.min_layers || layers > fab.max_layers {
        f.push(
            finding(
                "DFM-008",
                Error,
                format!(
                    "board has {layers} copper layers; {} profile covers {}-{}",
                    fab.name, fab.min_layers, fab.max_layers
                ),
                vec![],
                None,
            )
            .measured(layers as f64, "layers")
            .required(crate::Bound::Max, fab.max_layers as f64, "layers")
            .fix("Choose a fabrication profile for this layer count, or change the stackup."),
        );
    }
    // DFM-009: finished thickness.
    let t = to_mm(p.pcb.board.stackup.board_thickness);
    if t + 1e-9 < fab.min_thickness_mm || t > fab.max_thickness_mm + 1e-9 {
        f.push(
            finding(
                "DFM-009",
                Error,
                format!(
                    "board thickness {t:.2} mm is outside {} range {:.1}-{:.1} mm",
                    fab.name, fab.min_thickness_mm, fab.max_thickness_mm
                ),
                vec![],
                None,
            )
            .measured(t, "mm")
            .fix("Set a board thickness the fabricator offers in the stackup."),
        );
    }
    // DFM-010: generated legend line width and text height.
    rule_below(
        &mut f,
        "DFM-010",
        "silkscreen line width",
        silk::LINE_WIDTH,
        fab.min_silk_line_mm,
        "Legend strokes are generated at a fixed width; this profile needs wider strokes.",
    );
    rule_below(
        &mut f,
        "DFM-010",
        "silkscreen text height",
        silk::TEXT_HEIGHT,
        fab.min_silk_text_height_mm,
        "Legend text is generated at a fixed height; this profile needs taller text.",
    );

    let src = format!(
        "{} ({}; retrieved {}; verified = {})",
        fab.name, fab.source_url, fab.retrieved, fab.verified
    );
    let rules = vec![
        rule(
            "DFM-001",
            "Track width vs fabricator",
            format!("≥ {:.3} mm", fab.min_track_width_mm),
            &src,
        ),
        rule(
            "DFM-002",
            "Clearance rules vs fabricator",
            format!(
                "project and net-class clearance ≥ {:.3} mm",
                fab.min_clearance_mm
            ),
            &src,
        ),
        rule(
            "DFM-003",
            "Drill range",
            format!("{:.3}-{:.3} mm", fab.min_drill_mm, fab.max_drill_mm),
            &src,
        ),
        rule(
            "DFM-004",
            "Annular ring",
            format!(
                "via ≥ {:.3} mm, plated pad ≥ {:.3} mm",
                fab.min_via_annular_ring_mm, fab.min_pth_annular_ring_mm
            ),
            &src,
        ),
        rule(
            "DFM-005",
            "Hole-to-hole spacing",
            format!("≥ {:.3} mm", fab.min_hole_to_hole_mm),
            &src,
        ),
        rule(
            "DFM-006",
            "Copper-to-edge rule vs fabricator",
            format!("≥ {:.3} mm", fab.min_copper_to_edge_mm),
            &src,
        ),
        rule(
            "DFM-007",
            "Board size",
            format!(
                "{:.1} mm to {:.0} x {:.0} mm",
                fab.min_board_mm, fab.max_board_mm.0, fab.max_board_mm.1
            ),
            &src,
        ),
        rule(
            "DFM-008",
            "Layer count",
            format!("{}-{}", fab.min_layers, fab.max_layers),
            &src,
        ),
        rule(
            "DFM-009",
            "Board thickness",
            format!("{:.1}-{:.1} mm", fab.min_thickness_mm, fab.max_thickness_mm),
            &src,
        ),
        rule(
            "DFM-010",
            "Silkscreen line width and text height",
            format!(
                "line ≥ {:.3} mm, text ≥ {:.1} mm",
                fab.min_silk_line_mm, fab.min_silk_text_height_mm
            ),
            &src,
        ),
    ];
    let not_implemented = vec![
        "Copper-to-copper gaps measured against the fabricator minimum (only the project rule is compared; DRC-002 measures geometry against the project rule)".into(),
        "Acid traps, copper slivers and starved thermals".into(),
        "Solder-mask web/dam width and mask-to-pad registration".into(),
        "Paste aperture ratios and stencil checks".into(),
        "Panelisation, V-score and castellated-hole rules".into(),
        "Controlled-impedance stackup matching".into(),
        "Assembly DFM (component spacing for pick-and-place, fiducials, tooling holes)".into(),
    ];
    let limitations = vec![
        format!("Fabrication profile: {}. {}", src, fab.note),
        "A clean DFM report does not replace the fabricator's own DFM review of the generated files".into(),
    ];
    Report::new("DFM", p, rules, f, not_implemented, limitations)
}
