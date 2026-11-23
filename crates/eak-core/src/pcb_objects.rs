//! PCB objects of schema v4 (ADR-0042): arc geometry for tracks and board edges, keep-out and
//! constraint regions, board cut-outs, PCB text and dimensions.
//!
//! Arcs are stored exactly (end points and a sweep angle) and flattened with a bounded chord
//! error wherever a polygon or a segment is needed (DRC, zone fill, renderers); the Gerber writer
//! emits them as true circular interpolation.

use crate::geom::{polygon_self_intersects, Nm, Point, Rotation};
use crate::ir::{CopperLayer, Id, Side};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Largest distance between a flattened arc and the true arc (10 µm).
pub const ARC_FLATTEN_ERR_NM: f64 = 10_000.0;
/// Most segments one flattened arc is split into.
pub const MAX_ARC_SEGMENTS: usize = 256;
/// Stroke width used for PCB text and dimensions that do not set one (the 0.16 mm legend width).
pub const DEFAULT_DRAW_STROKE_NM: Nm = 160_000;
/// Character height of a dimension label that does not set one.
pub const DEFAULT_DIMENSION_TEXT_NM: Nm = 1_000_000;
/// Most decimals a dimension label may show.
pub const MAX_DIMENSION_PRECISION: u8 = 6;
/// Most characters of one PCB text.
pub const MAX_PCB_TEXT_CHARS: usize = 200;

/// Whether `sweep_deg` is a usable arc sweep: finite, non-zero and less than a full turn.
pub fn valid_sweep(sweep_deg: f64) -> bool {
    sweep_deg.is_finite() && sweep_deg != 0.0 && sweep_deg.abs() < 360.0
}

/// Centre `(x, y)` in nanometres of the arc from `a` to `b` that sweeps `sweep_deg` degrees
/// (positive counter-clockwise, Y up). `None` for a zero-length chord or an invalid sweep.
pub fn arc_center(a: Point, b: Point, sweep_deg: f64) -> Option<(f64, f64)> {
    if a == b || !valid_sweep(sweep_deg) {
        return None;
    }
    let (ax, ay, bx, by) = (a.x as f64, a.y as f64, b.x as f64, b.y as f64);
    let (dx, dy) = (bx - ax, by - ay);
    let c = (dx * dx + dy * dy).sqrt();
    // The centre lies on the chord's perpendicular bisector, left of a→b for a
    // counter-clockwise sweep below a half turn.
    let h = (c / 2.0) / (sweep_deg.to_radians() / 2.0).tan();
    let (nx, ny) = (-dy / c, dx / c);
    Some(((ax + bx) / 2.0 + nx * h, (ay + by) / 2.0 + ny * h))
}

/// Radius in nanometres of that arc.
pub fn arc_radius(a: Point, b: Point, sweep_deg: f64) -> Option<f64> {
    if a == b || !valid_sweep(sweep_deg) {
        return None;
    }
    Some(a.dist(b) / (2.0 * (sweep_deg.to_radians() / 2.0).sin().abs()))
}

/// Length of that arc in nanometres (the chord when the sweep is invalid).
pub fn arc_length(a: Point, b: Point, sweep_deg: f64) -> f64 {
    match arc_radius(a, b, sweep_deg) {
        Some(r) => r * sweep_deg.to_radians().abs(),
        None => a.dist(b),
    }
}

/// The point halfway along that arc (the chord midpoint when the sweep is invalid).
pub fn arc_mid(a: Point, b: Point, sweep_deg: f64) -> Point {
    match (arc_center(a, b, sweep_deg), arc_radius(a, b, sweep_deg)) {
        (Some((cx, cy)), Some(r)) => {
            let t = (a.y as f64 - cy).atan2(a.x as f64 - cx) + sweep_deg.to_radians() / 2.0;
            Point::new(
                (cx + r * t.cos()).round() as Nm,
                (cy + r * t.sin()).round() as Nm,
            )
        }
        _ => Point::new((a.x + b.x) / 2, (a.y + b.y) / 2),
    }
}

