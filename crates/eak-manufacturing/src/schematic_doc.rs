//! Schematic documents (gap #41): every sheet drawn once as a list of vector shapes, written as
//! SVG (one file per sheet) and as one PDF with a page per sheet. This is the SVG export
//! backend of AD-4: the on-screen editor uses the Canvas2D renderer, documents use this.
//!
//! Each page carries a title block with the project name, sheet, revision, date, package
//! status (DRAFT unless the release gate passed) and the EAK-IR hash it was drawn from.
//! The PDF uses the standard Helvetica fonts (not embedded) and needs no dependency.

use eak_core::geom::{point_segment_distance, to_mm};
use eak_core::{LabelKind, Point, Project};
use std::collections::BTreeMap;

/// Text anchor (horizontal alignment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// One drawing primitive, in millimetres, Y up, origin at the sheet's lower-left corner
/// (the schematic's own coordinates).
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Line {
        a: (f64, f64),
        b: (f64, f64),
        width: f64,
    },
    Rect {
        at: (f64, f64),
        w: f64,
        h: f64,
        width: f64,
    },
    Dot {
        at: (f64, f64),
        r: f64,
    },
    Text {
        at: (f64, f64),
        size: f64,
        text: String,
        anchor: Anchor,
        bold: bool,
    },
}

/// Title-block fields shared by every page of one export.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleBlock {
    pub project: String,
    pub revision: u64,
    /// `YYYY-MM-DD` (UTC).
    pub date: String,
    /// Status label, for example "DRAFT - NOT FOR FABRICATION".
    pub status: String,
    pub input_sha256: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub sheet: String,
    pub width: f64,
    pub height: f64,
    pub shapes: Vec<Shape>,
}

