//! Silkscreen (legend) geometry: component outlines, pin-1 markers and reference-designator
//! text drawn with a built-in stroke font, clipped clear of soldermask openings.
//!
//! The Gerber writer and the DRC both read this one model, so the checked legend is the
//! fabricated legend.
//!
//! Font provenance: the glyph strokes below are an original single-stroke set authored for
//! EAK in this repository (no third-party font data was used or consulted for coordinates);
//! they are covered by the repository's own license. See docs/engineering/IPC_RULE_VALUES.md.

use crate::copper::{pad_shape, Core, Shape};
use eak_core::geom::point_segment_distance;
use eak_core::{
    CopperLayer, Id, Nm, PadKind, Point, Project, Rect, Rotation, Side, DEFAULT_DRAW_STROKE_NM,
};

/// Legend stroke width: 0.16 mm, above the 0.153 mm Fabricator minimum (DFM-010).
pub const LINE_WIDTH: Nm = 160_000;
/// Cap height of reference-designator text.
pub const TEXT_HEIGHT: Nm = 1_000_000;
/// Gap between the courtyard outline and the text baseline.
pub const TEXT_GAP: Nm = 250_000;
/// Pin-1 marker dot diameter.
pub const PIN1_DOT: Nm = 300_000;
/// Minimum ink-to-soldermask-opening distance kept by clipping and checked by DRC.
// shortcut: fixed value because ManufacturingConfig has no silkscreen clearance field;
// upgrade when an ADR adds fabricator-profile silkscreen rules to the IR.
pub const MASK_CLEARANCE: Nm = 100_000;

/// One legend stroke: a round-ended line of `width` (a == b draws a round dot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Stroke {
    pub a: Point,
    pub b: Point,
    pub width: Nm,
}

impl Stroke {
    pub fn shape(&self) -> Shape {
        Shape {
            core: Core::Segment(self.a, self.b),
            radius: self.width as f64 / 2.0,
        }
    }
}

/// A soldermask opening (pad plus mask expansion) on one side.
#[derive(Debug, Clone)]
pub struct Opening {
    pub footprint_id: Id,
    pub refdes: String,
    pub pad: String,
    pub shape: Shape,
}

/// Soldermask openings on `layer`, matching the soldermask Gerber exactly (each pad's own mask
/// expansion, else the board-wide one; ADR-0044).
pub fn mask_openings(p: &Project, layer: CopperLayer) -> Vec<Opening> {
    let board = p.manufacturing.mask_expansion;
    let mut out = Vec::new();
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        for pad in &def.footprint.pads {
            if pad.kind != PadKind::Tht && fp.side.copper() != layer {
                continue;
            }
            let e = pad.mask_expansion_or(board);
            out.push(Opening {
                footprint_id: fp.id.clone(),
                refdes: fp.refdes.clone(),
                pad: pad.number.clone(),
                shape: pad_shape(
                    p.pad_position(fp, pad),
                    fp.rotation,
                    pad.shape,
                    pad.size_x + 2 * e,
                    pad.size_y + 2 * e,
                ),
            });
        }
    }
    out
}

/// Unclipped legend of every footprint on `side`: courtyard outline, pin-1 dot, refdes text.
pub fn raw_legend(p: &Project, side: Side) -> Vec<Stroke> {
    let mut out = Vec::new();
    for fp in p.pcb.footprints.values().filter(|f| f.side == side) {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        let (cw, ch) = fp.rotation.apply_size(
            def.footprint.courtyard_half_w * 2,
            def.footprint.courtyard_half_h * 2,
        );
        let r = Rect::centered(fp.position, cw, ch);
        let c = [
            r.min,
            Point::new(r.max.x, r.min.y),
            r.max,
            Point::new(r.min.x, r.max.y),
        ];
        for i in 0..4 {
            out.push(Stroke {
                a: c[i],
                b: c[(i + 1) % 4],
                width: LINE_WIDTH,
            });
        }
        let dot = Point::new(r.min.x - PIN1_DOT, r.max.y + PIN1_DOT);
        out.push(Stroke {
            a: dot,
            b: dot,
            width: PIN1_DOT,
        });
        let base = Point::new(fp.position.x, r.max.y + TEXT_GAP);
        for (a, b) in text_strokes(&fp.refdes, base, TEXT_HEIGHT, side == Side::Bottom) {
            out.push(Stroke {
                a,
                b,
                width: LINE_WIDTH,
            });
        }
    }
    out.extend(drawing_strokes(p, side));
    out
}

