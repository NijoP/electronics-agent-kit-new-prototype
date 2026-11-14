//! RS-274X (Gerber X2 attributes) writer and a structural re-reader for validation.

use eak_core::{CopperLayer, Nm, PadKind, PadShape, Point, Project, Side};
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
}

impl Writer {
    fn new() -> Self {
        Writer {
            body: String::new(),
            apertures: BTreeMap::new(),
            current: None,
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
        w.line(t.width, t.a, t.b);
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

pub fn outline(p: &Project) -> String {
    let mut w = Writer::new();
    let o = &p.pcb.board.outline;
    for i in 0..o.len() {
        w.line(100_000, o[i], o[(i + 1) % o.len()]);
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
}

pub fn validate(text: &str) -> GerberCheck {
    let mut c = GerberCheck::default();
    let mut defined = std::collections::BTreeSet::new();
    let (mut fs, mut mo, mut end, mut in_region) = (false, false, false, false);
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
        } else if l.starts_with('X') {
            if !(l.ends_with("D01*") || l.ends_with("D02*") || l.ends_with("D03*")) {
                c.problems
                    .push(format!("line {}: malformed operation", n + 1));
            }
            c.operations += 1;
        } else if !(l.starts_with("%TF") || l == "%LPD*%" || l == "G01*") {
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