/// The arc from `a` to `b` as a polyline whose chords stay within `max_err` nanometres of the
/// arc. The first and last points are exactly `a` and `b`; an invalid arc is the chord `[a, b]`.
pub fn arc_points(a: Point, b: Point, sweep_deg: f64, max_err: f64) -> Vec<Point> {
    let (Some((cx, cy)), Some(r)) = (arc_center(a, b, sweep_deg), arc_radius(a, b, sweep_deg))
    else {
        return vec![a, b];
    };
    let theta = sweep_deg.to_radians();
    // A chord spanning angle s deviates r(1 - cos(s/2)) from the arc.
    let step = if max_err > 0.0 && max_err < r {
        2.0 * (1.0 - max_err / r).acos()
    } else {
        theta.abs()
    };
    let n = if step > 0.0 {
        ((theta.abs() / step).ceil() as usize).clamp(1, MAX_ARC_SEGMENTS)
    } else {
        MAX_ARC_SEGMENTS
    };
    let a0 = (a.y as f64 - cy).atan2(a.x as f64 - cx);
    let mut out = Vec::with_capacity(n + 1);
    out.push(a);
    for k in 1..n {
        let t = a0 + theta * k as f64 / n as f64;
        out.push(Point::new(
            (cx + r * t.cos()).round() as Nm,
            (cy + r * t.sin()).round() as Nm,
        ));
    }
    out.push(b);
    out.dedup();
    out
}

/// One arc edge of the board outline: the edge from `outline[edge]` to `outline[edge + 1]` (the
/// last edge closes the polygon) sweeps `sweep_deg` degrees, positive counter-clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OutlineArc {
    pub edge: usize,
    pub sweep_deg: f64,
}

/// A closed outline with some arc edges, flattened to a polygon (the vertices themselves are
/// kept exactly).
pub fn flatten_outline(outline: &[Point], arcs: &[OutlineArc]) -> Vec<Point> {
    if arcs.is_empty() || outline.len() < 2 {
        return outline.to_vec();
    }
    let n = outline.len();
    let mut out = Vec::new();
    for (i, &a) in outline.iter().enumerate() {
        let b = outline[(i + 1) % n];
        match arcs.iter().find(|e| e.edge == i) {
            Some(e) => {
                let pts = arc_points(a, b, e.sweep_deg, ARC_FLATTEN_ERR_NM);
                // Every point but the last (it is the next edge's first vertex).
                let keep = pts.len().saturating_sub(1).max(1);
                out.extend(pts.iter().copied().take(keep));
            }
            None => out.push(a),
        }
    }
    out
}

/// Problems with the arc edges of an outline; empty when they are valid.
pub fn validate_outline_arcs(outline: &[Point], arcs: &[OutlineArc]) -> Vec<String> {
    let mut e = Vec::new();
    let mut seen = BTreeSet::new();
    for a in arcs {
        if a.edge >= outline.len() {
            e.push(format!(
                "outline arc on edge {} but the outline has {} edges",
                a.edge,
                outline.len()
            ));
        }
        if !valid_sweep(a.sweep_deg) {
            e.push(format!(
                "outline arc on edge {} needs a sweep between -360 and 360 degrees other than 0",
                a.edge
            ));
        }
        if !seen.insert(a.edge) {
            e.push(format!("outline edge {} has more than one arc", a.edge));
        }
    }
    if e.is_empty() && outline.len() >= 3 && !arcs.is_empty() {
        let flat = flatten_outline(outline, arcs);
        if polygon_self_intersects(&flat) {
            e.push("the outline with its arc edges crosses itself".into());
        }
    }
    e
}

/// Problems that make `outline` unusable as a closed region boundary.
fn polygon_problems(what: &str, outline: &[Point]) -> Vec<String> {
    if outline.len() < 3 || polygon_self_intersects(outline) {
        vec![format!(
            "{what} outline must be a simple polygon with at least 3 vertices"
        )]
    } else {
        Vec::new()
    }
}

/// Non-copper drawing layer for PCB text and dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawLayer {
    /// Top legend (fabricated).
    TopSilk,
    /// Bottom legend (fabricated, mirrored).
    BottomSilk,
    /// Documentation drawing (shown in the editor, not fabricated).
    Documentation,
}

