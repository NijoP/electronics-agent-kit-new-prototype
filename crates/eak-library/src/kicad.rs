//! KiCad library interchange: writes `.kicad_sym` symbol libraries and `.kicad_mod` footprints
//! from EAK definitions, the inverse of the existing clean-room reader
//! [`eak_core::kicad_import`]. Together they give a tested round trip:
//! import → EAK definitions → export → re-import yields the same definitions.
//!
//! Independent implementation written from the public, documented S-expression file formats; no
//! KiCad source is used and no KiCad library is bundled. EAK's own format stays the source of
//! truth: this is interchange only. Nothing here opens a path; callers get text.
//!
//! No silent data loss in this direction either. Everything EAK holds that these files cannot
//! carry back (for example a footprint body height, per-pad mask or paste expansion, mixed pin
//! name visibility) is listed in [`Export::losses`]. Lengths are written in millimetres with
//! six decimals at most, which is exact for EAK's integer nanometres.

use eak_core::geom::{Nm, Point};
use eak_core::graphic::{Graphic, LineStyle, Stroke};
use eak_core::kicad_import::{FootprintImport, SymbolImport};
use eak_core::library::{PadKind, PadShape, PinOrientation, PinType};
use std::fmt::Write as _;

pub use eak_core::kicad_import::{import_footprint, import_symbol_lib};

/// Generator name written into exported files.
pub const GENERATOR: &str = "eak";
/// Symbol library format version written (KiCad 8 era).
pub const SYMBOL_LIB_VERSION: &str = "20231120";
/// Footprint format version written (KiCad 8 era).
pub const FOOTPRINT_VERSION: &str = "20240108";

/// Exported text plus everything that the file cannot carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub text: String,
    /// One plain sentence per lost or approximated value. Empty means a lossless export.
    pub losses: Vec<String>,
}

/// Nanometres as millimetre text: exact, shortest form (`1.27`, `-0.5`, `3`).
pub fn mm_text(v: Nm) -> String {
    let sign = if v < 0 { "-" } else { "" };
    let a = v.unsigned_abs();
    let (int, frac) = (a / 1_000_000, a % 1_000_000);
    if frac == 0 {
        return format!("{sign}{int}");
    }
    let f = format!("{frac:06}");
    format!("{sign}{int}.{}", f.trim_end_matches('0'))
}

/// A quoted S-expression string with `\` and `"` escaped and control characters written as
/// escapes.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn xy(p: Point) -> String {
    format!("{} {}", mm_text(p.x), mm_text(p.y))
}

fn stroke_text(s: &Stroke) -> String {
    let t = match s.style {
        LineStyle::Solid => "solid",
        LineStyle::Dashed => "dash",
        LineStyle::Dotted => "dot",
        LineStyle::DashDot => "dash_dot",
    };
    format!("(stroke (width {}) (type {t}))", mm_text(s.width))
}

fn fill_text(fill: bool) -> &'static str {
    if fill {
        "(fill (type outline))"
    } else {
        "(fill (type none))"
    }
}

fn pin_type_text(t: PinType) -> &'static str {
    match t {
        PinType::Input => "input",
        PinType::Output => "output",
        PinType::Bidirectional => "bidirectional",
        PinType::Passive => "passive",
        PinType::PowerIn => "power_in",
        PinType::PowerOut => "power_out",
        PinType::OpenCollector => "open_collector",
        PinType::NoConnect => "no_connect",
    }
}

/// KiCad's pin angle: the direction from the connection point to the body.
fn pin_angle(o: PinOrientation) -> u32 {
    match o {
        PinOrientation::Right => 0,
        PinOrientation::Up => 90,
        PinOrientation::Left => 180,
        PinOrientation::Down => 270,
    }
}

fn pts(points: &[Point]) -> String {
    let xys: Vec<String> = points
        .iter()
        .map(|p| format!("(xy {})", xy(*p)))
        .collect();
    format!("(pts {})", xys.join(" "))
}

/// A point at `deg` on the circle around `c` with radius `r`, rounded to whole nanometres.
fn on_circle(c: Point, r: Nm, deg: f64) -> Point {
    let (s, co) = deg.to_radians().sin_cos();
    Point::new(
        c.x + (r as f64 * co).round() as Nm,
        c.y + (r as f64 * s).round() as Nm,
    )
}

fn polyline_text(points: &[Point], stroke: &Stroke, fill: bool) -> String {
    format!(
        "(polyline {} {} {})",
        pts(points),
        stroke_text(stroke),
        fill_text(fill)
    )
}

