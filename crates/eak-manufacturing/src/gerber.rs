//! RS-274X (Gerber X2 attributes) writer and a structural re-reader for validation.

use eak_core::{arc_center, CopperLayer, Nm, PadKind, PadShape, Point, Project, Side};
use eak_engineering::{copper, silk};
use std::collections::BTreeMap;
use std::fmt::Write;

fn coord(v: Nm) -> String {
    // Format 4.6 in mm: integer = nm (1e-6 mm).
    format!("{v}")
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Ap {
    Circle(Nm),
    Rect(Nm, Nm),
    Obround(Nm, Nm),
}

struct Writer {
    body: String,
    apertures: BTreeMap<Ap, u32>,
    current: Option<u32>,
    /// Whether the body has circular arcs (the header then sets multi-quadrant mode).
    arcs: bool,
}

impl Writer {
    fn new() -> Self {
        Writer {
            body: String::new(),
            apertures: BTreeMap::new(),
            current: None,
            arcs: false,
        }
    }
    fn select(&mut self, ap: Ap) {
        let next = 10 + self.apertures.len() as u32;
        let d = *self.apertures.entry(ap).or_insert(next);
        if self.current != Some(d) {
            let _ = writeln!(self.body, "D{d}*");
            self.current = Some(d);
        }
    }
    fn flash(&mut self, ap: Ap, at: Point) {
        self.select(ap);
        let _ = writeln!(self.body, "X{}Y{}D03*", coord(at.x), coord(at.y));
    }
    fn line(&mut self, width: Nm, a: Point, b: Point) {
        self.select(Ap::Circle(width));
        let _ = writeln!(self.body, "X{}Y{}D02*", coord(a.x), coord(a.y));
        let _ = writeln!(self.body, "X{}Y{}D01*", coord(b.x), coord(b.y));
    }
    /// A circular arc drawn with a round aperture from `a` to `b`, sweeping `sweep_deg`
    /// (positive counter-clockwise, Y up) in multi-quadrant mode: G03 (CCW) or G02 (CW) with
    /// the centre as I/J offsets from `a`. Falls back to a straight line for a degenerate arc.
    fn arc(&mut self, width: Nm, a: Point, b: Point, sweep_deg: f64) {
        let Some((cx, cy)) = arc_center(a, b, sweep_deg) else {
            self.line(width, a, b);
            return;
        };
        self.arcs = true;
        self.select(Ap::Circle(width));
        let (i, j) = (
            (cx - a.x as f64).round() as Nm,
            (cy - a.y as f64).round() as Nm,
        );
        let g = if sweep_deg > 0.0 { "G03" } else { "G02" };
        let _ = writeln!(self.body, "X{}Y{}D02*", coord(a.x), coord(a.y));
        let _ = writeln!(self.body, "{g}*");
        let _ = writeln!(
            self.body,
            "X{}Y{}I{}J{}D01*",
            coord(b.x),
            coord(b.y),
            coord(i),
            coord(j)
        );
        self.body.push_str("G01*\n");
    }
    fn region(&mut self, pts: &[Point]) {
        if pts.len() < 3 {
            return;
        }
        self.body.push_str("G36*\n");
        let _ = writeln!(self.body, "X{}Y{}D02*", coord(pts[0].x), coord(pts[0].y));
        for p in &pts[1..] {
            let _ = writeln!(self.body, "X{}Y{}D01*", coord(p.x), coord(p.y));
        }
        let _ = writeln!(self.body, "X{}Y{}D01*", coord(pts[0].x), coord(pts[0].y));
        self.body.push_str("G37*\n");
    }
    fn finish(self, function: &str, polarity: &str, project: &Project) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "%TF.GenerationSoftware,Tusk Innovations,EAK,{}*%",
            env!("CARGO_PKG_VERSION")
        );
        let _ = writeln!(
            s,
            "%TF.ProjectId,{},{},r{}*%",
            sanitize(&project.name),
            project.id,
            project.revision
        );
        let _ = writeln!(s, "%TF.FileFunction,{function}*%");
        let _ = writeln!(s, "%TF.FilePolarity,{polarity}*%");
        s.push_str("%FSLAX46Y46*%\n%MOMM*%\n%LPD*%\nG01*\n");
        if self.arcs {
            s.push_str("G75*\n");
        }
        for (ap, d) in &self.apertures {
            let mm = |v: Nm| format!("{:.6}", v as f64 / 1e6);
            match ap {
                Ap::Circle(w) => {
                    let _ = writeln!(s, "%ADD{d}C,{}*%", mm(*w));
                }
                Ap::Rect(w, h) => {
                    let _ = writeln!(s, "%ADD{d}R,{}X{}*%", mm(*w), mm(*h));
                }
                Ap::Obround(w, h) => {
                    let _ = writeln!(s, "%ADD{d}O,{}X{}*%", mm(*w), mm(*h));
                }
            }
        }
        s.push_str(&self.body);
        s.push_str("M02*\n");
        s
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect()
}