impl DrawLayer {
    /// The board side whose legend this layer is, or `None` for documentation.
    pub fn silk_side(self) -> Option<Side> {
        match self {
            DrawLayer::TopSilk => Some(Side::Top),
            DrawLayer::BottomSilk => Some(Side::Bottom),
            DrawLayer::Documentation => None,
        }
    }
}

/// Kind of object a keep-out region forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionObject {
    Tracks,
    Vias,
    /// Footprint pads.
    Components,
    /// Copper pour (zone outlines and fill).
    Zones,
}

/// A keep-out and/or constraint region on some copper layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub id: Id,
    #[serde(default)]
    pub name: String,
    pub outline: Vec<Point>,
    /// Copper layers the region applies to; empty = every layer.
    #[serde(default)]
    pub layers: Vec<CopperLayer>,
    /// Objects forbidden inside the region (a keep-out when not empty).
    #[serde(default)]
    pub forbid: BTreeSet<RegionObject>,
    /// Narrowest track allowed inside (constraint region). Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_track_width: Option<Nm>,
    /// Widest track allowed inside (constraint region). Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_track_width: Option<Nm>,
}

impl Region {
    pub fn applies_to(&self, layer: CopperLayer) -> bool {
        self.layers.is_empty() || self.layers.contains(&layer)
    }
    pub fn forbids(&self, o: RegionObject) -> bool {
        self.forbid.contains(&o)
    }
    pub fn is_keepout(&self) -> bool {
        !self.forbid.is_empty()
    }
    /// Display name: the given name, or "keep-out" / "constraint region" and the id.
    pub fn label(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.clone();
        }
        let kind = if self.is_keepout() {
            "keep-out"
        } else {
            "constraint region"
        };
        format!("{kind} {}", self.id)
    }

    /// Problems that make this region unusable; empty when it is valid.
    pub fn validate(&self) -> Vec<String> {
        let mut e = polygon_problems("region", &self.outline);
        if self.forbid.is_empty()
            && self.min_track_width.is_none()
            && self.max_track_width.is_none()
        {
            e.push("a region needs at least one keep-out object or a track-width rule".into());
        }
        for (name, w) in [
            ("minimum", self.min_track_width),
            ("maximum", self.max_track_width),
        ] {
            if w.is_some_and(|w| w <= 0) {
                e.push(format!("region {name} track width must be positive"));
            }
        }
        if let (Some(lo), Some(hi)) = (self.min_track_width, self.max_track_width) {
            if lo > hi {
                e.push("region minimum track width exceeds its maximum".into());
            }
        }
        let unique: BTreeSet<&CopperLayer> = self.layers.iter().collect();
        if unique.len() != self.layers.len() {
            e.push("region lists a layer more than once".into());
        }
        e
    }
}

/// A hole milled through the board (an inner board edge).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cutout {
    pub id: Id,
    pub outline: Vec<Point>,
}

impl Cutout {
    pub fn validate(&self) -> Vec<String> {
        polygon_problems("cut-out", &self.outline)
    }
}

/// Free text on a silk or documentation layer, drawn with the built-in stroke font and anchored
/// at the centre of its baseline. Bottom-silk text is mirrored so it reads from below.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcbText {
    pub id: Id,
    pub layer: DrawLayer,
    pub position: Point,
    pub text: String,
    /// Character (cap) height.
    pub size: Nm,
    /// Stroke width; 0 = [`DEFAULT_DRAW_STROKE_NM`].
    #[serde(default)]
    pub stroke_width: Nm,
    #[serde(default)]
    pub rotation: Rotation,
}

impl PcbText {
    pub fn line_width(&self) -> Nm {
        if self.stroke_width > 0 {
            self.stroke_width
        } else {
            DEFAULT_DRAW_STROKE_NM
        }
    }

    pub fn validate(&self) -> Vec<String> {
        text_problems(&self.text, self.size, self.stroke_width)
    }
}

fn text_problems(text: &str, size: Nm, stroke: Nm) -> Vec<String> {
    let mut e = Vec::new();
    if text.trim().is_empty() {
        e.push("text is empty".into());
    }
    if text.chars().count() > MAX_PCB_TEXT_CHARS {
        e.push(format!(
            "text is longer than {MAX_PCB_TEXT_CHARS} characters"
        ));
    }
    if text.chars().any(char::is_control) {
        e.push("text has a control character (one line only)".into());
    }
    if size <= 0 {
        e.push("text size must be positive".into());
    }
    if stroke < 0 {
        e.push("stroke width must not be negative".into());
    }
    e
}