impl Page {
    /// Every text string drawn on the page (used by tests and checks).
    pub fn texts(&self) -> Vec<&str> {
        self.shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
}

fn xy(p: Point) -> (f64, f64) {
    (to_mm(p.x), to_mm(p.y))
}

const WIRE: f64 = 0.25;
const BODY: f64 = 0.3;

/// One page per schematic sheet (a single A4 page when the project has no sheet).
pub fn pages(p: &Project, tb: &TitleBlock) -> Vec<Page> {
    let sheets: Vec<(String, String, f64, f64)> = if p.schematic.sheets.is_empty() {
        vec![(String::new(), "Sheet 1".into(), 297.0, 210.0)]
    } else {
        p.schematic
            .sheets
            .values()
            .map(|s| {
                (
                    s.id.clone(),
                    s.name.clone(),
                    to_mm(s.width).max(100.0),
                    to_mm(s.height).max(60.0),
                )
            })
            .collect()
    };
    let count = sheets.len();
    sheets
        .iter()
        .enumerate()
        .map(|(i, (id, name, w, h))| {
            let on = |sheet: &str| p.schematic.sheets.is_empty() || sheet == id.as_str();
            let mut shapes = vec![];
            draw_sheet(p, &on, &mut shapes);
            title_block(&mut shapes, *w, *h, tb, name, i + 1, count);
            Page {
                sheet: name.clone(),
                width: *w,
                height: *h,
                shapes,
            }
        })
        .collect()
}

fn draw_sheet(p: &Project, on: &dyn Fn(&str) -> bool, out: &mut Vec<Shape>) {
    let wires: Vec<_> = p
        .schematic
        .wires
        .values()
        .filter(|w| on(&w.sheet_id))
        .collect();
    for w in &wires {
        out.push(Shape::Line {
            a: xy(w.a),
            b: xy(w.b),
            width: WIRE,
        });
    }
    let mut pins: Vec<Point> = vec![];
    for s in p.schematic.symbols.values().filter(|s| on(&s.sheet_id)) {
        let (cx, cy) = xy(s.position);
        let Some(def) = p.library.get(&s.component_id) else {
            out.push(Shape::Rect {
                at: (cx - 2.0, cy - 1.5),
                w: 4.0,
                h: 3.0,
                width: BODY,
            });
            out.push(Shape::Text {
                at: (cx, cy - 0.6),
                size: 1.8,
                text: format!("{} (missing part {})", s.refdes, s.component_id),
                anchor: Anchor::Middle,
                bold: false,
            });
            continue;
        };
        let (hw, hh) = s
            .rotation
            .apply_size(def.symbol.body_half_w, def.symbol.body_half_h);
        let (hw, hh) = (to_mm(hw), to_mm(hh));
        for pin in &def.symbol.pins {
            let at = s.place(pin.position);
            pins.push(at);
            let (px, py) = xy(at);
            // Pin stub from the body edge to the connection point.
            let edge = (px.clamp(cx - hw, cx + hw), py.clamp(cy - hh, cy + hh));
            out.push(Shape::Line {
                a: edge,
                b: (px, py),
                width: WIRE,
            });
            out.push(Shape::Text {
                at: ((edge.0 + px) / 2.0, (edge.1 + py) / 2.0 + 0.4),
                size: 1.0,
                text: pin.number.clone(),
                anchor: Anchor::Middle,
                bold: false,
            });
        }
        out.push(Shape::Rect {
            at: (cx - hw, cy - hh),
            w: 2.0 * hw,
            h: 2.0 * hh,
            width: BODY,
        });
        out.push(Shape::Text {
            at: (cx, cy + hh + 1.0),
            size: 1.8,
            text: s.refdes.clone(),
            anchor: Anchor::Middle,
            bold: true,
        });
        out.push(Shape::Text {
            at: (cx, cy - hh - 2.5),
            size: 1.6,
            text: s.value.clone(),
            anchor: Anchor::Middle,
            bold: false,
        });
    }
    // Junction dots where three or more connections meet at a wire end.
    let mut ends: BTreeMap<(i64, i64), usize> = BTreeMap::new();
    for w in &wires {
        for e in [w.a, w.b] {
            *ends.entry((e.x, e.y)).or_default() += 1;
        }
    }
    for (&(x, y), &n) in &ends {
        let e = Point::new(x, y);
        let on_pins = pins.iter().filter(|q| **q == e).count();
        let through = wires
            .iter()
            .filter(|w| w.a != e && w.b != e && point_segment_distance(e, w.a, w.b) < 1_000.0)
            .count();
        if n + on_pins + 2 * through >= 3 {
            out.push(Shape::Dot { at: xy(e), r: 0.45 });
        }
    }
    for l in p.schematic.labels.values().filter(|l| on(&l.sheet_id)) {
        let (x, y) = xy(l.position);
        out.push(Shape::Dot { at: (x, y), r: 0.4 });
        out.push(Shape::Text {
            at: (x + 1.0, y + 0.8),
            size: 1.6,
            text: l.name.clone(),
            anchor: Anchor::Start,
            bold: l.kind == LabelKind::Power,
        });
    }
    for nc in p.schematic.no_connects.values() {
        let Some(s) = p.schematic.symbols.get(&nc.symbol_id) else {
            continue;
        };
        if !on(&s.sheet_id) {
            continue;
        }
        if let Some(at) = p.pin_position(s, &nc.pin) {
            let (x, y) = xy(at);
            out.push(Shape::Line {
                a: (x - 1.0, y - 1.0),
                b: (x + 1.0, y + 1.0),
                width: WIRE,
            });
            out.push(Shape::Line {
                a: (x - 1.0, y + 1.0),
                b: (x + 1.0, y - 1.0),
                width: WIRE,
            });
        }
    }
}

/// Frame and title block in the lower-right corner.
fn title_block(
    out: &mut Vec<Shape>,
    w: f64,
    h: f64,
    tb: &TitleBlock,
    sheet: &str,
    n: usize,
    count: usize,
) {
    out.push(Shape::Rect {
        at: (4.0, 4.0),
        w: w - 8.0,
        h: h - 8.0,
        width: 0.35,
    });
    let (bw, bh) = (96.0_f64.min(w - 8.0), 30.0);
    let (x0, y0) = (w - 4.0 - bw, 4.0);
    out.push(Shape::Rect {
        at: (x0, y0),
        w: bw,
        h: bh,
        width: 0.35,
    });
    let sha: String = tb.input_sha256.chars().take(16).collect();
    let rows = [
        (format!("Project: {}", tb.project), 2.6, true),
        (format!("Sheet: {sheet} ({n} of {count})"), 2.0, false),
        (
            format!("Revision: {}    Date: {}", tb.revision, tb.date),
            2.0,
            false,
        ),
        (format!("Status: {}", tb.status), 2.0, true),
        (format!("EAK-IR sha256: {sha}"), 1.6, false),
        (
            format!("eak-manufacturing {}", env!("CARGO_PKG_VERSION")),
            1.6,
            false,
        ),
    ];
    let mut y = y0 + bh - 4.5;
    for (i, (text, size, bold)) in rows.into_iter().enumerate() {
        if i > 0 {
            out.push(Shape::Line {
                a: (x0, y + size + 1.2),
                b: (x0 + bw, y + size + 1.2),
                width: 0.15,
            });
        }
        out.push(Shape::Text {
            at: (x0 + 2.0, y),
            size,
            text,
            anchor: Anchor::Start,
            bold,
        });
        y -= 4.6;
    }
}

/// A coordinate with at most three decimals and no trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.into()
    }
}

