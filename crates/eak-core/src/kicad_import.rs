//! Optional one-way "Import from KiCad": converts KiCad-format symbol and footprint text files
//! into EAK-native definitions (ADR-0035, M3). EAK's own library format and IR stay the source of
//! truth; nothing in EAK needs KiCad, KiCad libraries or KiCad file formats to work, and no KiCad
//! library is bundled. Imported records carry provenance and are never marked verified.
//!
//! Independent implementation written from the public, documented S-expression file formats
//! (`.kicad_sym` symbol libraries, `.kicad_mod` footprints); no KiCad source is used. Input is read
//! with the bounded reader in [`crate::sexpr`]. Nothing is executed and no path is opened here.
//!
//! No silent data loss: every element that is not interpreted is counted in
//! [`ImportReport::unsupported`], every skipped or approximated record carries a note, and an
//! unusable record is `Rejected` with a reason rather than dropped. Imported definitions are at
//! most [`crate::libmeta::LibraryMaturity::Imported`]; nothing is verified by importing.
//!
//! Coordinates are taken in millimetres exactly as written. The symbol y-axis direction is not
//! converted (a note says so), because EAK's convention was not changed by this module.

use crate::geom::{Nm, Point};
use crate::library::{FootprintDef, PadDef, PadKind, PadShape, PinType, SymbolDef, SymbolPin};
use crate::sexpr::{parse, Limits, Node, ParseError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    Imported,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportItem {
    pub name: String,
    pub status: ImportStatus,
    pub notes: Vec<String>,
}

/// What an import did, in full.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImportReport {
    pub format: String,
    pub items: Vec<ImportItem>,
    /// `path -> count` of elements that were present but not interpreted.
    pub unsupported: BTreeMap<String, usize>,
}