fn graphic_text(g: &Graphic, losses: &mut Vec<String>, sym: &str) -> String {
    match g {
        Graphic::Line { a, b, stroke } => polyline_text(&[*a, *b], stroke, false),
        Graphic::Polyline { points, stroke } => {
            if points.len() == 2 {
                losses.push(format!("{sym}: a 2-point polyline is read back as a line"));
            }
            polyline_text(points, stroke, false)
        }
        Graphic::Polygon {
            points,
            fill,
            stroke,
        } => {
            if !*fill || points.len() < 3 {
                losses.push(format!(
                    "{sym}: an unfilled or degenerate polygon is read back as an open polyline"
                ));
            }
            polyline_text(points, stroke, *fill)
        }
        Graphic::Rectangle {
            min,
            max,
            fill,
            stroke,
        } => format!(
            "(rectangle (start {}) (end {}) {} {})",
            xy(*min),
            xy(*max),
            stroke_text(stroke),
            fill_text(*fill)
        ),
        Graphic::Circle {
            center,
            radius,
            fill,
            stroke,
        } => format!(
            "(circle (center {}) (radius {}) {} {})",
            xy(*center),
            mm_text(*radius),
            stroke_text(stroke),
            fill_text(*fill)
        ),
        Graphic::Arc {
            center,
            radius,
            start_deg,
            end_deg,
            stroke,
        } => {
            // Counter-clockwise from start to end; the mid point fixes the direction.
            let sweep = (end_deg - start_deg).rem_euclid(360.0);
            let mid = start_deg + sweep / 2.0;
            format!(
                "(arc (start {}) (mid {}) (end {}) {} {})",
                xy(on_circle(*center, *radius, *start_deg)),
                xy(on_circle(*center, *radius, mid)),
                xy(on_circle(*center, *radius, *end_deg)),
                stroke_text(stroke),
                fill_text(false)
            )
        }
        Graphic::Text {
            position,
            text,
            size,
            rotation,
        } => {
            // Symbol text angles are written in tenths of a degree.
            let tenths = rotation.degrees() * 10;
            let s = mm_text(*size);
            format!(
                "(text {} (at {} {tenths}) (effects (font (size {s} {s}))))",
                quote(text),
                xy(*position)
            )
        }
    }
}

fn property_text(key: &str, value: &str) -> String {
    format!(
        "    (property {} {} (at 0 0 0) (effects (font (size 1.27 1.27))))\n",
        quote(key),
        quote(value)
    )
}

const PIN_FONT: &str = "(effects (font (size 1.27 1.27)))";

/// Write a symbol library holding `symbols`, in order.
pub fn export_symbol_lib(symbols: &[SymbolImport]) -> Export {
    let mut losses = Vec::new();
    let mut t = format!(
        "(kicad_symbol_lib (version {SYMBOL_LIB_VERSION}) (generator {})\n",
        quote(GENERATOR)
    );
    for s in symbols {
        let name = &s.name;
        let pins = &s.def.pins;
        let names_hidden = !pins.is_empty() && pins.iter().all(|p| !p.name_visible);
        let numbers_hidden = !pins.is_empty() && pins.iter().all(|p| !p.number_visible);
        if !names_hidden && pins.iter().any(|p| !p.name_visible) {
            losses.push(format!(
                "{name}: pin name visibility is per symbol in this format; all names shown"
            ));
        }
        if !numbers_hidden && pins.iter().any(|p| !p.number_visible) {
            losses.push(format!(
                "{name}: pin number visibility is per symbol in this format; all numbers shown"
            ));
        }
        let _ = write!(t, "  (symbol {}", quote(name));
        if names_hidden {
            t.push_str(" (pin_names hide)");
        }
        if numbers_hidden {
            t.push_str(" (pin_numbers hide)");
        }
        t.push_str(" (in_bom yes) (on_board yes)\n");
        // `?` is read back as "no reference prefix", `~` as "no datasheet".
        let refdes = s.refdes_prefix.as_deref().unwrap_or("?");
        t.push_str(&property_text("Reference", refdes));
        t.push_str(&property_text("Value", s.value.as_deref().unwrap_or("")));
        let footprint = s.footprint.as_deref().unwrap_or("");
        t.push_str(&property_text("Footprint", footprint));
        let datasheet = s.datasheet.as_deref().unwrap_or("~");
        t.push_str(&property_text("Datasheet", datasheet));
        // Drawing in unit 0 (shared by all units), pins in unit 1; body style 1.
        let _ = writeln!(t, "    (symbol {}", quote(&format!("{name}_0_1")));
        let boxed = [Graphic::body_box(s.def.body_half_w, s.def.body_half_h)];
        let graphics: &[Graphic] = if s.def.graphics.is_empty() {
            &boxed
        } else {
            &s.def.graphics
        };
        for g in graphics {
            let _ = writeln!(t, "      {}", graphic_text(g, &mut losses, name));
        }
        t.push_str("    )\n");
        let _ = writeln!(t, "    (symbol {}", quote(&format!("{name}_1_1")));
        for p in pins {
            if p.name == "~" {
                losses.push(format!(
                    "{name}: pin {} is named \"~\", which reads back as no name",
                    p.number
                ));
            }
            let pname = if p.name.is_empty() { "~" } else { p.name.as_str() };
            let _ = write!(
                t,
                "      (pin {} line (at {} {}) (length {})",
                pin_type_text(p.electrical_type),
                xy(p.position),
                pin_angle(p.orientation),
                mm_text(p.length)
            );
            let (pname, number) = (quote(pname), quote(&p.number));
            let _ = writeln!(t, " (name {pname} {PIN_FONT}) (number {number} {PIN_FONT}))");
        }
        t.push_str("    )\n  )\n");
    }
    t.push_str(")\n");
    Export { text: t, losses }
}