fn pad_ap(shape: PadShape, w: Nm, h: Nm) -> Ap {
    match shape {
        PadShape::Circle => Ap::Circle(w.min(h)),
        PadShape::Rect => Ap::Rect(w, h),
        PadShape::Oval => Ap::Obround(w, h),
    }
}

/// Visit every pad on the given copper side: (position, aperture shape, w, h, kind).
fn for_pads(p: &Project, layer: CopperLayer, mut f: impl FnMut(Point, PadShape, Nm, Nm, PadKind)) {
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        for pad in &def.footprint.pads {
            let on = pad.kind == PadKind::Tht || fp.side.copper() == layer;
            if !on {
                continue;
            }
            let (w, h) = fp.rotation.apply_size(pad.size_x, pad.size_y);
            f(p.pad_position(fp, pad), pad.shape, w, h, pad.kind);
        }
    }
}

pub fn copper_layer(p: &Project, layer: CopperLayer) -> String {
    let mut w = Writer::new();
    // Zones: emit the raster fill (with thermal reliefs) as merged horizontal runs.
    let items = copper::items(p);
    for zid in p.pcb.zones.keys() {
        let Some(fill) = copper::fill_zone(p, zid, &items) else {
            continue;
        };
        if fill.layer != layer {
            continue;
        }
        for r in 0..fill.rows {
            let mut c = 0;
            while c < fill.cols {
                if fill.cells[r * fill.cols + c] == 0 {
                    c += 1;
                    continue;
                }
                let start = c;
                while c < fill.cols && fill.cells[r * fill.cols + c] != 0 {
                    c += 1;
                }
                let x0 = fill.origin.x + start as Nm * fill.pitch;
                let x1 = fill.origin.x + c as Nm * fill.pitch;
                let y0 = fill.origin.y + r as Nm * fill.pitch;
                let y1 = y0 + fill.pitch;
                w.region(&[
                    Point::new(x0, y0),
                    Point::new(x1, y0),
                    Point::new(x1, y1),
                    Point::new(x0, y1),
                ]);
            }
        }
    }
    for_pads(p, layer, |at, shape, pw, ph, _| {
        w.flash(pad_ap(shape, pw, ph), at)
    });
    for t in p.pcb.tracks.values().filter(|t| t.layer == layer) {
        match t.arc_deg {
            Some(s) => w.arc(t.width, t.a, t.b, s),
            None => w.line(t.width, t.a, t.b),
        }
    }
    // A via has copper only on the layers its barrel spans (a blind or buried via leaves the
    // others free).
    for v in p.pcb.vias.values() {
        if v.layers(&p.pcb.board.stackup).contains(&layer) {
            w.flash(Ap::Circle(v.diameter), v.position);
        }
    }
    w.finish(&copper_function(p, layer), "Positive", p)
}

pub fn soldermask(p: &Project, layer: CopperLayer) -> String {
    let mut w = Writer::new();
    let e = p.manufacturing.mask_expansion;
    for_pads(p, layer, |at, shape, pw, ph, _| {
        w.flash(pad_ap(shape, pw + 2 * e, ph + 2 * e), at)
    });
    let f = if layer == CopperLayer::Top {
        "Soldermask,Top"
    } else {
        "Soldermask,Bot"
    };
    w.finish(f, "Negative", p)
}

pub fn paste(p: &Project, layer: CopperLayer) -> String {
    let mut w = Writer::new();
    for_pads(p, layer, |at, shape, pw, ph, kind| {
        if kind == PadKind::Smd {
            w.flash(pad_ap(shape, pw, ph), at)
        }
    });
    let f = if layer == CopperLayer::Top {
        "Paste,Top"
    } else {
        "Paste,Bot"
    };
    w.finish(f, "Positive", p)
}

