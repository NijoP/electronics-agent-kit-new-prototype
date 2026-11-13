//! JLCPCB manufacturing preparation (a provider separate from LCSC sourcing).
//!
//! Produces, for human review only:
//! * a JLCPCB-format assembly BOM (`Comment, Designator, Footprint, LCSC Part #`) and a
//!   review BOM with manufacturer, MPN and the stored stock/price snapshot;
//! * a JLCPCB-format pick-and-place file (`Designator, Mid X, Mid Y, Layer, Rotation`)
//!   holding the surface-mount footprints;
//! * a Gerber + Excellon `.zip` made from the files `compile` generated and self-validated;
//! * a capability / DFM rule check against JLCPCB's published 2-layer limits, in which every
//!   rule that cannot be evaluated is reported as `not_checked`, never as a pass.
//!
//! Nothing is uploaded and no order is placed or prepared for placement. The output is
//! labelled "prepared for review", never "manufacturing-ready".

use crate::bom::{self, BomLine};
use crate::sourcing::SourcingRecord;
use crate::{compile, FileEntry, Manifest, PackageStatus};
use eak_core::geom::{
    point_polygon_boundary_distance, point_segment_distance, segment_segment_distance,
    segments_intersect, to_mm, Rect,
};
use eak_core::{CopperLayer, PadKind, Point, Project, Side};
use eak_validation::sourcing::is_lcsc_part_number;
use eak_validation::Report;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// JLCPCB limits for a 2-layer FR-4 board with 1 oz outer copper, in millimetres.
///
/// Read from JLCPCB's published capability page on `verified_on`. A profile without a
/// source and a verification date would be UNVERIFIED and unusable; the values below are
/// the page's own numbers, not EAK defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub version: String,
    pub source_url: String,
    pub verified_on: String,
    pub verified_by: String,
    pub copper_layers: u8,
    pub outer_copper_oz: f64,
    pub min_track_mm: f64,
    pub min_track_space_mm: f64,
    pub min_pad_to_track_mm: f64,
    pub min_smd_pad_to_pad_mm: f64,
    pub min_smd_pad_mm: f64,
    pub min_drill_mm: f64,
    pub recommended_pth_drill_mm: f64,
    pub min_via_ring_mm: f64,
    pub min_pth_ring_mm: f64,
    pub recommended_pth_ring_mm: f64,
    pub min_via_hole_to_hole_mm: f64,
    pub min_pad_hole_to_hole_mm: f64,
    pub min_copper_to_edge_mm: f64,
    pub min_board_mm: (f64, f64),
    pub max_board_mm: (f64, f64),
}