fn pad_shape_text(s: PadShape) -> &'static str {
    match s {
        PadShape::Rect => "rect",
        PadShape::Circle => "circle",
        PadShape::Oval => "oval",
    }
}

/// Write one footprint file.
pub fn export_footprint(fp: &FootprintImport) -> Export {
    let mut losses = Vec::new();
    let d = &fp.def;
    let name = &d.name;
    let mut t = format!(
        "(footprint {} (version {FOOTPRINT_VERSION}) (generator {}) (layer \"F.Cu\")\n",
        quote(name),
        quote(GENERATOR)
    );
    if let Some(descr) = &fp.description {
        let _ = writeln!(t, "  (descr {})", quote(descr));
    }
    let all_smd = d.pads.iter().all(|p| p.kind == PadKind::Smd);
    let attr = if all_smd { "smd" } else { "through_hole" };
    let _ = writeln!(t, "  (attr {attr})");
    let (cw, ch) = (d.courtyard_half_w, d.courtyard_half_h);
    let _ = write!(
        t,
        "  (fp_rect (start {} {}) (end {} {})",
        mm_text(-cw),
        mm_text(-ch),
        mm_text(cw),
        mm_text(ch)
    );
    t.push_str(" (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\"))\n");
    if d.body_height.is_some() {
        losses.push(format!(
            "{name}: body height has no place in a footprint file; not written"
        ));
    }
    for p in &d.pads {
        let n = &p.number;
        if p.mask_expansion.is_some() || p.paste_expansion.is_some() {
            losses.push(format!("{name}: pad {n} mask/paste expansion is not written"));
        }
        let (kind, layers) = match p.kind {
            PadKind::Smd => ("smd", "\"F.Cu\" \"F.Paste\" \"F.Mask\""),
            PadKind::Tht => ("thru_hole", "\"*.Cu\" \"*.Mask\""),
        };
        let drill = match (p.kind, p.drill) {
            (PadKind::Tht, Some(dr)) => format!(" (drill {})", mm_text(dr)),
            (PadKind::Tht, None) => {
                losses.push(format!(
                    "{name}: through-hole pad {n} has no drill and is skipped when read back"
                ));
                String::new()
            }
            (PadKind::Smd, Some(_)) => {
                losses.push(format!("{name}: drill on SMD pad {n} is not written"));
                String::new()
            }
            (PadKind::Smd, None) => String::new(),
        };
        let _ = writeln!(
            t,
            "  (pad {} {kind} {} (at {}) (size {} {}){drill} (layers {layers}))",
            quote(n),
            pad_shape_text(p.shape),
            xy(p.position),
            mm_text(p.size_x),
            mm_text(p.size_y)
        );
    }
    if let Some(m) = &fp.model_path {
        let _ = writeln!(t, "  (model {})", quote(m));
    }
    t.push_str(")\n");
    Export { text: t, losses }
}

fn arc_close(a: &Graphic, b: &Graphic) -> bool {
    let (
        Graphic::Arc {
            center: c1,
            radius: r1,
            start_deg: s1,
            end_deg: e1,
            stroke: k1,
        },
        Graphic::Arc {
            center: c2,
            radius: r2,
            start_deg: s2,
            end_deg: e2,
            stroke: k2,
        },
    ) = (a, b)
    else {
        return false;
    };
    let same_angle = |x: f64, y: f64| {
        let d = (x - y).rem_euclid(360.0);
        d.min(360.0 - d) < 1e-3
    };
    (c1.x - c2.x).abs() <= 2
        && (c1.y - c2.y).abs() <= 2
        && (r1 - r2).abs() <= 2
        && same_angle(*s1, *s2)
        && same_angle(*e1, *e2)
        && k1 == k2
}

/// Whether two imported symbols hold the same definition. Exact, except that an arc may differ
/// by 2 nm in centre or radius and 0.001° in angle: arcs travel as three points, so their centre
/// is recomputed in floating point on every read.
pub fn same_symbol(a: &SymbolImport, b: &SymbolImport) -> bool {
    if a.def.graphics.len() != b.def.graphics.len() {
        return false;
    }
    let mut b = b.clone();
    for (ga, gb) in a.def.graphics.iter().zip(b.def.graphics.iter_mut()) {
        if arc_close(ga, gb) {
            *gb = ga.clone();
        }
    }
    *a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millimetre_text_is_exact_and_short() {
        assert_eq!(mm_text(0), "0");
        assert_eq!(mm_text(1_270_000), "1.27");
        assert_eq!(mm_text(-500_000), "-0.5");
        assert_eq!(mm_text(3_000_000), "3");
        assert_eq!(mm_text(1), "0.000001");
        assert_eq!(mm_text(-1_000_001), "-1.000001");
    }

    #[test]
    fn quoting_escapes_what_the_reader_unescapes() {
        assert_eq!(quote(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(quote("x\ny"), r#""x\ny""#);
    }
}