/// Strokes of `text` with cap height `height`, its baseline centred on `at` and turned by `rot`
/// about `at`; `mirror` flips it first (bottom side, read from below).
pub fn placed_text(
    text: &str,
    at: Point,
    height: Nm,
    rot: Rotation,
    mirror: bool,
) -> Vec<(Point, Point)> {
    let turn = |q: Point| at.plus(rot.apply(q.minus(at)));
    text_strokes(text, at, height, mirror)
        .into_iter()
        .map(|(a, b)| (turn(a), turn(b)))
        .collect()
}

/// Legend strokes of the PCB text and dimensions on the silk of `side` (schema v4, ADR-0042).
/// Documentation-layer objects are not fabricated and have none.
pub fn drawing_strokes(p: &Project, side: Side) -> Vec<Stroke> {
    let mirror = side == Side::Bottom;
    let mut out = Vec::new();
    for t in p
        .pcb
        .texts
        .values()
        .filter(|t| t.layer.silk_side() == Some(side))
    {
        for (a, b) in placed_text(&t.text, t.position, t.size, t.rotation, mirror) {
            out.push(Stroke {
                a,
                b,
                width: t.line_width(),
            });
        }
    }
    for d in p
        .pcb
        .dimensions
        .values()
        .filter(|d| d.layer.silk_side() == Some(side))
    {
        let g = d.drawing();
        let label = placed_text(&g.label, g.label_at, g.text_size, g.label_rotation, mirror);
        for (a, b) in g.lines.into_iter().chain(label) {
            out.push(Stroke {
                a,
                b,
                width: DEFAULT_DRAW_STROKE_NM,
            });
        }
    }
    out
}

/// Legend of `side` clipped so no ink comes within `MASK_CLEARANCE` of a mask opening.
pub fn legend(p: &Project, side: Side) -> Vec<Stroke> {
    let openings = mask_openings(p, side.copper());
    raw_legend(p, side)
        .iter()
        .flat_map(|s| clip(s, &openings, MASK_CLEARANCE))
        .collect()
}

/// Distance from `pt` to a shape's core (0 inside a polygon core).
fn core_dist(pt: Point, core: &Core) -> f64 {
    match core {
        Core::Segment(a, b) => point_segment_distance(pt, *a, *b),
        Core::Polygon(poly) => {
            if eak_core::geom::point_in_polygon(pt, poly) {
                0.0
            } else {
                eak_core::geom::point_polygon_boundary_distance(pt, poly)
            }
        }
    }
}