/// Legend layer: courtyard outlines, pin-1 dots and stroke-font reference designators, as
/// clipped clear of soldermask openings by `eak_engineering::silk` (the model DRC-015 checks).
pub fn silkscreen(p: &Project, side: Side) -> String {
    let mut w = Writer::new();
    for s in silk::legend(p, side) {
        if s.a == s.b {
            w.flash(Ap::Circle(s.width), s.a);
        } else {
            w.line(s.width, s.a, s.b);
        }
    }
    let f = if side == Side::Top {
        "Legend,Top"
    } else {
        "Legend,Bot"
    };
    w.finish(f, "Positive", p)
}

/// Board profile: the outline (arc edges as G02/G03 arcs) and every board cut-out, drawn
/// at 0.1 mm (schema v4, ADR-0042).
pub fn outline(p: &Project) -> String {
    const PROFILE_WIDTH: Nm = 100_000;
    let mut w = Writer::new();
    let o = &p.pcb.board.outline;
    for i in 0..o.len() {
        let (a, b) = (o[i], o[(i + 1) % o.len()]);
        match p.pcb.board.edge_sweep(i) {
            Some(s) => w.arc(PROFILE_WIDTH, a, b, s),
            None => w.line(PROFILE_WIDTH, a, b),
        }
    }
    for c in p.pcb.cutouts.values() {
        let h = &c.outline;
        for i in 0..h.len() {
            w.line(PROFILE_WIDTH, h[i], h[(i + 1) % h.len()]);
        }
    }
    w.finish("Profile,NP", "Positive", p)
}

/// Structural validation of a Gerber file produced (or received) by EAK.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GerberCheck {
    pub ok: bool,
    pub problems: Vec<String>,
    pub apertures: usize,
    pub operations: usize,
    /// Circular-arc operations (G02/G03), each checked to have both ends on one circle.
    #[serde(default)]
    pub arcs: usize,
}

pub fn validate(text: &str) -> GerberCheck {
    let mut c = GerberCheck::default();
    let mut defined = std::collections::BTreeSet::new();
    let (mut fs, mut mo, mut end, mut in_region) = (false, false, false, false);
    let (mut mode, mut multi_quadrant, mut at) = (Mode::Linear, false, Point::default());
    for (n, raw) in text.lines().enumerate() {
        let l = raw.trim();
        if l.is_empty() {
            continue;
        }
        if end {
            c.problems
                .push(format!("line {}: content after M02", n + 1));
        }
        if l.starts_with("%FS") {
            fs = l == "%FSLAX46Y46*%";
        } else if l.starts_with("%MO") {
            mo = l == "%MOMM*%";
        } else if let Some(rest) = l.strip_prefix("%ADD") {
            let num: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
            if num.parse::<u32>().map_or(true, |d| d < 10) || !defined.insert(num.clone()) {
                c.problems
                    .push(format!("line {}: bad or duplicate aperture D{num}", n + 1));
            }
            c.apertures += 1;
        } else if l == "M02*" {
            end = true;
        } else if l == "G36*" {
            in_region = true;
        } else if l == "G37*" {
            if !in_region {
                c.problems.push(format!("line {}: G37 without G36", n + 1));
            }
            in_region = false;
        } else if l.starts_with('D') && l.ends_with('*') {
            let num = &l[1..l.len() - 1];
            if !defined.contains(num) {
                c.problems.push(format!(
                    "line {}: aperture D{num} used before definition",
                    n + 1
                ));
            }
        } else if l.starts_with('X') || l.starts_with('Y') {
            c.operations += 1;
            let Some(op) = parse_op(l) else {
                c.problems
                    .push(format!("line {}: malformed operation", n + 1));
                continue;
            };
            let to = Point::new(op.x.unwrap_or(at.x), op.y.unwrap_or(at.y));
            let has_ij = op.i.is_some() || op.j.is_some();
            if op.d == 1 && mode != Mode::Linear {
                if !multi_quadrant {
                    c.problems.push(format!(
                        "line {}: circular arc before G75 (multi-quadrant mode)",
                        n + 1
                    ));
                }
                // The centre is the current point plus I/J; both ends must lie on one circle.
                let (cx, cy) = (
                    (at.x + op.i.unwrap_or(0)) as f64,
                    (at.y + op.j.unwrap_or(0)) as f64,
                );
                let r0 = (at.x as f64 - cx).hypot(at.y as f64 - cy);
                let r1 = (to.x as f64 - cx).hypot(to.y as f64 - cy);
                if (r0 - r1).abs() > ARC_RADIUS_TOLERANCE_NM || r0 == 0.0 {
                    c.problems.push(format!(
                        "line {}: arc ends are not on one circle (radii {:.4} and {:.4} mm)",
                        n + 1,
                        r0 / 1e6,
                        r1 / 1e6
                    ));
                }
                c.arcs += 1;
            } else if has_ij {
                c.problems.push(format!(
                    "line {}: I/J offsets on a linear or move operation",
                    n + 1
                ));
            }
            at = to;
        } else if l == "G01*" {
            mode = Mode::Linear;
        } else if l == "G02*" {
            mode = Mode::Clockwise;
        } else if l == "G03*" {
            mode = Mode::CounterClockwise;
        } else if l == "G75*" {
            multi_quadrant = true;
        } else if !(l.starts_with("%TF") || l == "%LPD*%") {
            c.problems
                .push(format!("line {}: unrecognised statement {l}", n + 1));
        }
    }
    if !fs {
        c.problems
            .push("missing or unexpected format specification (expected %FSLAX46Y46*%)".into());
    }
    if !mo {
        c.problems.push("missing unit specification %MOMM*%".into());
    }
    if !end {
        c.problems.push("missing M02 end of file".into());
    }
    if in_region {
        c.problems.push("unterminated region".into());
    }
    c.ok = c.problems.is_empty();
    c
}