/// Unit a dimension label is shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DimensionUnit {
    #[default]
    Mm,
    Mil,
    Inch,
}

impl DimensionUnit {
    pub fn suffix(self) -> &'static str {
        match self {
            DimensionUnit::Mm => "mm",
            DimensionUnit::Mil => "mil",
            DimensionUnit::Inch => "in",
        }
    }
    /// Nanometres in one unit.
    pub fn nm(self) -> f64 {
        match self {
            DimensionUnit::Mm => 1_000_000.0,
            DimensionUnit::Mil => 25_400.0,
            DimensionUnit::Inch => 25_400_000.0,
        }
    }
}

fn default_precision() -> u8 {
    2
}

/// An aligned linear dimension measuring `a`-`b`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcbDimension {
    pub id: Id,
    pub layer: DrawLayer,
    pub a: Point,
    pub b: Point,
    /// Distance of the dimension line from `a`-`b`, positive to the left of a→b.
    #[serde(default)]
    pub offset: Nm,
    #[serde(default)]
    pub unit: DimensionUnit,
    /// Decimals shown (0..=[`MAX_DIMENSION_PRECISION`]).
    #[serde(default = "default_precision")]
    pub precision: u8,
    /// Label character height; 0 = [`DEFAULT_DIMENSION_TEXT_NM`].
    #[serde(default)]
    pub text_size: Nm,
}

/// Lines and label placement of a dimension, ready to stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct DimensionDrawing {
    pub lines: Vec<(Point, Point)>,
    /// Baseline centre of the label.
    pub label_at: Point,
    pub label_rotation: Rotation,
    pub label: String,
    pub text_size: Nm,
}

impl PcbDimension {
    /// Measured length in nanometres.
    pub fn length_nm(&self) -> f64 {
        self.a.dist(self.b)
    }

    /// Label text: the length in the dimension's unit, e.g. `12.50 mm`.
    pub fn label(&self) -> String {
        format!(
            "{:.*} {}",
            usize::from(self.precision.min(MAX_DIMENSION_PRECISION)),
            self.length_nm() / self.unit.nm(),
            self.unit.suffix()
        )
    }

    pub fn text_height(&self) -> Nm {
        if self.text_size > 0 {
            self.text_size
        } else {
            DEFAULT_DIMENSION_TEXT_NM
        }
    }

    /// Dimension line, extension lines, arrowheads and label placement.
    pub fn drawing(&self) -> DimensionDrawing {
        let len = self.length_nm().max(1.0);
        let (ux, uy) = (
            (self.b.x - self.a.x) as f64 / len,
            (self.b.y - self.a.y) as f64 / len,
        );
        let (nx, ny) = (-uy, ux);
        let at = |p: Point, along: f64, across: f64| {
            Point::new(
                (p.x as f64 + ux * along + nx * across).round() as Nm,
                (p.y as f64 + uy * along + ny * across).round() as Nm,
            )
        };
        let off = self.offset as f64;
        let (a2, b2) = (at(self.a, 0.0, off), at(self.b, 0.0, off));
        let mut lines = vec![(a2, b2)];
        // Extension lines run from the measured points to just past the dimension line.
        if self.offset != 0 {
            let past = 500_000.0 * off.signum();
            lines.push((self.a, at(self.a, 0.0, off + past)));
            lines.push((self.b, at(self.b, 0.0, off + past)));
        }
        let arrow = (len / 4.0).min(1_000_000.0);
        let wing = arrow * 0.35;
        for (tip, dir) in [(a2, 1.0), (b2, -1.0)] {
            lines.push((tip, at(tip, dir * arrow, wing)));
            lines.push((tip, at(tip, dir * arrow, -wing)));
        }
        let side = if self.offset < 0 { -1.0 } else { 1.0 };
        let mid = Point::new((a2.x + b2.x) / 2, (a2.y + b2.y) / 2);
        let label_rotation = if (self.b.x - self.a.x).abs() >= (self.b.y - self.a.y).abs() {
            Rotation::R0
        } else {
            Rotation::R90
        };
        DimensionDrawing {
            lines,
            label_at: at(mid, 0.0, side * 400_000.0),
            label_rotation,
            label: self.label(),
            text_size: self.text_height(),
        }
    }

