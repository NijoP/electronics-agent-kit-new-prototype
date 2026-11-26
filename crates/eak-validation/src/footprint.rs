//! Footprint verification (FP-001..FP-010, ADR-0052).
//!
//! Checks one component definition's footprint against its symbol, its pin map, an optional
//! datasheet land-pattern table ([`LandPatternSpec`]) and the board's fabrication limits.
//! The result is a [`FootprintReport`]: one explicit state per rule, the rules this build does
//! not implement, and the limitations of the checks that ran. A rule that lacks its input is
//! `UNKNOWN`, never `PASS`; a spec value that no person has checked needs human approval.
//!
//! `FootprintVerified` maturity is earned only through [`footprint_maturity_problems`]: every
//! rule FP-001..FP-009 certifies **and** a named reviewer confirmed it. Nothing here mutates a
//! design or a library.

use crate::state::{CheckState, RuleResult};
use eak_core::datasheet::{DatasheetSource, DatasheetValue};
use eak_core::geom::{mm, to_mm};
use eak_core::libmeta::{maturity_problems, LibraryMaturity, MaturityEvidence};
use eak_core::{ComponentDef, FootprintDef, Nm, PadDef, PadKind, PadShape, PinType, Point};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Default geometric tolerance against the datasheet table: ±0.05 mm.
pub const DEFAULT_TOLERANCE: Nm = 50_000;

/// Rules evaluated (FP-010 is listed in `not_implemented` instead).
pub const RULES: [&str; 9] = [
    "FP-001", "FP-002", "FP-003", "FP-004", "FP-005", "FP-006", "FP-007", "FP-008", "FP-009",
];

/// One pad of a datasheet's recommended land pattern. Every number is a cited datasheet value
/// in millimetres (a value in another unit is not used and the rule reports `UNKNOWN`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PadSpec {
    pub number: String,
    pub x: DatasheetValue,
    pub y: DatasheetValue,
    pub size_x: DatasheetValue,
    pub size_y: DatasheetValue,
    #[serde(default)]
    pub shape: Option<PadShape>,
    /// Finished hole of a through-hole pad.
    #[serde(default)]
    pub drill: Option<DatasheetValue>,
    /// Lead diameter (or diagonal) of a through-hole pin.
    #[serde(default)]
    pub lead: Option<DatasheetValue>,
}

/// Typed table from a datasheet's recommended land pattern. Library data, not EAK-IR; the
/// library layer stores it (`eak_library::LandPatternSpec` re-exports it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandPatternSpec {
    /// Document, revision and page the table comes from.
    pub source: DatasheetSource,
    /// Who entered the table (a person, or a tool id for an automatic draft).
    pub entered_by: String,
    pub pads: Vec<PadSpec>,
    #[serde(default)]
    pub pitch: Option<DatasheetValue>,
    /// Body length (x) and width (y).
    #[serde(default)]
    pub body: Option<(DatasheetValue, DatasheetValue)>,
    #[serde(default)]
    pub height_max: Option<DatasheetValue>,
    /// Pin-1 pad centre in footprint coordinates.
    #[serde(default)]
    pub pin1: Option<Point>,
}

/// Fabrication limits the footprint must meet (from the board's design rules and fabricator).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FabLimits {
    pub min_clearance: Nm,
    pub min_mask_web: Nm,
    /// Board-wide solder-mask expansion used for pads without their own.
    pub mask_expansion: Nm,
    pub min_annular_ring: Nm,
    /// Drill over lead diameter a through-hole pin needs.
    pub drill_allowance: Nm,
    /// IPC-7351 courtyard excess of the declared density level; `None` = not declared.
    #[serde(default)]
    pub courtyard_excess: Option<Nm>,
}

/// Result of verifying one footprint. A new report type (ADR-0052): it is about a library
/// definition, not a project revision, so it is not a [`crate::Report`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootprintReport {
    pub check: String,
    pub definition_id: String,
    pub footprint: String,
    /// The land-pattern table the geometry was compared with, if any.
    pub spec_source: Option<DatasheetSource>,
    pub tolerance_nm: Nm,
    pub rule_results: Vec<RuleResult>,
    pub overall: CheckState,
    pub not_implemented: Vec<String>,
    pub limitations: Vec<String>,
}