impl ImportReport {
    pub fn imported(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.status == ImportStatus::Imported)
            .count()
    }
    pub fn rejected(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.status == ImportStatus::Rejected)
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolImport {
    /// Name in the source file.
    pub name: String,
    /// Name restricted to the stable item-id charset (see `libmeta::QualifiedId`).
    pub item_id: String,
    pub def: SymbolDef,
    /// Footprint reference text from the symbol's Footprint property (`lib:name`), unresolved.
    pub footprint: Option<String>,
    pub datasheet: Option<String>,
    pub refdes_prefix: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolLibImport {
    pub symbols: Vec<SymbolImport>,
    pub report: ImportReport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootprintImport {
    pub item_id: String,
    pub def: FootprintDef,
    pub description: Option<String>,
    /// 3D model path text as written; never opened or trusted.
    pub model_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootprintFileImport {
    pub footprint: Option<FootprintImport>,
    pub report: ImportReport,
}

/// Restrict a source name to `[A-Za-z0-9._+/-]` (max 128), replacing anything else with `_`.
pub fn sanitize_item_id(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '/' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    while s.contains("..") {
        s = s.replace("..", "._");
    }
    s.truncate(128);
    if s.is_empty() {
        s.push('_');
    }
    s
}

fn items(n: &Node) -> &[Node] {
    n.as_list().unwrap_or(&[])
}

fn num_at(n: &Node, idx: usize) -> Option<f64> {
    let v = items(n).get(idx)?.text()?.parse::<f64>().ok()?;
    v.is_finite().then_some(v)
}

fn mm_nm(v: f64) -> Nm {
    crate::geom::mm(v)
}

fn count(unsupported: &mut BTreeMap<String, usize>, key: String) {
    *unsupported.entry(key).or_insert(0) += 1;
}

fn track_unknown(
    unsupported: &mut BTreeMap<String, usize>,
    ctx: &str,
    node: &Node,
    known: &[&str],
) {
    for c in items(node) {
        if let Some(h) = c.head() {
            if !known.contains(&h) {
                count(unsupported, format!("{ctx}/{h}"));
            }
        }
    }
}

fn pin_type(t: &str) -> (PinType, Option<&'static str>) {
    match t {
        "input" => (PinType::Input, None),
        "output" => (PinType::Output, None),
        "bidirectional" => (PinType::Bidirectional, None),
        "tri_state" => (
            PinType::Bidirectional,
            Some("tri_state mapped to bidirectional"),
        ),
        "passive" => (PinType::Passive, None),
        "free" => (PinType::Passive, Some("free mapped to passive")),
        "unspecified" => (PinType::Passive, Some("unspecified mapped to passive")),
        "power_in" => (PinType::PowerIn, None),
        "power_out" => (PinType::PowerOut, None),
        "open_collector" => (PinType::OpenCollector, None),
        "open_emitter" => (
            PinType::OpenCollector,
            Some("open_emitter mapped to open_collector"),
        ),
        "no_connect" => (PinType::NoConnect, None),
        _ => (
            PinType::Passive,
            Some("unknown electrical type mapped to passive"),
        ),
    }
}

fn property(sym: &Node, key: &str) -> Option<String> {
    sym.children("property")
        .find(|p| items(p).get(1).and_then(Node::text) == Some(key))
        .and_then(|p| items(p).get(2).and_then(Node::text))
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
}

fn rejected(report: &mut ImportReport, name: &str, why: &str) {
    report.items.push(ImportItem {
        name: name.to_string(),
        status: ImportStatus::Rejected,
        notes: vec![why.to_string()],
    });
}

/// Parse a symbol library. A library that does not parse is an `Err`; individual bad symbols are
/// reported as `Rejected` items.
pub fn import_symbol_lib(src: &str, limits: Limits) -> Result<SymbolLibImport, ParseError> {
    let root = parse(src, limits)?;
    let mut report = ImportReport {
        format: "kicad_symbol_lib".into(),
        ..Default::default()
    };
    let mut symbols = Vec::new();
    if root.head() != Some("kicad_symbol_lib") {
        rejected(
            &mut report,
            "(file)",
            "top-level element is not kicad_symbol_lib",
        );
        return Ok(SymbolLibImport { symbols, report });
    }
    track_unknown(
        &mut report.unsupported,
        "lib",
        &root,
        &["version", "generator", "generator_version", "symbol"],
    );
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for sym in root.children("symbol") {
        let name = items(sym)
            .get(1)
            .and_then(Node::text)
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            rejected(&mut report, "(unnamed)", "symbol has no name");
            continue;
        }
        if !seen.insert(name.clone()) {
            rejected(&mut report, &name, "duplicate symbol name in this library");
            continue;
        }
        if sym.child("extends").is_some() {
            rejected(
                &mut report,
                &name,
                "derived symbol (extends) is not supported; import its parent",
            );
            continue;
        }
        track_unknown(
            &mut report.unsupported,
            "symbol",
            sym,
            &[
                "pin_names",
                "pin_numbers",
                "in_bom",
                "on_board",
                "property",
                "symbol",
                "exclude_from_sim",
            ],
        );
        let mut notes: Vec<String> = Vec::new();
        let mut pins: Vec<SymbolPin> = Vec::new();
        let mut numbers: BTreeSet<String> = BTreeSet::new();
        let mut body: Option<(f64, f64)> = None;
        let mut units: BTreeSet<u32> = BTreeSet::new();
        for sub in sym.children("symbol") {
            // Sub-symbol names are `<name>_<unit>_<style>`; style 2 is the alternate drawing.
            let sname = items(sub).get(1).and_then(Node::text).unwrap_or("");
            let mut it = sname.rsplit('_');
            let style = it.next().and_then(|s| s.parse::<u32>().ok());
            let unit = it.next().and_then(|s| s.parse::<u32>().ok());
            if let Some(u) = unit {
                if u > 0 {
                    units.insert(u);
                }
            }
            if style == Some(2) {
                count(
                    &mut report.unsupported,
                    "symbol/alternate_body_style".into(),
                );
                continue;
            }
            for g in items(sub) {
                match g.head() {
                    Some("pin") | None => {}
                    Some("rectangle") => {
                        let sx = g.child("start").and_then(|n| num_at(n, 1));
                        let sy = g.child("start").and_then(|n| num_at(n, 2));
                        let ex = g.child("end").and_then(|n| num_at(n, 1));
                        let ey = g.child("end").and_then(|n| num_at(n, 2));
                        if let (Some(sx), Some(sy), Some(ex), Some(ey)) = (sx, sy, ex, ey) {
                            let (w, h) = ((ex - sx).abs() / 2.0, (ey - sy).abs() / 2.0);
                            body = Some(match body {
                                Some((bw, bh)) => (bw.max(w), bh.max(h)),
                                None => (w, h),
                            });
                        }
                    }
                    Some(other) => {
                        count(&mut report.unsupported, format!("symbol_graphics/{other}"))
                    }
                }
            }
            for p in sub.children("pin") {
                track_unknown(
                    &mut report.unsupported,
                    "pin",
                    p,
                    &["at", "length", "name", "number", "hide"],
                );
                let tname = items(p).get(1).and_then(Node::text).unwrap_or("");
                let (ty, note) = pin_type(tname);
                if let Some(n) = note {
                    notes.push(format!("pin: {n}"));
                }
                let number = p
                    .child("number")
                    .and_then(|n| items(n).get(1))
                    .and_then(Node::text)
                    .unwrap_or("")
                    .to_string();
                let pname = p
                    .child("name")
                    .and_then(|n| items(n).get(1))
                    .and_then(Node::text)
                    .unwrap_or("")
                    .to_string();
                let (x, y) = match p.child("at") {
                    Some(a) => (num_at(a, 1), num_at(a, 2)),
                    None => (None, None),
                };
                let (Some(x), Some(y)) = (x, y) else {
                    notes.push(format!("pin '{number}' has no usable position; skipped"));
                    continue;
                };
                if number.is_empty() {
                    notes.push("pin without a number skipped".into());
                    continue;
                }
                if !numbers.insert(number.clone()) {
                    notes.push(format!(
                        "pin number {number} repeated (another unit); kept the first"
                    ));
                    continue;
                }
                if p.child("hide").is_some() {
                    notes.push(format!(
                        "pin {number} is hidden in the source; imported as a normal pin"
                    ));
                }
                pins.push(SymbolPin {
                    number,
                    name: if pname == "~" { String::new() } else { pname },
                    electrical_type: ty,
                    position: Point::mm(x, y),
                });
            }
        }
        if units.len() > 1 {
            notes.push(format!("{} units flattened into one symbol", units.len()));
        }
        if pins.is_empty() {
            rejected(&mut report, &name, "symbol has no usable pins");
            continue;
        }
        let (bw, bh) = match body {
            Some(b) => b,
            None => {
                notes.push("no body rectangle; size derived from pin positions".into());
                let mx = pins.iter().map(|p| p.position.x.abs()).max().unwrap_or(0) as f64 / 1e6;
                let my = pins.iter().map(|p| p.position.y.abs()).max().unwrap_or(0) as f64 / 1e6;
                (mx.max(1.27), my.max(1.27))
            }
        };
        notes.push("coordinates kept as written (y-axis direction not converted)".into());
        let item_id = sanitize_item_id(&name);
        if item_id != name {
            notes.push(format!("item id sanitised to '{item_id}'"));
        }
        let refdes = property(sym, "Reference").filter(|r| r != "#PWR" && !r.contains('?'));
        symbols.push(SymbolImport {
            name: name.clone(),
            item_id,
            def: SymbolDef {
                pins,
                body_half_w: mm_nm(bw),
                body_half_h: mm_nm(bh),
            },
            footprint: property(sym, "Footprint"),
            datasheet: property(sym, "Datasheet").filter(|d| d != "~"),
            refdes_prefix: refdes,
            value: property(sym, "Value"),
        });
        report.items.push(ImportItem {
            name,
            status: ImportStatus::Imported,
            notes,
        });
    }
    Ok(SymbolLibImport { symbols, report })
}

fn pad_shape(s: &str) -> (PadShape, Option<&'static str>) {
    match s {
        "rect" => (PadShape::Rect, None),
        "circle" => (PadShape::Circle, None),
        "oval" => (PadShape::Oval, None),
        "roundrect" => (PadShape::Rect, Some("roundrect imported as rect")),
        "trapezoid" => (PadShape::Rect, Some("trapezoid imported as rect")),
        "custom" => (
            PadShape::Rect,
            Some("custom pad imported as its anchor rect"),
        ),
        _ => (PadShape::Rect, Some("unknown pad shape imported as rect")),
    }
}

fn drill_of(pad: &Node) -> Option<f64> {
    let d = pad.child("drill")?;
    items(d)
        .iter()
        .skip(1)
        .filter_map(|n| n.text()?.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |x| x.min(v))))
}