    pub fn validate(&self) -> Vec<String> {
        let mut e = Vec::new();
        if self.a == self.b {
            e.push("dimension has zero length".into());
        }
        if self.precision > MAX_DIMENSION_PRECISION {
            e.push(format!(
                "dimension precision must be 0 to {MAX_DIMENSION_PRECISION} decimals"
            ));
        }
        if self.text_size < 0 {
            e.push("dimension text size must not be negative".into());
        }
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_arc_has_the_expected_centre_radius_and_midpoint() {
        let (a, b) = (Point::new(10_000_000, 0), Point::new(0, 10_000_000));
        let (cx, cy) = arc_center(a, b, 90.0).unwrap();
        assert!(cx.abs() < 1.0 && cy.abs() < 1.0, "{cx} {cy}");
        assert!((arc_radius(a, b, 90.0).unwrap() - 10_000_000.0).abs() < 1.0);
        let m = arc_mid(a, b, 90.0);
        assert!(
            (m.x - 7_071_068).abs() <= 1 && (m.y - 7_071_068).abs() <= 1,
            "{m:?}"
        );
        // Clockwise the long way round: the same circle.
        let (cx, cy) = arc_center(a, b, -270.0).unwrap();
        assert!(cx.abs() < 1.0 && cy.abs() < 1.0, "{cx} {cy}");
        let len = arc_length(a, b, 90.0);
        assert!((len - std::f64::consts::FRAC_PI_2 * 10_000_000.0).abs() < 1.0);
    }

    #[test]
    fn flattened_arcs_keep_their_ends_and_stay_within_the_chord_error() {
        let (a, b) = (Point::new(-5_000_000, 0), Point::new(5_000_000, 0));
        for sweep in [30.0, 90.0, 180.0, -180.0, 300.0] {
            let pts = arc_points(a, b, sweep, ARC_FLATTEN_ERR_NM);
            assert_eq!((pts[0], *pts.last().unwrap()), (a, b), "{sweep}");
            assert!(
                pts.len() > 2 && pts.len() <= MAX_ARC_SEGMENTS + 1,
                "{sweep}"
            );
            let (cx, cy) = arc_center(a, b, sweep).unwrap();
            let r = arc_radius(a, b, sweep).unwrap();
            for w in pts.windows(2) {
                let mx = (w[0].x + w[1].x) as f64 / 2.0 - cx;
                let my = (w[0].y + w[1].y) as f64 / 2.0 - cy;
                assert!(
                    r - (mx * mx + my * my).sqrt() <= ARC_FLATTEN_ERR_NM + 2.0,
                    "{sweep}"
                );
            }
        }
        // A positive half turn from (-5,0) to (5,0) runs below the chord (counter-clockwise).
        let below = arc_points(a, b, 180.0, ARC_FLATTEN_ERR_NM);
        assert!(below.iter().all(|p| p.y <= 0));
        assert_eq!(arc_points(a, b, 0.0, ARC_FLATTEN_ERR_NM), vec![a, b]);
        assert!(!valid_sweep(360.0) && !valid_sweep(f64::NAN) && valid_sweep(-359.0));
    }

    #[test]
    fn outline_with_an_arc_edge_flattens_and_validates() {
        let sq = vec![
            Point::new(0, 0),
            Point::new(10_000_000, 0),
            Point::new(10_000_000, 10_000_000),
            Point::new(0, 10_000_000),
        ];
        let arcs = vec![OutlineArc {
            edge: 1,
            sweep_deg: 90.0,
        }];
        let flat = flatten_outline(&sq, &arcs);
        assert!(flat.len() > sq.len());
        assert!(flat.contains(&sq[1]) && flat.contains(&sq[2]));
        assert!(validate_outline_arcs(&sq, &arcs).is_empty());
        let bad = [
            OutlineArc {
                edge: 4,
                sweep_deg: 90.0,
            },
            OutlineArc {
                edge: 0,
                sweep_deg: 0.0,
            },
        ];
        assert_eq!(validate_outline_arcs(&sq, &bad).len(), 2);
        // A thin board whose bottom edge bulges inward past its top edge crosses itself.
        let thin = vec![
            Point::new(0, 0),
            Point::new(10_000_000, 0),
            Point::new(10_000_000, 2_000_000),
            Point::new(0, 2_000_000),
        ];
        let inward = [OutlineArc {
            edge: 0,
            sweep_deg: -180.0,
        }];
        assert!(!validate_outline_arcs(&thin, &inward).is_empty());
        assert_eq!(flatten_outline(&sq, &[]), sq);
    }

    #[test]
    fn regions_need_a_rule_and_a_simple_outline() {
        let mut r = Region {
            id: "rg".into(),
            name: String::new(),
            outline: vec![Point::new(0, 0), Point::new(5, 0), Point::new(5, 5)],
            layers: vec![],
            forbid: BTreeSet::new(),
            min_track_width: None,
            max_track_width: None,
        };
        assert_eq!(r.validate().len(), 1, "no rule");
        r.forbid.insert(RegionObject::Tracks);
        assert!(r.validate().is_empty());
        assert!(r.is_keepout() && r.applies_to(CopperLayer::Inner(3)));
        assert_eq!(r.label(), "keep-out rg");
        r.layers = vec![CopperLayer::Top, CopperLayer::Top];
        r.min_track_width = Some(300);
        r.max_track_width = Some(200);
        assert_eq!(r.validate().len(), 2, "{:?}", r.validate());
        let v = serde_json::to_value(Region {
            layers: vec![CopperLayer::Bottom],
            min_track_width: None,
            max_track_width: None,
            ..r
        })
        .unwrap();
        assert_eq!(v["forbid"], serde_json::json!(["tracks"]));
        assert!(v.get("min_track_width").is_none());
    }

    #[test]
    fn dimension_label_and_drawing() {
        let d = PcbDimension {
            id: "dm".into(),
            layer: DrawLayer::Documentation,
            a: Point::new(0, 0),
            b: Point::new(25_400_000, 0),
            offset: 2_000_000,
            unit: DimensionUnit::Mm,
            precision: 2,
            text_size: 0,
        };
        assert_eq!(d.label(), "25.40 mm");
        let inch = PcbDimension {
            unit: DimensionUnit::Inch,
            precision: 3,
            ..d.clone()
        };
        assert_eq!(inch.label(), "1.000 in");
        let g = d.drawing();
        assert_eq!(
            g.lines[0],
            (Point::new(0, 2_000_000), Point::new(25_400_000, 2_000_000))
        );
        // Dimension line, two extension lines, two arrowheads of two strokes each.
        assert_eq!(g.lines.len(), 7);
        assert_eq!(g.label_rotation, Rotation::R0);
        assert!(g.label_at.y > 2_000_000);
        let parsed: PcbDimension = serde_json::from_str(
            r#"{"id":"d","layer":"top_silk","a":{"x":0,"y":0},"b":{"x":0,"y":5}}"#,
        )
        .unwrap();
        assert_eq!(
            (parsed.precision, parsed.unit, parsed.offset),
            (2, DimensionUnit::Mm, 0)
        );
        assert_eq!(parsed.drawing().label_rotation, Rotation::R90);
        let bad = PcbDimension {
            b: d.a,
            precision: 9,
            ..d
        };
        assert_eq!(bad.validate().len(), 2);
    }

    #[test]
    fn text_validation() {
        let t = PcbText {
            id: "tx".into(),
            layer: DrawLayer::TopSilk,
            position: Point::default(),
            text: "REV A".into(),
            size: 1_000_000,
            stroke_width: 0,
            rotation: Rotation::R0,
        };
        assert!(t.validate().is_empty());
        assert_eq!(t.line_width(), DEFAULT_DRAW_STROKE_NM);
        let bad = PcbText {
            text: "a\nb".into(),
            size: 0,
            stroke_width: -1,
            ..t
        };
        assert_eq!(bad.validate().len(), 3);
        assert_eq!(DrawLayer::BottomSilk.silk_side(), Some(Side::Bottom));
        assert_eq!(DrawLayer::Documentation.silk_side(), None);
    }
}