/// SVG document of one page (millimetre units, Y down as SVG requires).
pub fn svg(page: &Page, title: &str) -> String {
    let (w, h) = (page.width, page.height);
    let fy = |y: f64| num(h - y);
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\">\n<title>{}</title>\n<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>\n<g fill=\"none\" stroke=\"#000000\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n",
        num(w),
        num(h),
        num(w),
        num(h),
        crate::xlsx::xml_text(title),
        num(w),
        num(h)
    );
    for s in &page.shapes {
        match s {
            Shape::Line { a, b, width } => out.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke-width=\"{}\"/>\n",
                num(a.0),
                fy(a.1),
                num(b.0),
                fy(b.1),
                num(*width)
            )),
            Shape::Rect { at, w, h, width } => out.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" stroke-width=\"{}\"/>\n",
                num(at.0),
                fy(at.1 + h),
                num(*w),
                num(*h),
                num(*width)
            )),
            Shape::Dot { at, r } => out.push_str(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#000000\" stroke=\"none\"/>\n",
                num(at.0),
                fy(at.1),
                num(*r)
            )),
            Shape::Text {
                at,
                size,
                text,
                anchor,
                bold,
            } => out.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-family=\"Helvetica, Arial, sans-serif\" font-size=\"{}\" text-anchor=\"{}\"{} fill=\"#000000\" stroke=\"none\">{}</text>\n",
                num(at.0),
                fy(at.1),
                num(*size),
                match anchor {
                    Anchor::Start => "start",
                    Anchor::Middle => "middle",
                    Anchor::End => "end",
                },
                if *bold { " font-weight=\"bold\"" } else { "" },
                crate::xlsx::xml_text(text)
            )),
        }
    }
    out.push_str("</g>\n</svg>\n");
    out
}

/// Problems of an SVG written by [`svg`] (empty when well formed).
pub fn validate_svg(text: &str) -> Vec<String> {
    let mut out = vec![];
    if !text.starts_with("<?xml") || !text.contains("<svg ") {
        out.push("not an SVG document".into());
    }
    if !text.trim_end().ends_with("</svg>") {
        out.push("SVG document is truncated".into());
    }
    out
}

/// Helvetica advance widths (1/1000 em) of the printable ASCII characters 32..=126.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/// Text in the PDF's WinAnsi range: printable ASCII, anything else as `?`.
fn pdf_chars(s: &str) -> String {
    s.chars()
        .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
        .collect()
}