/// Parse one footprint file.
pub fn import_footprint(src: &str, limits: Limits) -> Result<FootprintFileImport, ParseError> {
    let root = parse(src, limits)?;
    let mut report = ImportReport {
        format: "kicad_footprint".into(),
        ..Default::default()
    };
    let head = root.head();
    if head != Some("footprint") && head != Some("module") {
        rejected(&mut report, "(file)", "top-level element is not footprint");
        return Ok(FootprintFileImport {
            footprint: None,
            report,
        });
    }
    let name = items(&root)
        .get(1)
        .and_then(Node::text)
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        rejected(&mut report, "(unnamed)", "footprint has no name");
        return Ok(FootprintFileImport {
            footprint: None,
            report,
        });
    }
    let mut notes: Vec<String> = Vec::new();
    track_unknown(
        &mut report.unsupported,
        "footprint",
        &root,
        &[
            "pad",
            "model",
            "fp_line",
            "fp_rect",
            "fp_circle",
            "fp_poly",
            "layer",
            "descr",
            "tags",
            "version",
            "generator",
            "generator_version",
            "uuid",
            "tstamp",
            "attr",
            "at",
        ],
    );
    let mut pads: Vec<PadDef> = Vec::new();
    let mut numbers: BTreeSet<String> = BTreeSet::new();
    for p in root.children("pad") {
        track_unknown(
            &mut report.unsupported,
            "pad",
            p,
            &["at", "size", "drill", "layers", "rect_delta"],
        );
        let number = items(p)
            .get(1)
            .and_then(Node::text)
            .unwrap_or("")
            .to_string();
        let ptype = items(p).get(2).and_then(Node::text).unwrap_or("");
        let shape_text = items(p).get(3).and_then(Node::text).unwrap_or("");
        let kind = match ptype {
            "smd" => PadKind::Smd,
            "thru_hole" => PadKind::Tht,
            other => {
                notes.push(format!(
                    "pad '{number}' of type {other} skipped (mechanical or connect-only)"
                ));
                continue;
            }
        };
        if number.is_empty() {
            notes.push("unnumbered pad skipped".into());
            continue;
        }
        if !numbers.insert(number.clone()) {
            notes.push(format!(
                "pad number {number} repeated; kept the first (split or thermal pads are not merged)"
            ));
            continue;
        }
        let (x, y, rot) = match p.child("at") {
            Some(a) => (num_at(a, 1), num_at(a, 2), num_at(a, 3).unwrap_or(0.0)),
            None => (None, None, 0.0),
        };
        let (w, h) = match p.child("size") {
            Some(s) => (num_at(s, 1), num_at(s, 2).or_else(|| num_at(s, 1))),
            None => (None, None),
        };
        let (Some(x), Some(y), Some(mut w), Some(mut h)) = (x, y, w, h) else {
            notes.push(format!("pad {number} lacks position or size; skipped"));
            continue;
        };
        let r = rot.rem_euclid(360.0);
        if (r - 90.0).abs() < 1e-6 || (r - 270.0).abs() < 1e-6 {
            std::mem::swap(&mut w, &mut h);
        } else if r.abs() > 1e-6 && (r - 180.0).abs() > 1e-6 {
            notes.push(format!(
                "pad {number} rotated {r} degrees imported unrotated"
            ));
        }
        let (shape, note) = pad_shape(shape_text);
        if let Some(n) = note {
            notes.push(format!("pad {number}: {n}"));
        }
        let drill = if kind == PadKind::Tht {
            drill_of(p)
        } else {
            None
        };
        if kind == PadKind::Tht && drill.is_none() {
            notes.push(format!("through-hole pad {number} has no drill; skipped"));
            continue;
        }
        pads.push(PadDef {
            number,
            kind,
            shape,
            position: Point::mm(x, y),
            size_x: mm_nm(w),
            size_y: mm_nm(h),
            drill: drill.map(mm_nm),
        });
    }
    if pads.is_empty() {
        rejected(&mut report, &name, "footprint has no usable pads");
        return Ok(FootprintFileImport {
            footprint: None,
            report,
        });
    }
    // Courtyard: extent of graphics on the courtyard layers.
    let (mut cx, mut cy, mut have_court) = (0.0f64, 0.0f64, false);
    for g in items(&root) {
        let Some(h) = g.head() else { continue };
        if !matches!(h, "fp_line" | "fp_rect" | "fp_circle" | "fp_poly") {
            continue;
        }
        let layer = g
            .child("layer")
            .and_then(|l| items(l).get(1))
            .and_then(Node::text)
            .unwrap_or("");
        if !layer.ends_with("CrtYd") {
            count(
                &mut report.unsupported,
                format!("footprint_graphics/{h}:{layer}"),
            );
            continue;
        }
        let mut pts: Vec<(f64, f64)> = Vec::new();
        for key in ["start", "end", "center"] {
            if let Some(n) = g.child(key) {
                if let (Some(x), Some(y)) = (num_at(n, 1), num_at(n, 2)) {
                    pts.push((x, y));
                }
            }
        }
        if let Some(poly) = g.child("pts") {
            for xy in poly.children("xy") {
                if let (Some(x), Some(y)) = (num_at(xy, 1), num_at(xy, 2)) {
                    pts.push((x, y));
                }
            }
        }
        for (x, y) in pts {
            cx = cx.max(x.abs());
            cy = cy.max(y.abs());
            have_court = true;
        }
    }
    let (court_w, court_h) = if have_court {
        (cx, cy)
    } else {
        notes.push("no courtyard in the file; derived from pad extents plus 0.25 mm".into());
        let ex = pads
            .iter()
            .map(|p| (p.position.x.abs() + p.size_x / 2) as f64 / 1e6)
            .fold(0.0, f64::max);
        let ey = pads
            .iter()
            .map(|p| (p.position.y.abs() + p.size_y / 2) as f64 / 1e6)
            .fold(0.0, f64::max);
        (ex + 0.25, ey + 0.25)
    };
    let model_path = root
        .child("model")
        .and_then(|m| items(m).get(1))
        .and_then(Node::text)
        .map(str::to_string);
    if root.children("model").count() > 1 {
        notes.push("more than one 3D model; only the first is recorded".into());
    }
    let item_id = sanitize_item_id(&name);
    if item_id != name {
        notes.push(format!("item id sanitised to '{item_id}'"));
    }
    let description = root
        .child("descr")
        .and_then(|d| items(d).get(1))
        .and_then(Node::text)
        .map(str::to_string);
    report.items.push(ImportItem {
        name: name.clone(),
        status: ImportStatus::Imported,
        notes,
    });
    Ok(FootprintFileImport {
        footprint: Some(FootprintImport {
            item_id,
            def: FootprintDef {
                name,
                pads,
                courtyard_half_w: mm_nm(court_w),
                courtyard_half_h: mm_nm(court_h),
                body_height: None,
            },
            description,
            model_path,
        }),
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYM: &str = r#"(kicad_symbol_lib (version 20231120) (generator "x")
      (symbol "R_0805" (in_bom yes) (on_board yes)
        (property "Reference" "R" (at 0 0 0))
        (property "Value" "10k" (at 0 0 0))
        (property "Footprint" "Resistor_SMD:R_0805" (at 0 0 0))
        (property "Datasheet" "~" (at 0 0 0))
        (symbol "R_0805_0_1" (rectangle (start -1 -2) (end 1 2) (stroke (width 0)))
          (polyline (pts (xy 0 0) (xy 1 1))))
        (symbol "R_0805_1_1"
          (pin passive line (at 0 3.81 270) (length 1.27) (name "~" (effects (font (size 1 1)))) (number "1" (effects (font (size 1 1)))))
          (pin free line (at 0 -3.81 90) (length 1.27) (name "B") (number "2"))))
      (symbol "EMPTY" (symbol "EMPTY_1_1"))
      (symbol "R_0805"))"#;

    #[test]
    fn symbol_import_maps_pins_and_reports_everything_not_used() {
        let r = import_symbol_lib(SYM, Limits::default()).unwrap();
        assert_eq!(r.symbols.len(), 1);
        let s = &r.symbols[0];
        assert_eq!(s.def.pins.len(), 2);
        assert_eq!(s.def.pins[0].electrical_type, PinType::Passive);
        assert_eq!(s.def.pins[0].name, "");
        assert_eq!(s.def.body_half_w, 1_000_000);
        assert_eq!(s.def.body_half_h, 2_000_000);
        assert_eq!(s.footprint.as_deref(), Some("Resistor_SMD:R_0805"));
        assert_eq!(s.datasheet, None);
        assert_eq!(s.refdes_prefix.as_deref(), Some("R"));
        assert_eq!(r.report.imported(), 1);
        assert_eq!(
            r.report.rejected(),
            2,
            "EMPTY and the duplicate must be rejected visibly"
        );
        assert_eq!(
            r.report.unsupported.get("symbol_graphics/polyline"),
            Some(&1)
        );
        let notes = &r.report.items[0].notes;
        assert!(notes.iter().any(|n| n.contains("free mapped to passive")));
    }

    #[test]
    fn symbol_with_wrong_root_or_extends_is_rejected_not_dropped() {
        let r = import_symbol_lib("(something_else)", Limits::default()).unwrap();
        assert_eq!(r.report.rejected(), 1);
        let r = import_symbol_lib(
            r#"(kicad_symbol_lib (symbol "D" (extends "P")))"#,
            Limits::default(),
        )
        .unwrap();
        assert_eq!(r.report.rejected(), 1);
        assert!(import_symbol_lib("(kicad_symbol_lib", Limits::default()).is_err());
    }

    const FP: &str = r#"(footprint "R_0805" (version 20240108) (layer "F.Cu") (descr "resistor 0805")
      (fp_text reference "REF**" (at 0 0) (layer "F.SilkS"))
      (fp_line (start -1.7 -0.95) (end 1.7 -0.95) (layer "F.CrtYd") (width 0.05))
      (fp_line (start 1.7 -0.95) (end 1.7 0.95) (layer "F.CrtYd") (width 0.05))
      (fp_line (start -1 -0.5) (end 1 -0.5) (layer "F.SilkS"))
      (pad "1" smd roundrect (at -0.9125 0) (size 1.025 1.4) (layers "F.Cu" "F.Paste" "F.Mask"))
      (pad "2" smd roundrect (at 0.9125 0 90) (size 1.025 1.4) (layers "F.Cu"))
      (pad "2" smd rect (at 5 0) (size 1 1))
      (pad "" np_thru_hole circle (at 0 3) (size 1 1) (drill 1))
      (model "${KIPRJMOD}/r.step" (offset (xyz 0 0 0))))"#;

    #[test]
    fn footprint_import_keeps_pads_courtyard_and_reports_skips() {
        let r = import_footprint(FP, Limits::default()).unwrap();
        let f = r.footprint.expect("imported");
        assert_eq!(f.def.pads.len(), 2);
        assert_eq!(f.def.pads[0].shape, PadShape::Rect);
        // Second pad was rotated 90 degrees: width and height swap.
        assert_eq!(f.def.pads[1].size_x, 1_400_000);
        assert_eq!(f.def.pads[1].size_y, 1_025_000);
        assert_eq!(f.def.courtyard_half_w, 1_700_000);
        assert_eq!(f.def.courtyard_half_h, 950_000);
        assert_eq!(f.model_path.as_deref(), Some("${KIPRJMOD}/r.step"));
        assert_eq!(f.description.as_deref(), Some("resistor 0805"));
        let notes = &r.report.items[0].notes;
        assert!(notes.iter().any(|n| n.contains("repeated")));
        assert!(notes.iter().any(|n| n.contains("np_thru_hole")));
        assert_eq!(r.report.unsupported.get("footprint/fp_text"), Some(&1));
        assert_eq!(
            r.report
                .unsupported
                .get("footprint_graphics/fp_line:F.SilkS"),
            Some(&1)
        );
    }

    #[test]
    fn footprint_without_pads_or_with_wrong_root_is_rejected() {
        let r = import_footprint(r#"(footprint "X")"#, Limits::default()).unwrap();
        assert!(r.footprint.is_none());
        assert_eq!(r.report.rejected(), 1);
        let r = import_footprint("(nope)", Limits::default()).unwrap();
        assert!(r.footprint.is_none());
    }

    #[test]
    fn sanitize_item_id_is_safe() {
        assert_eq!(sanitize_item_id("R 0805:x"), "R_0805_x");
        assert_eq!(sanitize_item_id("../etc"), "._/etc");
        assert_eq!(sanitize_item_id(""), "_");
    }
}