/// Remove the parts of `s` whose ink would come within `clearance` of any opening.
/// Pieces shorter than the stroke width are dropped (they would print as blobs).
pub fn clip(s: &Stroke, openings: &[Opening], clearance: Nm) -> Vec<Stroke> {
    let half = s.width as f64 / 2.0;
    let (ax, ay) = (s.a.x as f64, s.a.y as f64);
    let (dx, dy) = ((s.b.x - s.a.x) as f64, (s.b.y - s.a.y) as f64);
    let len = (dx * dx + dy * dy).sqrt();
    let at = |t: f64| Point::new((ax + t * dx).round() as Nm, (ay + t * dy).round() as Nm);
    let mut cut: Vec<(f64, f64)> = Vec::new();
    let reach = s.shape().bbox().inflate(clearance);
    for o in openings {
        if !o.shape.bbox().intersects(&reach) {
            continue;
        }
        let keep_out = o.shape.radius + half + clearance as f64;
        let f = |t: f64| core_dist(at(t), &o.shape.core) - keep_out;
        if len == 0.0 {
            if f(0.0) < 0.0 {
                return vec![];
            }
            continue;
        }
        // Distance to a convex set is convex along a line: ternary-search its minimum,
        // then bisect each side for the boundary crossings.
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        for _ in 0..100 {
            let m1 = lo + (hi - lo) / 3.0;
            let m2 = hi - (hi - lo) / 3.0;
            if f(m1) <= f(m2) {
                hi = m2;
            } else {
                lo = m1;
            }
        }
        let tmin = (lo + hi) / 2.0;
        if f(tmin) >= 0.0 {
            continue;
        }
        let crossing = |mut inside: f64, mut outside: f64| {
            for _ in 0..64 {
                let m = (inside + outside) / 2.0;
                if f(m) < 0.0 {
                    inside = m;
                } else {
                    outside = m;
                }
            }
            outside
        };
        let t0 = if f(0.0) < 0.0 {
            0.0
        } else {
            crossing(tmin, 0.0)
        };
        let t1 = if f(1.0) < 0.0 {
            1.0
        } else {
            crossing(tmin, 1.0)
        };
        // Widen by 2 nm so rounding to integer nanometres never re-enters the keep-out.
        let pad = 2.0 / len;
        cut.push(((t0 - pad).max(0.0), (t1 + pad).min(1.0)));
    }
    if len == 0.0 {
        return vec![*s];
    }
    cut.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    let mut start = 0.0f64;
    let mut emit = |t0: f64, t1: f64| {
        if (t1 - t0) * len >= s.width as f64 {
            out.push(Stroke {
                a: at(t0),
                b: at(t1),
                width: s.width,
            });
        }
    };
    for (t0, t1) in cut {
        if t0 > start {
            emit(start, t0);
        }
        start = start.max(t1);
    }
    if start < 1.0 {
        emit(start, 1.0);
    }
    out
}

/// Glyph strokes on a 4 × 6 unit grid (x right, y up, baseline at 0).
type Glyph = &'static [&'static [(i8, i8)]];

const BOX: Glyph = &[&[(0, 0), (4, 0), (4, 6), (0, 6), (0, 0)]];
const RING: &[(i8, i8)] = &[
    (1, 0),
    (3, 0),
    (4, 1),
    (4, 5),
    (3, 6),
    (1, 6),
    (0, 5),
    (0, 1),
    (1, 0),
];
const P_BOWL: &[(i8, i8)] = &[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)];

/// Stroke glyph for `ch` (case-insensitive). Unsupported characters draw a hollow box so a
/// missing glyph is visible on the board instead of silently vanishing.
/// Whether the stroke font draws `ch` (anything else is drawn as a hollow box).
pub fn has_glyph(ch: char) -> bool {
    glyph(ch) != BOX
}