impl FootprintReport {
    /// State of one rule; a rule that is not in the report is `UNKNOWN`.
    pub fn state_of(&self, rule_id: &str) -> CheckState {
        self.rule_results
            .iter()
            .find(|r| r.rule_id == rule_id)
            .map(|r| r.state)
            .unwrap_or(CheckState::Unknown)
    }

    /// Rules FP-001..FP-009 that are missing or do not certify (fail, unknown, approval).
    pub fn blocking(&self) -> Vec<String> {
        RULES
            .iter()
            .filter(|id| !self.state_of(id).certifies())
            .map(|id| match self.rule_results.iter().find(|r| r.rule_id == *id) {
                Some(r) => format!("{id} is {:?}: {}", r.state, r.reason),
                None => format!("{id} is missing from the report"),
            })
            .collect()
    }

    /// True only when every rule FP-001..FP-009 is present and certifies.
    pub fn verifies(&self) -> bool {
        self.blocking().is_empty()
    }
}

fn result(rule: &str, state: CheckState, reason: impl Into<String>) -> RuleResult {
    RuleResult {
        rule_id: rule.into(),
        state,
        reason: reason.into(),
        severity_override: None,
        default_state: None,
    }
}

fn pass(rule: &str) -> RuleResult {
    result(rule, CheckState::Pass, "")
}

fn fail(rule: &str, problems: &[String]) -> RuleResult {
    result(rule, CheckState::Fail, problems.join("; "))
}

fn unknown(rule: &str, why: &str) -> RuleResult {
    result(rule, CheckState::Unknown, why)
}

fn unreviewed(rule: &str, why: &str) -> RuleResult {
    result(rule, CheckState::RequiresHumanApproval, why)
}

/// A datasheet value in millimetres, as nanometres.
fn nm_of(v: &DatasheetValue) -> Option<Nm> {
    (v.unit.trim() == "mm" && v.value.is_finite()).then(|| mm(v.value))
}

fn mechanical(number: &str) -> bool {
    number.trim().is_empty() || number.starts_with("MP")
}

/// Axis-aligned copper extent of a pad (min, max).
fn extent(p: &PadDef) -> (Point, Point) {
    let (hx, hy) = (p.size_x / 2, p.size_y / 2);
    (
        Point::new(p.position.x - hx, p.position.y - hy),
        Point::new(p.position.x + hx, p.position.y + hy),
    )
}

/// Edge-to-edge gap of two pad extents (0 when they touch or overlap).
fn gap(a: &PadDef, b: &PadDef) -> Nm {
    let ((a0, a1), (b0, b1)) = (extent(a), extent(b));
    let dx = (b0.x - a1.x).max(a0.x - b1.x).max(0);
    let dy = (b0.y - a1.y).max(a0.y - b1.y).max(0);
    if dx == 0 || dy == 0 {
        dx.max(dy)
    } else {
        (dx as f64).hypot(dy as f64) as Nm
    }
}

/// FP-001: every pin maps to existing pads, every pad is mapped or mechanical, no duplicates.
fn fp001(def: &ComponentDef) -> RuleResult {
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    for p in &def.footprint.pads {
        if !mechanical(&p.number) && !seen.insert(p.number.as_str()) {
            problems.push(format!("pad number {} is used twice", p.number));
        }
    }
    let mut mapped = BTreeSet::new();
    for pin in &def.symbol.pins {
        let pads = def.pin_map.get(&pin.number).cloned().unwrap_or_default();
        if pads.is_empty() && pin.electrical_type != PinType::NoConnect {
            problems.push(format!("pin {} maps to no pad", pin.number));
        }
        for pad in pads {
            if !seen.contains(pad.as_str()) {
                problems.push(format!("pin {} maps to missing pad {pad}", pin.number));
            }
            mapped.insert(pad);
        }
    }
    for p in &def.footprint.pads {
        if !mechanical(&p.number) && !mapped.contains(&p.number) {
            problems.push(format!("pad {} is not mapped to a pin", p.number));
        }
    }
    if problems.is_empty() {
        pass("FP-001")
    } else {
        fail("FP-001", &problems)
    }
}

