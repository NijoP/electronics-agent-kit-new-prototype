//! Bill of materials derived from the EAK-IR, joined with locally stored LCSC sourcing
//! records.
//!
//! The BOM is computed, never stored: it is a view of the schematic symbols, the library
//! definitions they reference and the sourcing records the user has on disk. Stock and price
//! come only from a stored record and are shown with the record's retrieval time; they are
//! a snapshot for review, not a quote.

use crate::sourcing::SourcingRecord;
use eak_core::{PadKind, Project};
use eak_validation::sourcing::is_lcsc_part_number;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BomLine {
    pub component_id: String,
    /// Value as placed on the symbol (JLCPCB "Comment").
    pub value: String,
    pub designators: Vec<String>,
    pub quantity: usize,
    pub footprint: String,
    pub lcsc: Option<String>,
    pub manufacturer: Option<String>,
    pub mpn: Option<String>,
    /// Every pad of the footprint is surface mount.
    pub smd: bool,
    /// Designators that have a footprint placed on the PCB.
    pub placed: usize,
    /// Stock at the time the stored LCSC record was retrieved.
    pub stock: Option<u64>,
    /// Unit price (USD, decimal text from LCSC) at this line's quantity.
    pub unit_price_usd: Option<String>,
    /// Unix seconds the stored record was retrieved.
    pub price_as_of_unix_s: Option<u64>,
    pub issues: Vec<String>,
}

impl BomLine {
    /// The line belongs in the JLCPCB assembly BOM: surface mount and placed on the board.
    pub fn assembled(&self) -> bool {
        self.smd && self.placed > 0
    }
}

fn natural(s: &str) -> (String, u64) {
    let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    (
        s.chars().filter(|c| !c.is_ascii_digit()).collect(),
        d.parse().unwrap_or(0),
    )
}

/// Group the schematic's symbols into BOM lines and attach sourcing facts.
pub fn build(p: &Project, records: &[SourcingRecord]) -> Vec<BomLine> {
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for s in p.schematic.symbols.values() {
        groups
            .entry((s.component_id.clone(), s.value.clone()))
            .or_default()
            .push(s.refdes.clone());
    }
    let placed: std::collections::BTreeSet<&str> = p
        .pcb
        .footprints
        .values()
        .map(|f| f.refdes.as_str())
        .collect();
    let mut out = vec![];
    for ((comp, value), mut refs) in groups {
        refs.sort_by_key(|a| natural(a));
        let n_placed = refs.iter().filter(|r| placed.contains(r.as_str())).count();
        let Some(def) = p.library.get(&comp) else {
            out.push(BomLine {
                issues: vec![format!("component {comp} is not in the project library")],
                component_id: comp,
                value,
                quantity: refs.len(),
                placed: n_placed,
                designators: refs,
                footprint: String::new(),
                lcsc: None,
                manufacturer: None,
                mpn: None,
                smd: false,
                stock: None,
                unit_price_usd: None,
                price_as_of_unix_s: None,
            });
            continue;
        };
        let lcsc = def
            .lcsc
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);
        let mut issues = vec![];
        match &lcsc {
            None => issues.push("no LCSC part number".to_string()),
            Some(c) if !is_lcsc_part_number(c) => {
                issues.push(format!("{c:?} is not an LCSC part number"))
            }
            _ => {}
        }
        let record = lcsc.as_ref().and_then(|c| {
            records.iter().find(|r| {
                r.lcsc_part
                    .as_deref()
                    .is_some_and(|x| x.eq_ignore_ascii_case(c))
            })
        });
        let qty = refs.len() as u64;
        let (stock, unit_price_usd, as_of) = match record {
            Some(r) => {
                if let Some(s) = r.stock.filter(|s| *s < qty) {
                    issues.push(format!("stored LCSC stock {s} is below the quantity {qty}"));
                }
                let price = r
                    .price_breaks
                    .iter()
                    .filter(|b| b.min_quantity <= qty)
                    .max_by_key(|b| b.min_quantity)
                    .or_else(|| r.price_breaks.iter().min_by_key(|b| b.min_quantity))
                    .map(|b| b.unit_price.clone());
                (r.stock, price, Some(r.retrieved_at_unix_s))
            }
            None => (None, None, None),
        };
        let smd = !def.footprint.pads.is_empty()
            && def.footprint.pads.iter().all(|pd| pd.kind == PadKind::Smd);
        if n_placed < refs.len() {
            issues.push(format!(
                "{} of {} designator(s) have no footprint on the PCB",
                refs.len() - n_placed,
                refs.len()
            ));
        }
        out.push(BomLine {
            component_id: comp,
            value,
            quantity: refs.len(),
            placed: n_placed,
            designators: refs,
            footprint: def.footprint.name.clone(),
            lcsc,
            manufacturer: def.manufacturer.clone(),
            mpn: def.mpn.clone(),
            smd,
            stock,
            unit_price_usd,
            price_as_of_unix_s: as_of,
            issues,
        });
    }
    out
}

fn q(s: &str) -> String {
    // Neutralise spreadsheet formula injection as well as CSV quoting.
    let s = if s.starts_with(['=', '+', '-', '@']) {
        format!("'{s}")
    } else {
        s.to_string()
    };
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// JLCPCB assembly BOM: the four columns of JLCPCB's own template, surface-mount lines that
/// are placed on the board.
pub fn jlcpcb_csv(lines: &[BomLine]) -> String {
    let mut out = String::from("Comment,Designator,Footprint,LCSC Part #\n");
    for l in lines.iter().filter(|l| l.assembled()) {
        out.push_str(&format!(
            "{},{},{},{}\n",
            q(&l.value),
            q(&l.designators.join(",")),
            q(&l.footprint),
            q(l.lcsc.as_deref().unwrap_or(""))
        ));
    }
    out
}

/// Review BOM: every line, with manufacturer, MPN, quantity, the stored stock/price
/// snapshot and issues. Not for upload.
pub fn detail_csv(lines: &[BomLine]) -> String {
    let mut out = String::from(
        "Comment,Designator,Footprint,LCSC Part #,Manufacturer,MPN,Quantity,Assembled by JLCPCB,Stock (stored record),Unit price USD (stored record),Issues\n",
    );
    for l in lines {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{}\n",
            q(&l.value),
            q(&l.designators.join(",")),
            q(&l.footprint),
            q(l.lcsc.as_deref().unwrap_or("")),
            q(l.manufacturer.as_deref().unwrap_or("")),
            q(l.mpn.as_deref().unwrap_or("")),
            l.quantity,
            if l.assembled() {
                "yes"
            } else if l.smd {
                "no (not placed)"
            } else {
                "no (through-hole or unplaced)"
            },
            l.stock.map(|s| s.to_string()).unwrap_or_default(),
            q(l.unit_price_usd.as_deref().unwrap_or("")),
            q(&l.issues.join("; "))
        ));
    }
    out
}