/// A PDF literal string body: `pdf_chars` text with `\`, `(` and `)` escaped.
fn pdf_string(s: &str) -> String {
    pdf_chars(s)
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn text_width(s: &str, size: f64) -> f64 {
    s.chars()
        .map(|c| f64::from(HELVETICA[(c as usize).saturating_sub(32).min(94)]))
        .sum::<f64>()
        * size
        / 1000.0
}

/// A filled circle as four Bezier curves (PDF has no circle operator).
fn circle(x: f64, y: f64, r: f64) -> String {
    let k = 0.552_284_75 * r;
    let pts = [
        (x + r, y),
        (x + r, y + k),
        (x + k, y + r),
        (x, y + r),
        (x - k, y + r),
        (x - r, y + k),
        (x - r, y),
        (x - r, y - k),
        (x - k, y - r),
        (x, y - r),
        (x + k, y - r),
        (x + r, y - k),
        (x + r, y),
    ];
    let p = |i: usize| format!("{} {}", num(pts[i].0), num(pts[i].1));
    let mut s = format!("{} m", p(0));
    for c in 0..4 {
        let i = 1 + 3 * c;
        s.push_str(&format!(" {} {} {} c", p(i), p(i + 1), p(i + 2)));
    }
    s.push_str(" f\n");
    s
}

fn content(page: &Page) -> String {
    // Work in millimetres: scale the page's point space once.
    let mut s = String::from("2.834646 0 0 2.834646 0 0 cm 1 J 1 j 0 0 0 RG 0 0 0 rg\n");
    for shape in &page.shapes {
        match shape {
            Shape::Line { a, b, width } => s.push_str(&format!(
                "{} w {} {} m {} {} l S\n",
                num(*width),
                num(a.0),
                num(a.1),
                num(b.0),
                num(b.1)
            )),
            Shape::Rect { at, w, h, width } => s.push_str(&format!(
                "{} w {} {} {} {} re S\n",
                num(*width),
                num(at.0),
                num(at.1),
                num(*w),
                num(*h)
            )),
            Shape::Dot { at, r } => s.push_str(&circle(at.0, at.1, *r)),
            Shape::Text {
                at,
                size,
                text,
                anchor,
                bold,
            } => {
                let dx = match anchor {
                    Anchor::Start => 0.0,
                    Anchor::Middle => text_width(&pdf_chars(text), *size) / 2.0,
                    Anchor::End => text_width(&pdf_chars(text), *size),
                };
                s.push_str(&format!(
                    "BT /{} {} Tf {} {} Td ({}) Tj ET\n",
                    if *bold { "F2" } else { "F1" },
                    num(*size),
                    num(at.0 - dx),
                    num(at.1),
                    pdf_string(text)
                ));
            }
        }
    }
    s
}

/// PDF document with one page per [`Page`]. Everything in the file is ASCII.
pub fn pdf(pages: &[Page], title: &str) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = vec![];
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            (0..pages.len())
                .map(|i| format!("{} 0 R", 6 + 2 * i))
                .collect::<Vec<_>>()
                .join(" "),
            pages.len()
        )
        .into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
            .to_vec(),
        format!(
            "<< /Title ({}) /Producer (eak-manufacturing {}) >>",
            pdf_string(title),
            env!("CARGO_PKG_VERSION")
        )
        .into_bytes(),
    ];
    for (i, page) in pages.iter().enumerate() {
        let (w, h) = (page.width * 72.0 / 25.4, page.height * 72.0 / 25.4);
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                num(w),
                num(h),
                7 + 2 * i
            )
            .into_bytes(),
        );
        let data = content(page);
        objects
            .push(format!("<< /Length {} >>\nstream\n{data}\nendstream", data.len()).into_bytes());
    }
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// Problems of a PDF written by [`pdf`]: header, trailer, and a cross-reference table where
/// every object offset points at that object.
pub fn validate_pdf(bytes: &[u8]) -> Vec<String> {
    let mut out = vec![];
    if !bytes.starts_with(b"%PDF-") {
        out.push("no %PDF header".to_string());
    }
    let text = String::from_utf8_lossy(bytes);
    if !text.trim_end().ends_with("%%EOF") {
        out.push("no %%EOF trailer".to_string());
        return out;
    }
    let Some(start) = text
        .rfind("startxref")
        .and_then(|i| text[i + 9..].split_whitespace().next())
        .and_then(|n| n.parse::<usize>().ok())
    else {
        out.push("no startxref offset".into());
        return out;
    };
    let Some(table) = bytes.get(start..) else {
        out.push("startxref points past the end of the file".into());
        return out;
    };
    if !table.starts_with(b"xref") {
        out.push("startxref does not point at the cross-reference table".into());
        return out;
    }
    let table = String::from_utf8_lossy(table);
    for (n, line) in table.lines().skip(3).enumerate() {
        if !line.ends_with(" n ") || line.len() < 10 {
            break;
        }
        let at: usize = line[..10].parse().unwrap_or(usize::MAX);
        let want = format!("{} 0 obj", n + 1);
        if bytes.get(at..at.saturating_add(want.len())) != Some(want.as_bytes()) {
            out.push(format!(
                "object {} is not at its cross-reference offset",
                n + 1
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tb() -> TitleBlock {
        TitleBlock {
            project: "Doc (test)".into(),
            revision: 7,
            date: "2026-11-23".into(),
            status: "DRAFT - NOT FOR FABRICATION".into(),
            input_sha256: "0123456789abcdef0123".into(),
        }
    }

    #[test]
    fn pages_draw_symbols_and_the_title_block_fields() {
        let p = crate::bom_config::tests::project();
        let pages = pages(&p, &tb());
        assert_eq!(pages.len(), 1);
        let texts = pages[0].texts();
        for want in [
            "Project: Doc (test)",
            "Sheet: Sheet 1 (1 of 1)",
            "Revision: 7    Date: 2026-11-23",
            "Status: DRAFT - NOT FOR FABRICATION",
            "EAK-IR sha256: 0123456789abcdef",
            "R10",
            "100n",
        ] {
            assert!(texts.contains(&want), "missing {want:?} in {texts:?}");
        }
        assert_eq!(pages[0].width, 297.0);
    }

    #[test]
    fn pdf_has_a_valid_cross_reference_and_the_title_block_text() {
        let p = crate::bom_config::tests::project();
        let mut pages = pages(&p, &tb());
        pages.push(pages[0].clone());
        let doc = pdf(&pages, "Doc (test) schematic");
        assert!(validate_pdf(&doc).is_empty(), "{:?}", validate_pdf(&doc));
        let text = String::from_utf8(doc.clone()).expect("ASCII PDF");
        assert!(text.contains("/Count 2"));
        assert!(text.contains("(Status: DRAFT - NOT FOR FABRICATION) Tj"));
        assert!(text.contains("(Project: Doc \\(test\\)) Tj"));
        let mut broken = doc.clone();
        let at = text.find("2 0 obj").unwrap();
        broken[at] = b'9';
        assert!(!validate_pdf(&broken).is_empty());
        assert!(!validate_pdf(b"%PDF-1.4\n").is_empty());
    }

    #[test]
    fn svg_flips_y_and_escapes_text() {
        let page = Page {
            sheet: "S".into(),
            width: 100.0,
            height: 60.0,
            shapes: vec![
                Shape::Line {
                    a: (0.0, 0.0),
                    b: (10.0, 10.0),
                    width: 0.25,
                },
                Shape::Text {
                    at: (5.0, 50.0),
                    size: 2.0,
                    text: "A<B>&".into(),
                    anchor: Anchor::Middle,
                    bold: true,
                },
            ],
        };
        let s = svg(&page, "t");
        assert!(validate_svg(&s).is_empty());
        assert!(s.contains("<line x1=\"0\" y1=\"60\" x2=\"10\" y2=\"50\""));
        assert!(s.contains(">A&lt;B&gt;&amp;</text>"));
        assert!(s.contains("y=\"10\""));
        assert!(!validate_svg("<svg").is_empty());
    }
}
