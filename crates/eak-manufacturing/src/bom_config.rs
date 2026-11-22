//! Configurable BOM export (gap #40): which columns, in which order and under which headers,
//! how lines are grouped and sorted, and which lines are removed by filter rules.
//!
//! The configuration is plain data (JSON), so it can be saved next to an export and loaded
//! again: a configuration read back from its own JSON is the same configuration. The table it
//! produces is a view of [`crate::bom::build`]; nothing here changes the project.

use crate::bom::{self, BomLine};
use crate::sourcing::SourcingRecord;
use eak_core::Project;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeSet;

/// Version of the configuration format written by this crate.
pub const CONFIG_VERSION: u32 = 1;

/// One BOM column a user can pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BomField {
    /// Value as placed on the symbol ("Comment" in fabricator templates).
    Comment,
    Designator,
    Quantity,
    Footprint,
    /// Library component the line uses.
    ComponentId,
    Manufacturer,
    Mpn,
    DistributorPart,
    /// Every pad of the footprint is surface mount.
    Smd,
    /// Designators with a footprint on the PCB.
    Placed,
    /// Surface mount and placed on the board (the fabricator assembly BOM).
    Assembled,
    /// Stock in the stored Distributor record (a snapshot, not a quote).
    Stock,
    /// Unit price (USD) in the stored Distributor record at this line's quantity.
    UnitPriceUsd,
    /// Date (UTC) the stored Distributor record was retrieved.
    PriceAsOf,
    Issues,
}

impl BomField {
    /// Every field, in the order the column picker lists them.
    pub const ALL: [BomField; 15] = [
        BomField::Comment,
        BomField::Designator,
        BomField::Quantity,
        BomField::Footprint,
        BomField::ComponentId,
        BomField::Manufacturer,
        BomField::Mpn,
        BomField::DistributorPart,
        BomField::Smd,
        BomField::Placed,
        BomField::Assembled,
        BomField::Stock,
        BomField::UnitPriceUsd,
        BomField::PriceAsOf,
        BomField::Issues,
    ];

    /// Header used when a column does not set its own.
    pub fn default_header(self) -> &'static str {
        match self {
            BomField::Comment => "Comment",
            BomField::Designator => "Designator",
            BomField::Quantity => "Quantity",
            BomField::Footprint => "Footprint",
            BomField::ComponentId => "Component",
            BomField::Manufacturer => "Manufacturer",
            BomField::Mpn => "MPN",
            BomField::DistributorPart => "Distributor Part #",
            BomField::Smd => "SMD",
            BomField::Placed => "Placed",
            BomField::Assembled => "Assembled by Fabricator",
            BomField::Stock => "Stock (stored record)",
            BomField::UnitPriceUsd => "Unit price USD (stored record)",
            BomField::PriceAsOf => "Price as of (UTC)",
            BomField::Issues => "Issues",
        }
    }

    /// Fields whose cells are numbers (written as numbers in XLSX).
    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            BomField::Quantity | BomField::Placed | BomField::Stock | BomField::UnitPriceUsd
        )
    }

    fn cell(self, l: &BomLine) -> String {
        let opt = |o: &Option<String>| o.clone().unwrap_or_default();
        let yes = |b: bool| if b { "yes" } else { "no" }.to_string();
        match self {
            BomField::Comment => l.value.clone(),
            BomField::Designator => l.designators.join(","),
            BomField::Quantity => l.quantity.to_string(),
            BomField::Footprint => l.footprint.clone(),
            BomField::ComponentId => l.component_id.clone(),
            BomField::Manufacturer => opt(&l.manufacturer),
            BomField::Mpn => opt(&l.mpn),
            BomField::DistributorPart => opt(&l.distributor),
            BomField::Smd => yes(l.smd),
            BomField::Placed => l.placed.to_string(),
            BomField::Assembled => yes(l.assembled()),
            BomField::Stock => l.stock.map(|s| s.to_string()).unwrap_or_default(),
            BomField::UnitPriceUsd => opt(&l.unit_price_usd),
            BomField::PriceAsOf => l.price_as_of_unix_s.map(utc_date).unwrap_or_default(),
            BomField::Issues => l.issues.join("; "),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDir {
    #[default]
    None,
    Asc,
    Desc,
}

/// One column of the exported table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BomColumn {
    pub field: BomField,
    /// Header text; `None` uses [`BomField::default_header`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    /// Sort on this column. Several sorted columns sort in column order (the first is the
    /// primary key).
    #[serde(default)]
    pub sort: SortDir,
}

impl BomColumn {
    pub fn new(field: BomField) -> Self {
        BomColumn {
            field,
            header: None,
            sort: SortDir::None,
        }
    }

    pub fn header_text(&self) -> String {
        self.header
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .unwrap_or(self.field.default_header())
            .to_string()
    }
}

/// How symbols are grouped into rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grouping {
    /// One row per part to buy: same component and value.
    #[default]
    Part,
    /// One row per designator (quantity 1).
    Designator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Equals,
    NotEquals,
    Contains,
    IsEmpty,
    NotEmpty,
}

