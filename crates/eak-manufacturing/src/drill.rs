//! Excellon drill file (plated holes: vias and through-hole pads), metric, decimal coords.

use eak_core::{Nm, PadKind, Point, Project};
use std::collections::BTreeMap;
use std::fmt::Write;

pub fn holes(p: &Project) -> BTreeMap<Nm, Vec<Point>> {
    let mut h: BTreeMap<Nm, Vec<Point>> = BTreeMap::new();
    for v in p.pcb.vias.values() {
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

pub fn excellon(p: &Project) -> String {
    let h = holes(p);
    let mut s = String::from("M48\n");
    let _ = writeln!(
        s,
        "; EAK {} project {} revision {}",
        env!("CARGO_PKG_VERSION"),
        p.id,
        p.revision
    );
    s.push_str("; #@! TF.FileFunction,Plated,1,2,PTH\nFMAT,2\nMETRIC,TZ\n");
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