impl Profile {
    pub fn two_layer_1oz() -> Profile {
        Profile {
            id: "jlcpcb-fr4-2layer-1oz".into(),
            version: "2026-11-14".into(),
            source_url: "https://jlcpcb.com/capabilities/pcb-capabilities".into(),
            verified_on: "2026-11-14".into(),
            verified_by: "EAK worker, read of the published page text (not a fabricator DFM)"
                .into(),
            copper_layers: 2,
            outer_copper_oz: 1.0,
            min_track_mm: 0.10,
            min_track_space_mm: 0.10,
            min_pad_to_track_mm: 0.10,
            min_smd_pad_to_pad_mm: 0.15,
            min_smd_pad_mm: 0.25,
            min_drill_mm: 0.15,
            recommended_pth_drill_mm: 0.5,
            // Page: "via diameter should be 0.1 mm (0.15 preferred) larger than hole size".
            min_via_ring_mm: 0.05,
            // Page: PTH annular ring >= 0.20 mm; 2-layer 1 oz absolute minimum 0.18 mm,
            // recommended 0.25 mm.
            min_pth_ring_mm: 0.18,
            recommended_pth_ring_mm: 0.25,
            min_via_hole_to_hole_mm: 0.2,
            min_pad_hole_to_hole_mm: 0.45,
            min_copper_to_edge_mm: 0.2,
            min_board_mm: (3.0, 3.0),
            max_board_mm: (670.0, 600.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pass,
    /// Within the hard limit but outside JLCPCB's recommendation, or a soft finding.
    Warn,
    Fail,
    /// The rule exists but this software cannot evaluate it. Never a pass.
    NotChecked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfmCheck {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub required: String,
    pub measured: String,
    /// Offending objects or the reason a rule was not checked (at most 12 listed).
    pub detail: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfmReport {
    pub profile: Profile,
    pub checks: Vec<DfmCheck>,
    pub failures: usize,
    pub warnings: usize,
    pub not_checked: usize,
    pub statement: String,
    pub limitations: Vec<String>,
}

fn mm(nm: f64) -> f64 {
    nm / 1e6
}

fn fmt_mm(v: f64) -> String {
    format!("{v:.3} mm")
}

const LISTED: usize = 12;

struct Acc {
    id: &'static str,
    title: &'static str,
    required: String,
    min: Option<f64>,
    detail: Vec<String>,
    fails: usize,
    warns: usize,
    objects: usize,
}

impl Acc {
    fn new(id: &'static str, title: &'static str, required: String) -> Acc {
        Acc {
            id,
            title,
            required,
            min: None,
            detail: vec![],
            fails: 0,
            warns: 0,
            objects: 0,
        }
    }

    /// Record one measurement against a hard limit (and optional recommendation), in mm.
    fn measure(
        &mut self,
        what: impl FnOnce() -> String,
        value_mm: f64,
        hard: f64,
        rec: Option<f64>,
    ) {
        self.objects += 1;
        self.min = Some(self.min.map_or(value_mm, |m| m.min(value_mm)));
        // 1 nm tolerance so a value exactly at the limit passes.
        let eps = 1e-6;
        let status = if value_mm + eps < hard {
            self.fails += 1;
            Some(("below limit", hard))
        } else if let Some(r) = rec.filter(|r| value_mm + eps < *r) {
            self.warns += 1;
            Some(("below recommended", r))
        } else {
            None
        };
        if let (Some((s, lim)), true) = (status, self.detail.len() < LISTED) {
            self.detail.push(format!(
                "{}: {} ({s} {})",
                what(),
                fmt_mm(value_mm),
                fmt_mm(lim)
            ));
        }
    }

    fn done(self) -> DfmCheck {
        let min = fmt_mm(self.min.unwrap_or(0.0));
        let (status, measured) = if self.objects == 0 {
            (
                Status::NotChecked,
                "nothing to measure on this board".to_string(),
            )
        } else if self.fails > 0 {
            (
                Status::Fail,
                format!("minimum {min} ({} violation(s))", self.fails),
            )
        } else if self.warns > 0 {
            (
                Status::Warn,
                format!("minimum {min} ({} below recommendation)", self.warns),
            )
        } else {
            (Status::Pass, format!("minimum {min}"))
        };
        DfmCheck {
            id: self.id.into(),
            title: self.title.into(),
            status,
            required: self.required,
            measured,
            detail: self.detail,
        }
    }
}

#[derive(Clone)]
struct PadRect {
    refdes: String,
    number: String,
    layers: Vec<CopperLayer>,
    cx: f64,
    cy: f64,
    hw: f64,
    hh: f64,
    net: Option<String>,
    drill: Option<f64>,
    tht: bool,
}

impl PadRect {
    fn label(&self) -> String {
        format!("{}.{}", self.refdes, self.number)
    }

    fn corners(&self) -> [Point; 4] {
        let p = |dx: f64, dy: f64| Point {
            x: (self.cx + dx) as i64,
            y: (self.cy + dy) as i64,
        };
        [
            p(-self.hw, -self.hh),
            p(self.hw, -self.hh),
            p(self.hw, self.hh),
            p(-self.hw, self.hh),
        ]
    }
}

fn pads(p: &Project) -> Vec<PadRect> {
    let mut out = vec![];
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        for pad in &def.footprint.pads {
            let at = p.pad_position(fp, pad);
            let (w, h) = fp.rotation.apply_size(pad.size_x, pad.size_y);
            let tht = pad.kind == PadKind::Tht;
            out.push(PadRect {
                refdes: fp.refdes.clone(),
                number: pad.number.clone(),
                layers: if tht {
                    CopperLayer::ALL.to_vec()
                } else {
                    vec![fp.side.copper()]
                },
                cx: at.x as f64,
                cy: at.y as f64,
                hw: w as f64 / 2.0,
                hh: h as f64 / 2.0,
                net: fp.pad_nets.get(&pad.number).cloned(),
                drill: pad.drill.filter(|_| tht).map(|d| d as f64),
                tht,
            });
        }
    }
    out
}

fn same_net(a: Option<&str>, b: Option<&str>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if !x.is_empty() && x == y)
}

fn point_rect(px: f64, py: f64, r: &PadRect) -> f64 {
    let dx = ((px - r.cx).abs() - r.hw).max(0.0);
    let dy = ((py - r.cy).abs() - r.hh).max(0.0);
    dx.hypot(dy)
}

/// Distance between a segment and an axis-aligned rectangle (0 when they touch).
fn segment_rect(a: Point, b: Point, r: &PadRect) -> f64 {
    let c = r.corners();
    for i in 0..4 {
        if segments_intersect(a, b, c[i], c[(i + 1) % 4]) {
            return 0.0;
        }
    }
    let da = point_rect(a.x as f64, a.y as f64, r);
    let db = point_rect(b.x as f64, b.y as f64, r);
    if da == 0.0 || db == 0.0 {
        return 0.0;
    }
    c.iter()
        .map(|corner| point_segment_distance(*corner, a, b))
        .fold(da.min(db), f64::min)
}

fn rect_rect(a: &PadRect, b: &PadRect) -> f64 {
    let gx = ((a.cx - b.cx).abs() - a.hw - b.hw).max(0.0);
    let gy = ((a.cy - b.cy).abs() - a.hh - b.hh).max(0.0);
    gx.hypot(gy)
}

fn outline_edges(p: &Project) -> Vec<(Point, Point)> {
    let o = &p.pcb.board.outline;
    (0..o.len()).map(|i| (o[i], o[(i + 1) % o.len()])).collect()
}

fn not_checked(id: &str, title: &str, why: &str) -> DfmCheck {
    DfmCheck {
        id: id.into(),
        title: title.into(),
        status: Status::NotChecked,
        required: "per JLCPCB capability page".into(),
        measured: "not evaluated".into(),
        detail: vec![why.into()],
    }
}

/// Run the capability / DFM rule check. `bom` is the BOM view of the same project.
pub fn dfm(p: &Project, bom: &[BomLine], profile: &Profile) -> DfmReport {
    let mut checks: Vec<DfmCheck> = vec![];
    let pads = pads(p);
    let edges = outline_edges(p);
    let outline_ok = p.pcb.board.outline.len() >= 3;

    // JLC-001 board size
    {
        let mut c = DfmCheck {
            id: "JLC-001".into(),
            title: "Board outline and dimensions".into(),
            status: Status::Fail,
            required: format!(
                "closed outline, {:.0}x{:.0} mm to {:.0}x{:.0} mm",
                profile.min_board_mm.0,
                profile.min_board_mm.1,
                profile.max_board_mm.0,
                profile.max_board_mm.1
            ),
            measured: "no board outline".into(),
            detail: vec![],
        };
        if let Some(r) = Rect::from_points(&p.pcb.board.outline).filter(|_| outline_ok) {
            let (w, h) = (to_mm(r.width()), to_mm(r.height()));
            c.measured = format!("{w:.2} x {h:.2} mm bounding box");
            let (lo, hi) = (profile.min_board_mm, profile.max_board_mm);
            let ok_max = (w <= hi.0 && h <= hi.1) || (h <= hi.0 && w <= hi.1);
            let ok_min = w >= lo.0 && h >= lo.1;
            c.status = if ok_max && ok_min {
                Status::Pass
            } else {
                Status::Fail
            };
            if eak_core::geom::polygon_self_intersects(&p.pcb.board.outline) {
                c.status = Status::Fail;
                c.detail.push("outline polygon intersects itself".into());
            }
        }
        checks.push(c);
    }

    // JLC-002 stackup
    let st = &p.pcb.board.stackup;
    checks.push(DfmCheck {
        id: "JLC-002".into(),
        title: "Layer count and copper weight match the profile".into(),
        status: if st.copper_layers == profile.copper_layers
            && (st.outer_copper_oz - profile.outer_copper_oz).abs() < 1e-9
        {
            Status::Pass
        } else {
            Status::Fail
        },
        required: format!(
            "{} layers, {} oz outer copper (the limits below are for this build-up)",
            profile.copper_layers, profile.outer_copper_oz
        ),
        measured: format!("{} layers, {} oz", st.copper_layers, st.outer_copper_oz),
        detail: vec![],
    });

    // JLC-010 track width
    let mut a = Acc::new(
        "JLC-010",
        "Minimum track width",
        fmt_mm(profile.min_track_mm),
    );
    for t in p.pcb.tracks.values() {
        a.measure(
            || format!("track {} ({:?}, net {})", t.id, t.layer, t.net),
            to_mm(t.width),
            profile.min_track_mm,
            None,
        );
    }
    checks.push(a.done());

    // JLC-011 track-to-track spacing, different nets, same layer
    let mut a = Acc::new(
        "JLC-011",
        "Track-to-track spacing (different nets)",
        fmt_mm(profile.min_track_space_mm),
    );
    let tracks: Vec<_> = p.pcb.tracks.values().collect();
    for i in 0..tracks.len() {
        for j in i + 1..tracks.len() {
            let (s, t) = (tracks[i], tracks[j]);
            if s.layer != t.layer || same_net(Some(&s.net), Some(&t.net)) {
                continue;
            }
            let gap =
                segment_segment_distance(s.a, s.b, t.a, t.b) - (s.width + t.width) as f64 / 2.0;
            a.measure(
                || format!("{} vs {}", s.id, t.id),
                mm(gap.max(0.0)),
                profile.min_track_space_mm,
                None,
            );
        }
    }
    checks.push(a.done());

    // JLC-012 pad-to-track and via-to-track
    let mut a = Acc::new(
        "JLC-012",
        "Pad / via to track spacing (different nets)",
        fmt_mm(profile.min_pad_to_track_mm),
    );
    for t in p.pcb.tracks.values() {
        for pd in pads.iter().filter(|pd| pd.layers.contains(&t.layer)) {
            if same_net(Some(&t.net), pd.net.as_deref()) {
                continue;
            }
            let gap = segment_rect(t.a, t.b, pd) - t.width as f64 / 2.0;
            a.measure(
                || format!("track {} vs pad {}", t.id, pd.label()),
                mm(gap.max(0.0)),
                profile.min_pad_to_track_mm,
                None,
            );
        }
        for v in p
            .pcb
            .vias
            .values()
            .filter(|v| !same_net(Some(&t.net), Some(&v.net)))
        {
            let gap =
                point_segment_distance(v.position, t.a, t.b) - (v.diameter + t.width) as f64 / 2.0;
            a.measure(
                || format!("track {} vs via {}", t.id, v.id),
                mm(gap.max(0.0)),
                profile.min_pad_to_track_mm,
                None,
            );
        }
    }
    checks.push(a.done());

    // JLC-013 SMD pad-to-pad
    let mut a = Acc::new(
        "JLC-013",
        "SMD pad-to-pad spacing (different nets)",
        fmt_mm(profile.min_smd_pad_to_pad_mm),
    );
    let smd: Vec<&PadRect> = pads.iter().filter(|x| !x.tht).collect();
    for i in 0..smd.len() {
        for j in i + 1..smd.len() {
            let (x, y) = (smd[i], smd[j]);
            if x.layers != y.layers || same_net(x.net.as_deref(), y.net.as_deref()) {
                continue;
            }
            a.measure(
                || format!("{} vs {}", x.label(), y.label()),
                mm(rect_rect(x, y)),
                profile.min_smd_pad_to_pad_mm,
                None,
            );
        }
    }
    checks.push(a.done());

    // JLC-014 smallest SMD pad
    let mut a = Acc::new(
        "JLC-014",
        "Smallest SMD pad",
        format!("{0:.2} x {0:.2} mm", profile.min_smd_pad_mm),
    );
    for x in &smd {
        a.measure(
            || format!("pad {}", x.label()),
            mm(2.0 * x.hw.min(x.hh)),
            profile.min_smd_pad_mm,
            None,
        );
    }
    checks.push(a.done());

    // JLC-020 drills
    let mut a = Acc::new(
        "JLC-020",
        "Drill diameter (vias and plated holes)",
        format!(
            ">= {} (recommended plated hole >= {})",
            fmt_mm(profile.min_drill_mm),
            fmt_mm(profile.recommended_pth_drill_mm)
        ),
    );
    for v in p.pcb.vias.values() {
        a.measure(
            || format!("via {}", v.id),
            to_mm(v.drill),
            profile.min_drill_mm,
            None,
        );
    }
    for x in pads.iter().filter(|x| x.tht) {
        if let Some(d) = x.drill {
            a.measure(
                || format!("hole {}", x.label()),
                mm(d),
                profile.min_drill_mm,
                Some(profile.recommended_pth_drill_mm),
            );
        }
    }
    checks.push(a.done());

    // JLC-021 annular rings
    let mut a = Acc::new(
        "JLC-021",
        "Annular ring",
        format!(
            "via >= {}, plated pad >= {} (recommended {})",
            fmt_mm(profile.min_via_ring_mm),
            fmt_mm(profile.min_pth_ring_mm),
            fmt_mm(profile.recommended_pth_ring_mm)
        ),
    );
    for v in p.pcb.vias.values() {
        a.measure(
            || format!("via {}", v.id),
            to_mm(v.diameter - v.drill) / 2.0,
            profile.min_via_ring_mm,
            None,
        );
    }
    for x in pads.iter().filter(|x| x.tht) {
        if let Some(d) = x.drill {
            a.measure(
                || format!("pad {}", x.label()),
                mm((2.0 * x.hw.min(x.hh) - d) / 2.0),
                profile.min_pth_ring_mm,
                Some(profile.recommended_pth_ring_mm),
            );
        }
    }
    checks.push(a.done());

    // JLC-022 hole-to-hole
    let mut a = Acc::new(
        "JLC-022",
        "Hole-to-hole spacing (edge to edge)",
        format!(
            "via-via {}, with a plated pad hole {}",
            fmt_mm(profile.min_via_hole_to_hole_mm),
            fmt_mm(profile.min_pad_hole_to_hole_mm)
        ),
    );
    let mut holes: Vec<(String, Point, f64, bool)> = vec![];
    for v in p.pcb.vias.values() {
        holes.push((format!("via {}", v.id), v.position, v.drill as f64, false));
    }
    for x in pads.iter().filter(|x| x.tht) {
        if let Some(d) = x.drill {
            let at = Point {
                x: x.cx as i64,
                y: x.cy as i64,
            };
            holes.push((format!("hole {}", x.label()), at, d, true));
        }
    }
    for i in 0..holes.len() {
        for j in i + 1..holes.len() {
            let gap = holes[i].1.dist(holes[j].1) - (holes[i].2 + holes[j].2) / 2.0;
            let lim = if holes[i].3 || holes[j].3 {
                profile.min_pad_hole_to_hole_mm
            } else {
                profile.min_via_hole_to_hole_mm
            };
            a.measure(
                || format!("{} vs {}", holes[i].0, holes[j].0),
                mm(gap),
                lim,
                None,
            );
        }
    }
    checks.push(a.done());

    // JLC-030 copper to board edge
    let mut a = Acc::new(
        "JLC-030",
        "Copper to routed board edge",
        fmt_mm(profile.min_copper_to_edge_mm),
    );
    if outline_ok {
        for t in p.pcb.tracks.values() {
            let d = edges
                .iter()
                .map(|(e0, e1)| segment_segment_distance(t.a, t.b, *e0, *e1))
                .fold(f64::MAX, f64::min)
                - t.width as f64 / 2.0;
            a.measure(
                || format!("track {} ({:?})", t.id, t.layer),
                mm(d.max(0.0)),
                profile.min_copper_to_edge_mm,
                None,
            );
        }
        for v in p.pcb.vias.values() {
            let d = point_polygon_boundary_distance(v.position, &p.pcb.board.outline)
                - v.diameter as f64 / 2.0;
            a.measure(
                || format!("via {}", v.id),
                mm(d.max(0.0)),
                profile.min_copper_to_edge_mm,
                None,
            );
        }
        for x in &pads {
            let d = edges
                .iter()
                .map(|(e0, e1)| segment_rect(*e0, *e1, x))
                .fold(f64::MAX, f64::min);
            a.measure(
                || format!("pad {}", x.label()),
                mm(d),
                profile.min_copper_to_edge_mm,
                None,
            );
        }
    }
    checks.push(a.done());

    // JLC-040..042 assembly data
    let assembled: Vec<&BomLine> = bom.iter().filter(|l| l.assembled()).collect();
    let missing: Vec<String> = assembled
        .iter()
        .filter(|l| l.lcsc.is_none())
        .map(|l| format!("{} ({})", l.designators.join(","), l.value))
        .collect();
    checks.push(DfmCheck {
        id: "JLC-040".into(),
        title: "Every assembled BOM line has an LCSC part number".into(),
        status: if assembled.is_empty() {
            Status::NotChecked
        } else if missing.is_empty() {
            Status::Pass
        } else {
            Status::Fail
        },
        required: "LCSC Part # on each surface-mount line".into(),
        measured: if assembled.is_empty() {
            "no placed surface-mount parts".into()
        } else {
            format!("{} of {} line(s) lack one", missing.len(), assembled.len())
        },
        detail: missing.into_iter().take(LISTED).collect(),
    });
    let malformed: Vec<String> = bom
        .iter()
        .filter_map(|l| {
            l.lcsc
                .as_deref()
                .filter(|c| !is_lcsc_part_number(c))
                .map(|c| format!("{c:?} on {}", l.designators.join(",")))
        })
        .collect();
    checks.push(DfmCheck {
        id: "JLC-041".into(),
        title: "LCSC part numbers are well formed".into(),
        status: if malformed.is_empty() {
            Status::Pass
        } else {
            Status::Fail
        },
        required: "C followed by digits".into(),
        measured: format!("{} malformed", malformed.len()),
        detail: malformed,
    });
    let unplaced: Vec<String> = bom
        .iter()
        .filter(|l| l.smd && l.placed < l.quantity)
        .map(|l| l.designators.join(","))
        .collect();
    checks.push(DfmCheck {
        id: "JLC-042".into(),
        title: "Every schematic part has a footprint on the PCB".into(),
        status: if unplaced.is_empty() {
            Status::Pass
        } else {
            Status::Warn
        },
        required: "all designators placed".into(),
        measured: format!("{} BOM line(s) with unplaced designators", unplaced.len()),
        detail: unplaced,
    });

    // Rules this software does not evaluate. Listed so they cannot be mistaken for a pass.
    for (id, title, why) in [
        (
            "JLC-050",
            "Basic vs extended assembly parts and fees",
            "needs JLCPCB's assembly parts library, which has no public machine-readable access",
        ),
        (
            "JLC-051",
            "CPL rotation and polarity per package",
            "JLCPCB applies per-package rotation offsets that EAK does not hold; check every part in JLCPCB's viewer",
        ),
        (
            "JLC-052",
            "Silkscreen line width, text height and pad clearance",
            "silkscreen geometry is not measured",
        ),
        (
            "JLC-053",
            "Soldermask bridge and opening clearance",
            "soldermask geometry is not measured",
        ),
        (
            "JLC-054",
            "Plated-slot, NPTH and castellation rules",
            "the EAK-IR has no slots, NPTH or castellated holes",
        ),
        (
            "JLC-055",
            "Copper pour, thermal and acid-trap rules",
            "zone fills are not checked against JLCPCB rules",
        ),
    ] {
        checks.push(not_checked(id, title, why));
    }

    let count = |s: Status| checks.iter().filter(|c| c.status == s).count();
    let failures = count(Status::Fail);
    let warnings = count(Status::Warn);
    let not_checked = count(Status::NotChecked);
    DfmReport {
        statement: format!(
            "PREPARED FOR REVIEW. {failures} failing, {warnings} warning, {not_checked} not-checked rule(s) against {} ({}). A fabricator DFM review is still required.",
            profile.id, profile.version
        ),
        profile: profile.clone(),
        checks,
        failures,
        warnings,
        not_checked,
        limitations: vec![
            "Limits are JLCPCB's published numbers for 2-layer 1 oz FR-4 read on the date above; JLCPCB can change them and applies its own DFM at order time.".into(),
            "Circular and oval pads are measured as their bounding rectangle (conservative).".into(),
            "Copper pours are not included in spacing checks; only tracks, vias and pads are.".into(),
            "Rules marked not_checked are not evaluated and are not passes.".into(),
        ],
    }
}

fn stem(p: &Project) -> String {
    p.name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// JLCPCB pick-and-place file: surface-mount footprints only.
pub fn cpl_csv(p: &Project) -> String {
    let mut rows: Vec<_> = p
        .pcb
        .footprints
        .values()
        .filter(|f| {
            p.library.get(&f.component_id).is_some_and(|d| {
                !d.footprint.pads.is_empty()
                    && d.footprint.pads.iter().all(|x| x.kind == PadKind::Smd)
            })
        })
        .collect();
    rows.sort_by_key(|f| {
        let d: String = f.refdes.chars().filter(|c| c.is_ascii_digit()).collect();
        (
            f.refdes
                .chars()
                .filter(|c| !c.is_ascii_digit())
                .collect::<String>(),
            d.parse::<u64>().unwrap_or(0),
        )
    });
    let mut out = String::from("Designator,Mid X,Mid Y,Layer,Rotation\n");
    for f in rows {
        out.push_str(&format!(
            "{},{:.4}mm,{:.4}mm,{},{}\n",
            f.refdes,
            to_mm(f.position.x),
            to_mm(f.position.y),
            if f.side == Side::Top { "Top" } else { "Bottom" },
            f.rotation.degrees()
        ));
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepManifest {
    pub project_id: String,
    pub project_name: String,
    pub revision: u64,
    pub generator: String,
    pub generated_ms: u128,
    pub provider: String,
    pub statement: String,
    pub package_status: PackageStatus,
    pub dfm_failures: usize,
    pub dfm_warnings: usize,
    pub dfm_not_checked: usize,
    pub files: Vec<FileEntry>,
    pub not_verified: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepResult {
    pub out_dir: String,
    pub manifest: PrepManifest,
    pub package: Manifest,
    pub dfm: DfmReport,
    pub bom: Vec<BomLine>,
}

fn sha(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

/// Write the JLCPCB preparation bundle into `out_dir` (created if absent). Nothing leaves
/// the machine; the caller chooses where the files go.
pub fn prepare(
    p: &Project,
    reports: &[&Report],
    records: &[SourcingRecord],
    out_dir: &Path,
) -> Result<PrepResult, String> {
    let lines = bom::build(p, records);
    let profile = Profile::two_layer_1oz();
    let dfm_report = dfm(p, &lines, &profile);
    // The generic package supplies the Gerber/Excellon files, already self-validated and
    // hashed; the JLCPCB files below are derived from the same project revision.
    let pkg_dir = out_dir.join("eak-package");
    let package = compile(p, reports, &pkg_dir)?;
    let stem = stem(p);

    let mut zip_entries = vec![];
    for f in package
        .files
        .iter()
        .filter(|f| f.kind == "gerber" || f.kind == "drill")
    {
        let data = std::fs::read(pkg_dir.join(&f.name)).map_err(|e| format!("{}: {e}", f.name))?;
        zip_entries.push((f.name.clone(), data));
    }
    let zip = crate::zip::stored(&zip_entries)?;
    let dfm_json = serde_json::to_string_pretty(&dfm_report).map_err(|e| e.to_string())?;
    let outputs: Vec<(String, &str, Vec<u8>)> = vec![
        (format!("{stem}-JLCPCB-gerbers.zip"), "gerber_zip", zip),
        (
            format!("{stem}-JLCPCB-BOM.csv"),
            "bom",
            bom::jlcpcb_csv(&lines).into_bytes(),
        ),
        (
            format!("{stem}-JLCPCB-CPL.csv"),
            "cpl",
            cpl_csv(p).into_bytes(),
        ),
        (
            format!("{stem}-BOM-review.csv"),
            "bom_review",
            bom::detail_csv(&lines).into_bytes(),
        ),
        (
            "jlcpcb-dfm-report.json".into(),
            "dfm",
            dfm_json.into_bytes(),
        ),
    ];
    let mut files = vec![];
    for (name, kind, data) in &outputs {
        let path: PathBuf = out_dir.join(name);
        eak_runtime::store::atomic_write(&path, data).map_err(|e| e.to_string())?;
        let written = std::fs::read(&path).map_err(|e| e.to_string())?;
        files.push(FileEntry {
            name: name.clone(),
            bytes: written.len() as u64,
            sha256: sha(&written),
            kind: (*kind).into(),
        });
    }
    let blocked = dfm_report.failures > 0 || package.status == PackageStatus::Draft;
    let manifest = PrepManifest {
        project_id: p.id.clone(),
        project_name: p.name.clone(),
        revision: p.revision,
        generator: format!(
            "eak-manufacturing {} (jlcpcb prep)",
            env!("CARGO_PKG_VERSION")
        ),
        generated_ms: eak_runtime::store::now_ms(),
        provider: "JLCPCB (preparation only: no upload, no quote, no order)".into(),
        statement: if blocked {
            format!(
                "PREPARED FOR REVIEW — BLOCKED: {} DFM failure(s); package gate status {:?}. Do not send to a fabricator.",
                dfm_report.failures, package.status
            )
        } else {
            "PREPARED FOR REVIEW. Internal checks found no failing rule, but this is NOT manufacturing-ready: rules marked not_checked, independent engineering review and JLCPCB's own DFM are still required. Upload and ordering are manual and not done by EAK.".into()
        },
        package_status: package.status,
        dfm_failures: dfm_report.failures,
        dfm_warnings: dfm_report.warnings,
        dfm_not_checked: dfm_report.not_checked,
        files,
        not_verified: vec![
            "JLCPCB's own DFM / engineering review of these files".into(),
            "Per-package CPL rotation and polarity in JLCPCB's placement viewer".into(),
            "Basic/extended classification, stock and price at order time".into(),
            "Independent electrical engineering review and physical testing".into(),
        ],
    };
    let mjson = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    eak_runtime::store::atomic_write(&out_dir.join("jlcpcb-prep-manifest.json"), mjson.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(PrepResult {
        out_dir: out_dir.display().to_string(),
        manifest,
        package,
        dfm: dfm_report,
        bom: lines,
    })
}
