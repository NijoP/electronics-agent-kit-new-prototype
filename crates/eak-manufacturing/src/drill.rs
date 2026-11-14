//! Excellon drill files (plated holes), metric, decimal coords: one file for the through
//! holes (through vias and through-hole pads) and one per blind or buried via span.

use eak_core::{CopperLayer, Nm, PadKind, Point, Project, ViaKind};
use std::collections::BTreeMap;
use std::fmt::Write;

/// The layer pair one drill file covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub from: CopperLayer,
    pub to: CopperLayer,
}

/// Drill diameter → positions for the through holes: through vias and THT pad holes.
pub fn holes(p: &Project) -> BTreeMap<Nm, Vec<Point>> {
    let mut h: BTreeMap<Nm, Vec<Point>> = BTreeMap::new();
    for v in p.pcb.vias.values().filter(|v| v.kind() == ViaKind::Through) {
        h.entry(v.drill).or_default().push(v.position);
    }
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        for pad in def.footprint.pads.iter().filter(|p| p.kind == PadKind::Tht) {
            if let Some(d) = pad.drill {
                h.entry(d).or_default().push(p.pad_position(fp, pad));
            }
        }
    }
    h
}

/// Blind and buried via holes, grouped by layer span (one drill file each), top-most span first.
pub fn span_holes(p: &Project) -> BTreeMap<Span, BTreeMap<Nm, Vec<Point>>> {
    let mut out: BTreeMap<Span, BTreeMap<Nm, Vec<Point>>> = BTreeMap::new();
    for v in p.pcb.vias.values().filter(|v| v.kind() != ViaKind::Through) {
        out.entry(Span {
            from: v.from,
            to: v.to,
        })
        .or_default()
        .entry(v.drill)
        .or_default()
        .push(v.position);
    }
    out
}

/// Excellon file of the through holes (`Plated,1,N,PTH`).
pub fn excellon(p: &Project) -> String {
    let n = p.pcb.board.stackup.copper_layers();
    render(p, &holes(p), &format!("Plated,1,{n},PTH"))
}

/// Excellon file of the blind or buried vias of one span.
pub fn excellon_span(p: &Project, span: Span) -> String {
    let st = &p.pcb.board.stackup;
    let holes = span_holes(p).remove(&span).unwrap_or_default();
    let (i, j) = (
        st.index_of(span.from).map_or(0, |i| i + 1),
        st.index_of(span.to).map_or(0, |i| i + 1),
    );
    let kind = if span.from.is_outer() || span.to.is_outer() {
        "Blind"
    } else {
        "Buried"
    };
    render(p, &holes, &format!("Plated,{i},{j},{kind}"))
}

fn render(p: &Project, h: &BTreeMap<Nm, Vec<Point>>, function: &str) -> String {
    let mut s = String::from("M48\n");
    let _ = writeln!(
        s,
        "; EAK {} project {} revision {}",
        env!("CARGO_PKG_VERSION"),
        p.id,
        p.revision
    );
    let _ = writeln!(s, "; #@! TF.FileFunction,{function}");
    s.push_str("FMAT,2\nMETRIC,TZ\n");
    for (i, d) in h.keys().enumerate() {
        let _ = writeln!(s, "T{}C{:.3}", i + 1, *d as f64 / 1e6);
    }
    s.push_str("%\nG90\nG05\n");
    for (i, pts) in h.values().enumerate() {
        let _ = writeln!(s, "T{}", i + 1);
        for pt in pts {
            let _ = writeln!(s, "X{:.4}Y{:.4}", pt.x as f64 / 1e6, pt.y as f64 / 1e6);
        }
    }
    s.push_str("T0\nM30\n");
    s
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DrillCheck {
    pub ok: bool,
    pub problems: Vec<String>,
    pub tools: usize,
    pub holes: usize,
}

pub fn validate(text: &str) -> DrillCheck {
    let mut c = DrillCheck::default();
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    if lines.next() != Some("M48") {
        c.problems.push("missing M48 header".into());
    }
    let mut tools = std::collections::BTreeSet::new();
    let (mut header, mut cur, mut end, mut metric) = (true, None::<String>, false, false);
    for l in lines {
        if l.starts_with(';') {
            continue;
        }
        if header {
            if l == "%" {
                header = false;
            } else if l.starts_with("METRIC") {
                metric = true;
            } else if let Some(rest) = l.strip_prefix('T') {
                let (num, dia) = rest.split_once('C').unwrap_or((rest, ""));
                if dia.parse::<f64>().map_or(true, |d| d <= 0.0) || !tools.insert(num.to_string()) {
                    c.problems.push(format!("bad tool definition {l}"));
                }
            }
            continue;
        }
        if l == "M30" {
            end = true;
        } else if let Some(t) = l.strip_prefix('T') {
            if t != "0" && !tools.contains(t) {
                c.problems
                    .push(format!("tool T{t} selected but not defined"));
            }
            cur = Some(t.to_string());
        } else if l.starts_with('X') {
            if cur.as_deref().is_none_or(|t| t == "0") {
                c.problems.push("hole without a selected tool".into());
            }
            c.holes += 1;
        }
    }
    if !metric {
        c.problems.push("units not declared METRIC".into());
    }
    if !end {
        c.problems.push("missing M30".into());
    }
    c.tools = tools.len();
    c.ok = c.problems.is_empty();
    c
}