/// FP-002: each pad's centre and size within tolerance of the spec.
fn fp002(fp: &FootprintDef, spec: Option<&LandPatternSpec>, tol: Nm) -> RuleResult {
    let Some(spec) = spec else {
        return unknown("FP-002", "no land-pattern table");
    };
    let mut problems = Vec::new();
    for s in &spec.pads {
        if !fp.pads.iter().any(|p| p.number == s.number) {
            problems.push(format!("spec pad {} is missing from the footprint", s.number));
        }
    }
    let mut verified = true;
    for p in fp.pads.iter().filter(|p| !mechanical(&p.number)) {
        let Some(s) = spec.pads.iter().find(|s| s.number == p.number) else {
            problems.push(format!("pad {} is not in the land-pattern table", p.number));
            continue;
        };
        let vals = [&s.x, &s.y, &s.size_x, &s.size_y];
        verified &= vals.iter().all(|v| v.is_human_verified());
        let have = [p.position.x, p.position.y, p.size_x, p.size_y];
        for (i, what) in ["x", "y", "size x", "size y"].iter().enumerate() {
            let Some(want) = nm_of(vals[i]) else {
                return unknown("FP-002", "land-pattern values are not all in mm");
            };
            if (have[i] - want).abs() > tol {
                problems.push(format!(
                    "pad {} {what} is {} mm, the datasheet says {} mm",
                    p.number,
                    to_mm(have[i]),
                    to_mm(want)
                ));
            }
        }
        if s.shape.is_some_and(|shape| shape != p.shape) {
            problems.push(format!("pad {} shape differs from the datasheet", p.number));
        }
    }
    if !problems.is_empty() {
        fail("FP-002", &problems)
    } else if !verified {
        unreviewed("FP-002", "land-pattern values are not checked by a person")
    } else {
        pass("FP-002")
    }
}