/// Largest difference between the start and end radius of an arc that `validate` accepts:
/// 1 µm, well above the 1 nm rounding of the centre offsets EAK writes.
pub const ARC_RADIUS_TOLERANCE_NM: f64 = 1_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Linear,
    Clockwise,
    CounterClockwise,
}

/// One `X…Y…I…J…D0n*` operation; omitted coordinates are `None` (modal).
struct Op {
    x: Option<Nm>,
    y: Option<Nm>,
    i: Option<Nm>,
    j: Option<Nm>,
    d: u8,
}

fn parse_op(l: &str) -> Option<Op> {
    let body = l.strip_suffix('*')?;
    let (coords, d) = body.rsplit_once('D')?;
    let d = match d {
        "01" => 1,
        "02" => 2,
        "03" => 3,
        _ => return None,
    };
    let mut op = Op {
        x: None,
        y: None,
        i: None,
        j: None,
        d,
    };
    let mut rest = coords;
    while let Some(letter) = rest.chars().next() {
        let tail = &rest[1..];
        let len = tail
            .char_indices()
            .find(|&(k, ch)| !(ch.is_ascii_digit() || (k == 0 && (ch == '-' || ch == '+'))))
            .map_or(tail.len(), |(k, _)| k);
        let v: Nm = tail[..len].parse().ok()?;
        let slot = match letter {
            'X' => &mut op.x,
            'Y' => &mut op.y,
            'I' => &mut op.i,
            'J' => &mut op.j,
            _ => return None,
        };
        if slot.replace(v).is_some() {
            return None;
        }
        rest = &tail[len..];
    }
    if op.x.is_none() && op.y.is_none() {
        return None;
    }
    Some(op)
}