pub fn glyph(ch: char) -> Glyph {
    match ch.to_ascii_uppercase() {
        'A' => &[&[(0, 0), (0, 4), (2, 6), (4, 4), (4, 0)], &[(0, 3), (4, 3)]],
        'B' => &[
            &[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)],
            &[(3, 3), (4, 2), (4, 1), (3, 0), (0, 0)],
        ],
        'C' => &[&[
            (4, 1),
            (3, 0),
            (1, 0),
            (0, 1),
            (0, 5),
            (1, 6),
            (3, 6),
            (4, 5),
        ]],
        'D' => &[&[(0, 0), (0, 6), (2, 6), (4, 4), (4, 2), (2, 0), (0, 0)]],
        'E' => &[&[(4, 0), (0, 0), (0, 6), (4, 6)], &[(0, 3), (3, 3)]],
        'F' => &[&[(0, 0), (0, 6), (4, 6)], &[(0, 3), (3, 3)]],
        'G' => &[&[
            (4, 5),
            (3, 6),
            (1, 6),
            (0, 5),
            (0, 1),
            (1, 0),
            (3, 0),
            (4, 1),
            (4, 3),
            (2, 3),
        ]],
        'H' => &[&[(0, 0), (0, 6)], &[(4, 0), (4, 6)], &[(0, 3), (4, 3)]],
        'I' => &[&[(1, 0), (3, 0)], &[(2, 0), (2, 6)], &[(1, 6), (3, 6)]],
        'J' => &[&[(4, 6), (4, 1), (3, 0), (1, 0), (0, 1)]],
        'K' => &[&[(0, 0), (0, 6)], &[(4, 6), (0, 2)], &[(1, 3), (4, 0)]],
        'L' => &[&[(0, 6), (0, 0), (4, 0)]],
        'M' => &[&[(0, 0), (0, 6), (2, 3), (4, 6), (4, 0)]],
        'N' => &[&[(0, 0), (0, 6), (4, 0), (4, 6)]],
        'O' => &[RING],
        'P' => &[P_BOWL],
        'Q' => &[RING, &[(2, 2), (4, 0)]],
        'R' => &[P_BOWL, &[(2, 3), (4, 0)]],
        'S' => &[&[
            (4, 5),
            (3, 6),
            (1, 6),
            (0, 5),
            (0, 4),
            (1, 3),
            (3, 3),
            (4, 2),
            (4, 1),
            (3, 0),
            (1, 0),
            (0, 1),
        ]],
        'T' => &[&[(0, 6), (4, 6)], &[(2, 6), (2, 0)]],
        'U' => &[&[(0, 6), (0, 1), (1, 0), (3, 0), (4, 1), (4, 6)]],
        'V' => &[&[(0, 6), (2, 0), (4, 6)]],
        'W' => &[&[(0, 6), (1, 0), (2, 3), (3, 0), (4, 6)]],
        'X' => &[&[(0, 0), (4, 6)], &[(0, 6), (4, 0)]],
        'Y' => &[&[(0, 6), (2, 3), (4, 6)], &[(2, 3), (2, 0)]],
        'Z' => &[&[(0, 6), (4, 6), (0, 0), (4, 0)]],
        // Slashed zero keeps 0 distinct from O.
        '0' => &[RING, &[(0, 1), (4, 5)]],
        '1' => &[&[(1, 5), (2, 6), (2, 0)], &[(1, 0), (3, 0)]],
        '2' => &[&[(0, 5), (1, 6), (3, 6), (4, 5), (4, 4), (0, 0), (4, 0)]],
        '3' => &[
            &[
                (0, 5),
                (1, 6),
                (3, 6),
                (4, 5),
                (4, 4),
                (3, 3),
                (4, 2),
                (4, 1),
                (3, 0),
                (1, 0),
                (0, 1),
            ],
            &[(1, 3), (3, 3)],
        ],
        '4' => &[&[(3, 0), (3, 6), (0, 2), (4, 2)]],
        '5' => &[&[
            (4, 6),
            (0, 6),
            (0, 3),
            (3, 3),
            (4, 2),
            (4, 1),
            (3, 0),
            (1, 0),
            (0, 1),
        ]],
        '6' => &[&[
            (3, 6),
            (1, 6),
            (0, 5),
            (0, 1),
            (1, 0),
            (3, 0),
            (4, 1),
            (4, 2),
            (3, 3),
            (0, 3),
        ]],
        '7' => &[&[(0, 6), (4, 6), (1, 0)]],
        '8' => &[&[
            (1, 3),
            (0, 4),
            (0, 5),
            (1, 6),
            (3, 6),
            (4, 5),
            (4, 4),
            (3, 3),
            (1, 3),
            (0, 2),
            (0, 1),
            (1, 0),
            (3, 0),
            (4, 1),
            (4, 2),
            (3, 3),
        ]],
        '9' => &[&[
            (1, 0),
            (3, 0),
            (4, 1),
            (4, 5),
            (3, 6),
            (1, 6),
            (0, 5),
            (0, 4),
            (1, 3),
            (4, 3),
        ]],
        '-' => &[&[(1, 3), (3, 3)]],
        '+' => &[&[(0, 3), (4, 3)], &[(2, 1), (2, 5)]],
        '_' => &[&[(0, 0), (4, 0)]],
        '.' => &[&[(2, 0), (2, 0)]],
        '/' => &[&[(0, 0), (4, 6)]],
        ' ' => &[],
        _ => BOX,
    }
}

/// Glyph advance in grid units (4 units of glyph + 2 of spacing).
const ADVANCE: i64 = 6;
const CAP_UNITS: i64 = 6;

/// Width of `text` at cap height `height`.
pub fn text_width(text: &str, height: Nm) -> Nm {
    let n = text.chars().count() as i64;
    if n == 0 {
        return 0;
    }
    (n * ADVANCE - 2) * height / CAP_UNITS
}