/// Distances between neighbouring pads that share a row (same y) or, failing that, a column.
fn row_pitches(fp: &FootprintDef, tol: Nm) -> Vec<(String, String, Nm)> {
    let pads: Vec<&PadDef> = fp.pads.iter().filter(|p| !mechanical(&p.number)).collect();
    let mut out = Vec::new();
    for by_y in [true, false] {
        let across = |q: &PadDef| if by_y { q.position.y } else { q.position.x };
        let along = |q: &PadDef| if by_y { q.position.x } else { q.position.y };
        let mut lines: Vec<Vec<&PadDef>> = Vec::new();
        for &p in &pads {
            match lines.iter_mut().find(|l| (across(l[0]) - across(p)).abs() <= tol) {
                Some(l) => l.push(p),
                None => lines.push(vec![p]),
            }
        }
        for l in &mut lines {
            l.sort_by_key(|q| if by_y { q.position.x } else { q.position.y });
            for w in l.windows(2) {
                let d = along(w[1]) - along(w[0]);
                out.push((w[0].number.clone(), w[1].number.clone(), d));
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

/// `d` is a whole number (1 or more) of pitches `p`, within `tol`.
fn multiple_of(d: Nm, p: Nm, tol: Nm) -> bool {
    let k = (d as f64 / p as f64).round() as Nm;
    k >= 1 && (d - k * p).abs() <= tol
}

/// FP-003: row pitch equals the spec pitch. Neighbours in a row with a gap (the top row of a
/// SOT-23-5) may be a whole number of pitches apart.
fn fp003(fp: &FootprintDef, spec: Option<&LandPatternSpec>, tol: Nm) -> RuleResult {
    let Some(pitch) = spec.and_then(|s| s.pitch.as_ref()) else {
        return unknown("FP-003", "no datasheet pitch");
    };
    let Some(want) = nm_of(pitch).filter(|w| *w > 0) else {
        return unknown("FP-003", "datasheet pitch is not a positive length in mm");
    };
    let pitches = row_pitches(fp, tol);
    if pitches.is_empty() {
        return unknown("FP-003", "no row of two or more pads");
    }
    let problems: Vec<String> = pitches
        .iter()
        .filter(|(_, _, d)| !multiple_of(*d, want, tol))
        .map(|(a, b, d)| {
            format!(
                "pads {a}-{b} are {} mm apart, not a multiple of the datasheet pitch {} mm",
                to_mm(*d),
                to_mm(want)
            )
        })
        .collect();
    if !problems.is_empty() {
        fail("FP-003", &problems)
    } else if !pitch.is_human_verified() {
        unreviewed("FP-003", "datasheet pitch is not checked by a person")
    } else {
        pass("FP-003")
    }
}

/// FP-004: pad-to-pad copper clearance and solder-mask web.
fn fp004(fp: &FootprintDef, limits: Option<&FabLimits>) -> RuleResult {
    let Some(l) = limits else {
        return unknown("FP-004", "no fabrication limits given");
    };
    let mut problems = Vec::new();
    for (i, a) in fp.pads.iter().enumerate() {
        for b in &fp.pads[i + 1..] {
            let g = gap(a, b);
            if g < l.min_clearance {
                problems.push(format!(
                    "pads {}-{} copper gap {} mm is below {} mm",
                    a.number,
                    b.number,
                    to_mm(g),
                    to_mm(l.min_clearance)
                ));
                continue;
            }
            let mask =
                a.mask_expansion_or(l.mask_expansion) + b.mask_expansion_or(l.mask_expansion);
            if g - mask < l.min_mask_web {
                problems.push(format!(
                    "pads {}-{} solder-mask web {} mm is below {} mm",
                    a.number,
                    b.number,
                    to_mm(g - mask),
                    to_mm(l.min_mask_web)
                ));
            }
        }
    }
    if problems.is_empty() {
        pass("FP-004")
    } else {
        fail("FP-004", &problems)
    }
}

/// FP-005: mask and paste expansion of each pad.
fn fp005(fp: &FootprintDef) -> RuleResult {
    let problems: Vec<String> = fp
        .pads
        .iter()
        .flat_map(PadDef::expansion_problems)
        .collect();
    if problems.is_empty() {
        pass("FP-005")
    } else {
        fail("FP-005", &problems)
    }
}

/// FP-006: through-hole drill against lead size and annular ring.
fn fp006(
    fp: &FootprintDef,
    spec: Option<&LandPatternSpec>,
    limits: Option<&FabLimits>,
) -> RuleResult {
    let tht: Vec<&PadDef> = fp.pads.iter().filter(|p| p.kind == PadKind::Tht).collect();
    if tht.is_empty() {
        return result("FP-006", CheckState::NotApplicable, "no through-hole pads");
    }
    let Some(l) = limits else {
        return unknown("FP-006", "no fabrication limits given");
    };
    let mut problems = Vec::new();
    let mut missing = Vec::new();
    for p in tht {
        let Some(drill) = p.drill else {
            problems.push(format!("through-hole pad {} has no drill", p.number));
            continue;
        };
        let ring = (p.size_x.min(p.size_y) - drill) / 2;
        if ring < l.min_annular_ring {
            problems.push(format!(
                "pad {} annular ring {} mm is below {} mm",
                p.number,
                to_mm(ring),
                to_mm(l.min_annular_ring)
            ));
        }
        let lead = spec
            .and_then(|s| s.pads.iter().find(|s| s.number == p.number))
            .and_then(|s| s.lead.as_ref())
            .and_then(nm_of);
        match lead {
            Some(lead) if drill < lead + l.drill_allowance => problems.push(format!(
                "pad {} drill {} mm is below lead {} mm plus allowance",
                p.number,
                to_mm(drill),
                to_mm(lead)
            )),
            Some(_) => {}
            None => missing.push(p.number.clone()),
        }
    }
    if !problems.is_empty() {
        fail("FP-006", &problems)
    } else if !missing.is_empty() {
        let why = format!("no datasheet lead size for pads {}", missing.join(", "));
        unknown("FP-006", &why)
    } else {
        pass("FP-006")
    }
}

/// FP-007: the courtyard contains all pads, and the body plus the declared courtyard excess.
fn fp007(
    fp: &FootprintDef,
    spec: Option<&LandPatternSpec>,
    limits: Option<&FabLimits>,
) -> RuleResult {
    let (cw, ch) = (fp.courtyard_half_w, fp.courtyard_half_h);
    let mut problems = Vec::new();
    for p in &fp.pads {
        let (lo, hi) = extent(p);
        if lo.x < -cw || hi.x > cw || lo.y < -ch || hi.y > ch {
            problems.push(format!("pad {} extends outside the courtyard", p.number));
        }
    }
    if !problems.is_empty() {
        return fail("FP-007", &problems);
    }
    let Some((bl, bw)) = spec.and_then(|s| s.body.as_ref()) else {
        return unknown("FP-007", "no datasheet body size");
    };
    let (Some(len), Some(wid)) = (nm_of(bl), nm_of(bw)) else {
        return unknown("FP-007", "datasheet body size is not in mm");
    };
    let Some(excess) = limits.and_then(|l| l.courtyard_excess) else {
        return unknown("FP-007", "no IPC-7351 density level declared");
    };
    if len / 2 + excess > cw || wid / 2 + excess > ch {
        let why = "courtyard does not contain the body plus the courtyard excess".to_string();
        fail("FP-007", &[why])
    } else if !(bl.is_human_verified() && bw.is_human_verified()) {
        unreviewed("FP-007", "datasheet body size is not checked by a person")
    } else {
        pass("FP-007")
    }
}

/// FP-008: pad 1 exists where the spec puts pin 1.
fn fp008(fp: &FootprintDef, spec: Option<&LandPatternSpec>, tol: Nm) -> RuleResult {
    let Some(at) = spec.and_then(|s| s.pin1) else {
        return unknown("FP-008", "no datasheet pin-1 location");
    };
    let Some(p1) = fp.pads.iter().find(|p| p.number == "1") else {
        return fail("FP-008", &["the footprint has no pad 1".to_string()]);
    };
    if (p1.position.x - at.x).abs() > tol || (p1.position.y - at.y).abs() > tol {
        let why = "pad 1 is not at the datasheet pin-1 location".to_string();
        fail("FP-008", &[why])
    } else {
        pass("FP-008")
    }
}

/// FP-009: body height within the datasheet maximum.
fn fp009(fp: &FootprintDef, spec: Option<&LandPatternSpec>) -> RuleResult {
    let max = spec.and_then(|s| s.height_max.as_ref());
    let (Some(h), Some(max)) = (fp.body_height, max) else {
        return unknown("FP-009", "body height or datasheet maximum height missing");
    };
    let Some(max_nm) = nm_of(max) else {
        return unknown("FP-009", "datasheet height is not in mm");
    };
    if h > max_nm {
        let why = format!(
            "body height {} mm exceeds the datasheet maximum {} mm",
            to_mm(h),
            to_mm(max_nm)
        );
        fail("FP-009", &[why])
    } else if !max.is_human_verified() {
        unreviewed("FP-009", "datasheet height is not checked by a person")
    } else {
        pass("FP-009")
    }
}

/// Run FP-001..FP-009 on one definition. FP-010 is reported as not implemented.
pub fn verify(
    def: &ComponentDef,
    spec: Option<&LandPatternSpec>,
    limits: Option<&FabLimits>,
    tolerance: Nm,
) -> FootprintReport {
    let fp = &def.footprint;
    let rule_results = vec![
        fp001(def),
        fp002(fp, spec, tolerance),
        fp003(fp, spec, tolerance),
        fp004(fp, limits),
        fp005(fp),
        fp006(fp, spec, limits),
        fp007(fp, spec, limits),
        fp008(fp, spec, tolerance),
        fp009(fp, spec),
    ];
    let overall = rule_results
        .iter()
        .fold(CheckState::NotApplicable, |acc, r| acc.worst(r.state));
    FootprintReport {
        check: "footprint".into(),
        definition_id: def.id.clone(),
        footprint: fp.name.clone(),
        spec_source: spec.map(|s| s.source.clone()),
        tolerance_nm: tolerance,
        rule_results,
        overall,
        not_implemented: vec!["FP-010 3D model bounding box against the footprint body".into()],
        limitations: vec![
            "pad clearance uses bounding boxes: round and oval pads count as rectangles".into(),
            "pads numbered empty or MP* are treated as mechanical and need no pin".into(),
            "pitch is checked only between neighbouring pads of one row".into(),
            "rotated pads and custom pad shapes are not modelled".into(),
        ],
    }
}

/// `maturity_problems` for a footprint claim: a claim of `FootprintVerified` or higher is
/// refused unless a report exists in which FP-001..FP-009 all certify (no fail, no unknown,
/// no pending approval) and the evidence names a source and a reviewer.
pub fn footprint_maturity_problems(
    claimed: LibraryMaturity,
    evidence: &[MaturityEvidence],
    report: Option<&FootprintReport>,
) -> Vec<String> {
    let mut out = maturity_problems(claimed, evidence);
    if claimed < LibraryMaturity::FootprintVerified {
        return out;
    }
    match report {
        None => out.push("FootprintVerified needs a footprint verification report".into()),
        Some(r) => out.extend(r.blocking()),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::datasheet::{Extraction, LimitClass, PartScope};

    fn ldo() -> ComponentDef {
        eak_core::builtin::components()
            .into_iter()
            .find(|c| c.id == "LDO_SOT-23-5")
            .expect("builtin SOT-23-5")
    }

    fn source() -> DatasheetSource {
        DatasheetSource {
            manufacturer: "Example".into(),
            document_sha256: "0".repeat(64),
            document_revision: Some("Rev. A".into()),
            page: Some(12),
            section: Some("Recommended land pattern".into()),
            table: None,
        }
    }

    fn val(parameter: &str, value: f64) -> DatasheetValue {
        DatasheetValue {
            parameter: parameter.into(),
            class: LimitClass::Characteristic,
            statistic: None,
            value,
            unit: "mm".into(),
            conditions: String::new(),
            scope: PartScope::default(),
            source: source(),
            extraction: Extraction::HumanVerified {
                reviewer: "A. Reviewer".into(),
                at_unix_s: 1_790_000_000,
            },
        }
    }

    /// The land pattern the builtin SOT-23-5 was drawn from.
    fn spec() -> LandPatternSpec {
        let pads = [
            ("1", -0.95, -1.1),
            ("2", 0.0, -1.1),
            ("3", 0.95, -1.1),
            ("4", 0.95, 1.1),
            ("5", -0.95, 1.1),
        ];
        LandPatternSpec {
            source: source(),
            entered_by: "A. Reviewer".into(),
            pads: pads
                .iter()
                .map(|(n, x, y)| PadSpec {
                    number: (*n).into(),
                    x: val("x", *x),
                    y: val("y", *y),
                    size_x: val("pad width", 0.6),
                    size_y: val("pad length", 1.1),
                    shape: Some(PadShape::Rect),
                    drill: None,
                    lead: None,
                })
                .collect(),
            pitch: Some(val("pitch", 0.95)),
            body: Some((val("body length", 2.9), val("body width", 1.6))),
            height_max: Some(val("height max", 1.45)),
            pin1: Some(Point::mm(-0.95, -1.1)),
        }
    }

    fn limits() -> FabLimits {
        FabLimits {
            min_clearance: mm(0.15),
            min_mask_web: mm(0.1),
            mask_expansion: mm(0.05),
            min_annular_ring: mm(0.15),
            drill_allowance: mm(0.2),
            courtyard_excess: Some(mm(0.25)),
        }
    }

    fn evidence() -> Vec<MaturityEvidence> {
        vec![MaturityEvidence {
            level: LibraryMaturity::FootprintVerified,
            source: "Example datasheet Rev. A p12".into(),
            by: "A. Reviewer".into(),
        }]
    }

    #[test]
    fn correct_spec_passes_every_rule_and_lists_fp010() {
        let r = verify(&ldo(), Some(&spec()), Some(&limits()), DEFAULT_TOLERANCE);
        assert!(r.verifies(), "{:?}", r.blocking());
        assert_eq!(r.state_of("FP-006"), CheckState::NotApplicable);
        assert!(r.not_implemented.iter().any(|n| n.starts_with("FP-010")));
        assert!(!r.limitations.is_empty());
        let claim = LibraryMaturity::FootprintVerified;
        assert!(footprint_maturity_problems(claim, &evidence(), Some(&r)).is_empty());
    }

    #[test]
    fn broken_spec_fails_geometry_pitch_and_pin1() {
        let mut s = spec();
        s.pitch = Some(val("pitch", 1.27));
        s.pads[0].x = val("x", -1.27);
        s.pin1 = Some(Point::mm(0.95, 1.1));
        s.height_max = Some(val("height max", 1.0));
        let r = verify(&ldo(), Some(&s), Some(&limits()), DEFAULT_TOLERANCE);
        for rule in ["FP-002", "FP-003", "FP-008", "FP-009"] {
            assert_eq!(r.state_of(rule), CheckState::Fail, "{rule}");
        }
        assert_eq!(r.overall, CheckState::Fail);
        assert!(!r.verifies());
        let claim = LibraryMaturity::FootprintVerified;
        assert!(!footprint_maturity_problems(claim, &evidence(), Some(&r)).is_empty());
    }

    #[test]
    fn no_spec_is_unknown_never_pass() {
        let r = verify(&ldo(), None, Some(&limits()), DEFAULT_TOLERANCE);
        for rule in ["FP-002", "FP-003", "FP-008"] {
            assert_eq!(r.state_of(rule), CheckState::Unknown, "{rule}");
        }
        assert_eq!(r.state_of("FP-001"), CheckState::Pass);
        assert_eq!(r.state_of("FP-004"), CheckState::Pass);
        assert!(!r.verifies());
        let claim = LibraryMaturity::FootprintVerified;
        let problems = footprint_maturity_problems(claim, &evidence(), Some(&r));
        assert!(problems.iter().any(|p| p.starts_with("FP-002")));
    }

    #[test]
    fn footprint_verified_needs_report_and_named_reviewer() {
        let r = verify(&ldo(), Some(&spec()), Some(&limits()), DEFAULT_TOLERANCE);
        let claim = LibraryMaturity::FootprintVerified;
        assert!(!footprint_maturity_problems(claim, &evidence(), None).is_empty());
        let mut anon = evidence();
        anon[0].by = " ".into();
        assert!(!footprint_maturity_problems(claim, &anon, Some(&r)).is_empty());
        let lower = LibraryMaturity::StructurallyValidated;
        assert!(footprint_maturity_problems(lower, &[], None).is_empty());
    }

    #[test]
    fn unreviewed_spec_needs_approval() {
        let mut s = spec();
        s.pitch.as_mut().unwrap().extraction = Extraction::ExtractedAuto;
        let r = verify(&ldo(), Some(&s), Some(&limits()), DEFAULT_TOLERANCE);
        assert_eq!(r.state_of("FP-003"), CheckState::RequiresHumanApproval);
        assert!(!r.verifies());
    }

    #[test]
    fn unmapped_pin_and_tight_clearance_fail() {
        let mut def = ldo();
        def.pin_map.remove("1");
        let mut tight = limits();
        tight.min_clearance = mm(0.5);
        let r = verify(&def, None, Some(&tight), DEFAULT_TOLERANCE);
        assert_eq!(r.state_of("FP-001"), CheckState::Fail);
        assert_eq!(r.state_of("FP-004"), CheckState::Fail);
        let r = verify(&def, None, None, DEFAULT_TOLERANCE);
        assert_eq!(r.state_of("FP-004"), CheckState::Unknown);
    }
}