/// "Remove rows where `field` `op` `value`" (text comparison, case-insensitive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterRule {
    pub field: BomField,
    pub op: FilterOp,
    #[serde(default)]
    pub value: String,
}

impl FilterRule {
    fn removes(&self, cell: &str) -> bool {
        let c = cell.trim().to_lowercase();
        let v = self.value.trim().to_lowercase();
        match self.op {
            FilterOp::Equals => c == v,
            FilterOp::NotEquals => c != v,
            FilterOp::Contains => c.contains(&v),
            FilterOp::IsEmpty => c.is_empty(),
            FilterOp::NotEmpty => !c.is_empty(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BomConfig {
    #[serde(default = "config_version")]
    pub version: u32,
    pub columns: Vec<BomColumn>,
    #[serde(default)]
    pub grouping: Grouping,
    /// Rows matching any rule are removed.
    #[serde(default)]
    pub exclude: Vec<FilterRule>,
}

fn config_version() -> u32 {
    CONFIG_VERSION
}

impl Default for BomConfig {
    /// The review BOM: what to buy, sorted by designator.
    fn default() -> Self {
        let mut designator = BomColumn::new(BomField::Designator);
        designator.sort = SortDir::Asc;
        BomConfig {
            version: CONFIG_VERSION,
            columns: vec![
                BomColumn::new(BomField::Comment),
                designator,
                BomColumn::new(BomField::Footprint),
                BomColumn::new(BomField::DistributorPart),
                BomColumn::new(BomField::Manufacturer),
                BomColumn::new(BomField::Mpn),
                BomColumn::new(BomField::Quantity),
            ],
            grouping: Grouping::Part,
            exclude: vec![],
        }
    }
}

impl BomConfig {
    /// Problems that make the configuration unusable (empty when it can be applied).
    pub fn problems(&self) -> Vec<String> {
        let mut out = vec![];
        if self.version > CONFIG_VERSION {
            out.push(format!(
                "BOM configuration version {} is newer than this EAK supports ({CONFIG_VERSION})",
                self.version
            ));
        }
        if self.columns.is_empty() {
            out.push("choose at least one BOM column".into());
        }
        let mut seen = BTreeSet::new();
        for c in &self.columns {
            if !seen.insert(c.field) {
                out.push(format!(
                    "column {} is listed twice",
                    c.field.default_header()
                ));
            }
        }
        for r in &self.exclude {
            if r.op == FilterOp::Contains && r.value.trim().is_empty() {
                out.push(format!(
                    "the filter on {} needs a value to look for",
                    r.field.default_header()
                ));
            }
        }
        out
    }

    /// JSON of the configuration (what an export saves next to the BOM).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Result<BomConfig, String> {
        let c: BomConfig =
            serde_json::from_str(json).map_err(|e| format!("not a BOM configuration: {e}"))?;
        match c.problems().first() {
            Some(p) => Err(p.clone()),
            None => Ok(c),
        }
    }
}

/// The table a configuration produces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BomTable {
    pub fields: Vec<BomField>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Rows removed by the filter rules.
    pub removed: usize,
    /// Sum of the quantity of the rows kept.
    pub parts: usize,
}

/// Apply `cfg` to the project's BOM.
pub fn table(p: &Project, records: &[SourcingRecord], cfg: &BomConfig) -> Result<BomTable, String> {
    if let Some(problem) = cfg.problems().first() {
        return Err(problem.clone());
    }
    let mut lines = bom::build(p, records);
    if cfg.grouping == Grouping::Designator {
        let placed: BTreeSet<&str> = p
            .pcb
            .footprints
            .values()
            .map(|f| f.refdes.as_str())
            .collect();
        let mut single = vec![];
        for l in &lines {
            for d in &l.designators {
                single.push(BomLine {
                    quantity: 1,
                    placed: usize::from(placed.contains(d.as_str())),
                    designators: vec![d.clone()],
                    ..l.clone()
                });
            }
        }
        lines = single;
    }
    let before = lines.len();
    lines.retain(|l| !cfg.exclude.iter().any(|r| r.removes(&r.field.cell(l))));
    let removed = before - lines.len();
    let sorted: Vec<&BomColumn> = cfg
        .columns
        .iter()
        .filter(|c| c.sort != SortDir::None)
        .collect();
    lines.sort_by(|a, b| {
        for c in &sorted {
            let o = natural_cmp(&c.field.cell(a), &c.field.cell(b));
            let o = if c.sort == SortDir::Desc {
                o.reverse()
            } else {
                o
            };
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    });
    Ok(BomTable {
        fields: cfg.columns.iter().map(|c| c.field).collect(),
        headers: cfg.columns.iter().map(BomColumn::header_text).collect(),
        rows: lines
            .iter()
            .map(|l| cfg.columns.iter().map(|c| c.field.cell(l)).collect())
            .collect(),
        removed,
        parts: lines.iter().map(|l| l.quantity).sum(),
    })
}

/// CSV of a table (quoted, with spreadsheet formula injection neutralised).
pub fn csv(t: &BomTable) -> String {
    let line = |cells: &[String]| {
        let mut s = cells
            .iter()
            .map(|c| bom::q(c))
            .collect::<Vec<_>>()
            .join(",");
        s.push('\n');
        s
    };
    let mut out = line(&t.headers);
    for r in &t.rows {
        out.push_str(&line(r));
    }
    out
}

/// XLSX workbook of a table (one sheet named "BOM").
pub fn xlsx(t: &BomTable) -> Vec<u8> {
    let numeric: Vec<bool> = t.fields.iter().map(|f| f.is_numeric()).collect();
    crate::xlsx::workbook("BOM", &t.headers, &t.rows, &numeric)
}

/// Natural order: numbers compare by value ("R2" before "R10"), text case-insensitively.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    if let (Ok(x), Ok(y)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        if let Some(o) = x.partial_cmp(&y) {
            return o;
        }
    }
    let (ca, cb) = (chunks(a), chunks(b));
    for (x, y) in ca.iter().zip(cb.iter()) {
        let o = match (x, y) {
            (Chunk::Num(p), Chunk::Num(q)) => {
                let (p, q) = (p.trim_start_matches('0'), q.trim_start_matches('0'));
                p.len().cmp(&q.len()).then_with(|| p.cmp(q))
            }
            (Chunk::Num(_), Chunk::Text(_)) => Ordering::Less,
            (Chunk::Text(_), Chunk::Num(_)) => Ordering::Greater,
            (Chunk::Text(p), Chunk::Text(q)) => p.cmp(q),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    ca.len().cmp(&cb.len())
}

enum Chunk {
    Num(String),
    Text(String),
}

fn chunks(s: &str) -> Vec<Chunk> {
    let mut out: Vec<Chunk> = vec![];
    for c in s.chars() {
        let digit = c.is_ascii_digit();
        let extended = match out.last_mut() {
            Some(Chunk::Num(n)) if digit => {
                n.push(c);
                true
            }
            Some(Chunk::Text(t)) if !digit => {
                t.extend(c.to_lowercase());
                true
            }
            _ => false,
        };
        if !extended {
            out.push(if digit {
                Chunk::Num(c.to_string())
            } else {
                Chunk::Text(c.to_lowercase().collect())
            });
        }
    }
    out
}

/// `YYYY-MM-DD` (UTC) of a Unix time in seconds.
pub fn utc_date(unix_s: u64) -> String {
    // Civil-from-days (H. Hinnant's public-domain algorithm).
    let z = (unix_s / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use eak_core::{Point, Rotation, SymbolInstance};

    /// R10, R2 (10k), R1 (1k) and C1 on the first sheet, no PCB.
    pub(crate) fn project() -> Project {
        let mut p = Project::new("BOM config");
        let sheet = p.first_sheet_id().cloned().unwrap_or_default();
        for (i, (refdes, comp, value)) in [
            ("R10", "R_0603", "10k"),
            ("R2", "R_0603", "10k"),
            ("R1", "R_0603", "1k"),
            ("C1", "C_0603", "100n"),
        ]
        .into_iter()
        .enumerate()
        {
            let id = format!("sym_{i}");
            p.schematic.symbols.insert(
                id.clone(),
                SymbolInstance {
                    id,
                    sheet_id: sheet.clone(),
                    component_id: comp.into(),
                    refdes: refdes.into(),
                    value: value.into(),
                    position: Point::mm(20.0 + 15.0 * i as f64, 40.0),
                    rotation: Rotation::R0,
                    mirror: false,
                },
            );
        }
        for comp in ["R_0603", "C_0603"] {
            let def = eak_core::builtin::get(comp).expect("built-in part");
            p.library.insert(comp.into(), def);
        }
        p
    }

    #[test]
    fn configuration_round_trips_through_json() {
        let mut cfg = BomConfig::default();
        cfg.columns.push(BomColumn {
            field: BomField::Issues,
            header: Some("Problems".into()),
            sort: SortDir::Desc,
        });
        cfg.grouping = Grouping::Designator;
        cfg.exclude.push(FilterRule {
            field: BomField::Comment,
            op: FilterOp::Contains,
            value: "dnp".into(),
        });
        let back = BomConfig::from_json(&cfg.to_json()).unwrap();
        assert_eq!(back, cfg);
        // A file without the optional fields still loads with the defaults.
        let minimal = BomConfig::from_json(r#"{"columns":[{"field":"mpn"}]}"#).unwrap();
        assert_eq!(minimal.version, CONFIG_VERSION);
        assert_eq!(minimal.grouping, Grouping::Part);
        assert!(BomConfig::from_json(r#"{"columns":[]}"#).is_err());
        assert!(BomConfig::from_json(r#"{"columns":[{"field":"mpn"},{"field":"mpn"}]}"#).is_err());
        assert!(BomConfig::from_json("not json").is_err());
    }

    #[test]
    fn columns_sort_group_and_filter_apply_in_order() {
        let p = project();
        let t = table(&p, &[], &BomConfig::default()).unwrap();
        assert_eq!(t.headers[0], "Comment");
        assert_eq!(t.headers.len(), 7);
        // Grouped by part, sorted by designator (natural order).
        let designators: Vec<&str> = t.rows.iter().map(|r| r[1].as_str()).collect();
        assert_eq!(designators, ["C1", "R1", "R2,R10"]);
        assert_eq!(t.parts, 4);

        let cfg = BomConfig {
            version: CONFIG_VERSION,
            columns: vec![
                BomColumn {
                    field: BomField::Designator,
                    header: Some("Ref".into()),
                    sort: SortDir::Desc,
                },
                BomColumn::new(BomField::Quantity),
            ],
            grouping: Grouping::Designator,
            exclude: vec![FilterRule {
                field: BomField::Designator,
                op: FilterOp::Equals,
                value: "c1".into(),
            }],
        };
        let t = table(&p, &[], &cfg).unwrap();
        assert_eq!(t.headers, ["Ref", "Quantity"]);
        let refs: Vec<&str> = t.rows.iter().map(|r| r[0].as_str()).collect();
        assert_eq!(refs, ["R10", "R2", "R1"]);
        assert!(t.rows.iter().all(|r| r[1] == "1"));
        assert_eq!(t.removed, 1);
        let text = csv(&t);
        assert!(
            text.starts_with("\"Ref\",\"Quantity\"\n\"R10\",\"1\"\n"),
            "{text}"
        );
    }

    #[test]
    fn natural_order_and_dates() {
        assert_eq!(natural_cmp("R2", "R10"), Ordering::Less);
        assert_eq!(natural_cmp("0.0123", "0.05"), Ordering::Less);
        assert_eq!(natural_cmp("c1", "C1"), Ordering::Equal);
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(1_700_000_000), "2023-11-14");
    }
}