/// Centre-lines of `text` set horizontally with its baseline centred on `base`. `mirror`
/// flips it left-to-right so bottom-side text reads correctly when viewed from below.
pub fn text_strokes(text: &str, base: Point, height: Nm, mirror: bool) -> Vec<(Point, Point)> {
    let left = base.x - text_width(text, height) / 2;
    let mut out = Vec::new();
    for (k, ch) in text.chars().enumerate() {
        let x0 = k as i64 * ADVANCE;
        for line in glyph(ch) {
            let pt = |&(gx, gy): &(i8, i8)| {
                let x = left + (x0 + gx as i64) * height / CAP_UNITS;
                let x = if mirror { 2 * base.x - x } else { x };
                Point::new(x, base.y + gy as i64 * height / CAP_UNITS)
            };
            let pts: Vec<Point> = line.iter().map(pt).collect();
            if pts.len() == 1 {
                out.push((pts[0], pts[0]));
            }
            for w in pts.windows(2) {
                out.push((w[0], w[1]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copper::distance;

    fn rect_opening(c: Point, w: Nm, h: Nm) -> Opening {
        Opening {
            footprint_id: "fp".into(),
            refdes: "U1".into(),
            pad: "1".into(),
            shape: pad_shape(c, eak_core::Rotation::R0, eak_core::PadShape::Rect, w, h),
        }
    }

    #[test]
    fn every_refdes_character_has_a_distinct_glyph() {
        let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-+_./";
        for (i, a) in chars.chars().enumerate() {
            assert_ne!(glyph(a), BOX, "{a} has no glyph");
            for b in chars.chars().skip(i + 1) {
                assert_ne!(glyph(a), glyph(b), "{a} and {b} share a glyph");
            }
            for line in glyph(a) {
                for &(x, y) in *line {
                    assert!((0..=4).contains(&x) && (0..=6).contains(&y), "{a}");
                }
            }
        }
        assert_eq!(glyph('r'), glyph('R'));
        assert_eq!(glyph('§'), BOX);
    }

    #[test]
    fn text_is_centred_and_bottom_text_is_mirrored() {
        let base = Point::mm(10.0, 5.0);
        let w = text_width("R8", TEXT_HEIGHT);
        assert_eq!(w, 10 * TEXT_HEIGHT / 6);
        let top = text_strokes("R8", base, TEXT_HEIGHT, false);
        let bot = text_strokes("R8", base, TEXT_HEIGHT, true);
        assert_eq!(top.len(), bot.len());
        let xs = |v: &[(Point, Point)]| {
            let it = v.iter().flat_map(|(a, b)| [a.x, b.x]);
            (it.clone().min().unwrap(), it.max().unwrap())
        };
        assert_eq!(xs(&top), (base.x - w / 2, base.x + w / 2));
        assert_eq!(xs(&bot), xs(&top));
        for ((a, b), (c, d)) in top.iter().zip(&bot) {
            assert_eq!((a.x + c.x, b.x + d.x), (2 * base.x, 2 * base.x));
            assert_eq!((a.y, b.y), (c.y, d.y));
        }
        let ys = top.iter().flat_map(|(a, b)| [a.y, b.y]);
        assert_eq!(ys.clone().min(), Some(base.y));
        assert_eq!(ys.max(), Some(base.y + TEXT_HEIGHT));
    }

    #[test]
    fn clip_removes_ink_over_openings_and_keeps_the_rest() {
        let o = rect_opening(Point::mm(5.0, 0.0), 1_000_000, 1_000_000);
        let s = Stroke {
            a: Point::mm(0.0, 0.0),
            b: Point::mm(10.0, 0.0),
            width: LINE_WIDTH,
        };
        let parts = clip(&s, std::slice::from_ref(&o), MASK_CLEARANCE);
        assert_eq!(parts.len(), 2);
        let keep = 500_000 + LINE_WIDTH / 2 + MASK_CLEARANCE;
        assert!((parts[0].b.x - (5_000_000 - keep)).abs() <= 3);
        assert!((parts[1].a.x - (5_000_000 + keep)).abs() <= 3);
        assert_eq!((parts[0].a, parts[1].b), (s.a, s.b));
        for p in &parts {
            let d = distance(&p.shape(), &o.shape);
            assert!(d >= MASK_CLEARANCE as f64 - 1.0, "{d}");
        }
        // A stroke clear of every opening is unchanged; a dot over a pad disappears.
        let far = Stroke {
            a: Point::mm(0.0, 3.0),
            b: Point::mm(10.0, 3.0),
            width: LINE_WIDTH,
        };
        assert_eq!(
            clip(&far, std::slice::from_ref(&o), MASK_CLEARANCE),
            vec![far]
        );
        let dot = Stroke {
            a: Point::mm(5.0, 0.6),
            b: Point::mm(5.0, 0.6),
            width: PIN1_DOT,
        };
        assert!(clip(&dot, &[o], MASK_CLEARANCE).is_empty());
    }

    #[test]
    fn clip_against_round_and_diagonal_geometry() {
        let o = Opening {
            footprint_id: "fp".into(),
            refdes: "J1".into(),
            pad: "2".into(),
            shape: pad_shape(
                Point::mm(0.0, 0.0),
                eak_core::Rotation::R90,
                eak_core::PadShape::Oval,
                2_000_000,
                1_000_000,
            ),
        };
        let s = Stroke {
            a: Point::mm(-3.0, -2.5),
            b: Point::mm(3.0, 2.0),
            width: LINE_WIDTH,
        };
        let parts = clip(&s, std::slice::from_ref(&o), MASK_CLEARANCE);
        assert_eq!(parts.len(), 2);
        for p in &parts {
            assert!(distance(&p.shape(), &o.shape) >= MASK_CLEARANCE as f64 - 1.0);
        }
        // A stroke ending inside the keep-out is shortened, not split.
        let end_in = Stroke {
            a: Point::mm(-3.0, 0.0),
            b: Point::mm(0.0, 0.0),
            width: LINE_WIDTH,
        };
        let parts = clip(&end_in, &[o], MASK_CLEARANCE);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].a, end_in.a);
    }

    #[test]
    fn pcb_text_and_dimensions_join_the_legend_of_their_side_only() {
        use eak_core::{DimensionUnit, DrawLayer, PcbDimension, PcbText};
        let mut p = Project::new("legend");
        let text = |id: &str, layer| PcbText {
            id: id.into(),
            layer,
            position: Point::mm(10.0, 10.0),
            text: "AB".into(),
            size: TEXT_HEIGHT,
            stroke_width: 0,
            rotation: Rotation::R0,
        };
        for (id, layer) in [
            ("top", DrawLayer::TopSilk),
            ("bot", DrawLayer::BottomSilk),
            ("doc", DrawLayer::Documentation),
        ] {
            p.pcb.texts.insert(id.into(), text(id, layer));
        }
        p.pcb.dimensions.insert(
            "dim".into(),
            PcbDimension {
                id: "dim".into(),
                layer: DrawLayer::TopSilk,
                a: Point::mm(0.0, 0.0),
                b: Point::mm(20.0, 0.0),
                offset: 2_000_000,
                unit: DimensionUnit::Mm,
                precision: 1,
                text_size: 0,
            },
        );
        let ab = text_strokes("AB", Point::mm(10.0, 10.0), TEXT_HEIGHT, false).len();
        let label = text_strokes("20.0 mm", Point::default(), TEXT_HEIGHT, false).len();
        let top = drawing_strokes(&p, Side::Top);
        assert_eq!(top.len(), ab + 7 + label);
        assert!(top.iter().all(|s| s.width == DEFAULT_DRAW_STROKE_NM));
        let bottom = drawing_strokes(&p, Side::Bottom);
        assert_eq!(bottom.len(), ab);
        // Bottom text is mirrored about its anchor: the first stroke lands right of it.
        assert!(bottom[0].a.x > Point::mm(10.0, 10.0).x);
        assert_eq!(raw_legend(&p, Side::Top).len(), top.len());
        // A quarter turn stands the text up about its anchor.
        let up = placed_text(
            "AB",
            Point::mm(10.0, 10.0),
            TEXT_HEIGHT,
            Rotation::R90,
            false,
        );
        assert!(up.iter().all(|(a, _)| a.x <= Point::mm(10.0, 10.0).x));
        assert!(has_glyph('A') && has_glyph(' ') && !has_glyph('§'));
    }
}