/// Gerber X2 `FileFunction` value for a copper layer: `Copper,L<n>,Top|Inr|Bot`.
pub fn copper_function(p: &Project, layer: CopperLayer) -> String {
    let st = &p.pcb.board.stackup;
    let n = st.index_of(layer).map_or(0, |i| i + 1);
    let pos = match layer {
        CopperLayer::Top => "Top",
        CopperLayer::Inner(_) => "Inr",
        CopperLayer::Bottom => "Bot",
    };
    format!("Copper,L{n},{pos}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::{Cutout, DrawLayer, OutlineArc, PcbText, Rotation, Track};

    fn board() -> Project {
        let mut p = Project::new("arcs");
        p.pcb.board.outline = vec![
            Point::mm(0.0, 0.0),
            Point::mm(40.0, 0.0),
            Point::mm(40.0, 30.0),
            Point::mm(0.0, 30.0),
        ];
        p
    }

    fn track(id: &str, a: Point, b: Point, arc_deg: Option<f64>) -> Track {
        Track {
            id: id.into(),
            layer: CopperLayer::Top,
            a,
            b,
            width: 250_000,
            net: "N".into(),
            arc_deg,
        }
    }

    fn lines<'a>(g: &'a str, prefix: &'a str) -> impl Iterator<Item = &'a str> {
        g.lines().filter(move |l| l.starts_with(prefix))
    }

    #[test]
    fn arc_tracks_are_written_as_g02_g03_and_revalidate() {
        let mut p = board();
        let (a, b) = (Point::mm(10.0, 10.0), Point::mm(20.0, 10.0));
        for (id, s) in [("ccw", 90.0), ("cw", -270.0)] {
            p.pcb.tracks.insert(id.into(), track(id, a, b, Some(s)));
        }
        p.pcb
            .tracks
            .insert("line".into(), track("line", a, b, None));
        let g = copper_layer(&p, CopperLayer::Top);
        let check = validate(&g);
        assert!(check.ok, "{:?}\n{g}", check.problems);
        assert_eq!(check.arcs, 2);
        assert_eq!(lines(&g, "G75*").count(), 1);
        assert_eq!(lines(&g, "G03*").count(), 1);
        assert_eq!(lines(&g, "G02*").count(), 1);
        // 90° CCW from (10,10) to (20,10) passes below the chord: the centre is (15,15).
        assert!(g.contains("X20000000Y10000000I5000000J5000000D01*"), "{g}");
        // Without arcs the file keeps the plain header (no G75).
        let plain = copper_layer(&board(), CopperLayer::Top);
        assert!(!plain.contains("G75*") && validate(&plain).ok);
    }

    #[test]
    fn validator_rejects_bad_arcs() {
        let head = "%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.250000*%\nD10*\nX0Y0D02*\n";
        let bad = |body: &str| validate(&format!("{head}{body}M02*\n"));
        let good = bad("G75*\nG03*\nX2000000Y0I1000000J0D01*\nG01*\n");
        assert!(good.ok && good.arcs == 1, "{:?}", good.problems);
        let no_g75 = bad("G03*\nX2000000Y0I1000000J0D01*\n");
        assert!(no_g75.problems.iter().any(|p| p.contains("G75")));
        let off_circle = bad("G75*\nG02*\nX2000000Y0I900000J0D01*\n");
        assert!(off_circle.problems.iter().any(|p| p.contains("one circle")));
        let ij_on_line = bad("X2000000Y0I1000000J0D01*\n");
        assert!(ij_on_line.problems.iter().any(|p| p.contains("I/J")));
        let junk = bad("X12Q3D01*\n");
        assert!(junk.problems.iter().any(|p| p.contains("malformed")));
        // Modal coordinates: an omitted Y keeps the current one.
        let modal = bad("X5000000D01*\nY3000000D01*\n");
        assert!(modal.ok, "{:?}", modal.problems);
    }

    #[test]
    fn profile_has_arc_edges_and_cutouts() {
        let mut p = board();
        p.pcb.board.outline_arcs = vec![OutlineArc {
            edge: 1,
            sweep_deg: 60.0,
        }];
        p.pcb.cutouts.insert(
            "cut".into(),
            Cutout {
                id: "cut".into(),
                outline: vec![
                    Point::mm(5.0, 5.0),
                    Point::mm(9.0, 5.0),
                    Point::mm(9.0, 9.0),
                ],
            },
        );
        let g = outline(&p);
        let check = validate(&g);
        assert!(check.ok, "{:?}\n{g}", check.problems);
        assert_eq!(check.arcs, 1);
        // 3 straight edges + 1 arc on the outline, 3 cut-out edges.
        assert_eq!(lines(&g, "X").filter(|l| l.ends_with("D01*")).count(), 7);
        assert!(g.contains("X9000000Y9000000D01*") || g.contains("X5000000Y5000000D01*"));
    }

    #[test]
    fn silk_text_reaches_the_legend_file() {
        let mut p = board();
        let before = validate(&silkscreen(&p, Side::Top)).operations;
        p.pcb.texts.insert(
            "t".into(),
            PcbText {
                id: "t".into(),
                layer: DrawLayer::TopSilk,
                position: Point::mm(20.0, 15.0),
                text: "EAK".into(),
                size: 1_000_000,
                stroke_width: 0,
                rotation: Rotation::R0,
            },
        );
        let g = silkscreen(&p, Side::Top);
        let check = validate(&g);
        assert!(check.ok, "{:?}", check.problems);
        assert!(check.operations > before + 10);
        // Bottom legend does not carry top text.
        assert_eq!(validate(&silkscreen(&p, Side::Bottom)).operations, 0);
    }
}
