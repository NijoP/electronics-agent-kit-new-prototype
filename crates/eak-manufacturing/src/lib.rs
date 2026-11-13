//! # eak-manufacturing — manufacturing compiler
//!
//! Generates Gerber (RS-274X with X2 file attributes), Excellon drill, BOM and CPL from
//! EAK-IR, binds them to the exact project revision via a SHA-256 manifest, and
//! re-validates the written files. A package is only marked a release candidate when the
//! ERC, DRC and mechanical reports supplied were produced for the same revision and input
//! hash and contain no errors. Even then it is "internally verified", never
//! "manufacturing-ready": independent engineering review and fabricator DFM are required.

pub mod drill;
pub mod gerber;
pub mod sourcing;

use eak_core::{geom::to_mm, CopperLayer, Project, Side};
use eak_validation::{report::project_hash, Outcome, Report};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageStatus {
    /// Generated without passing gates: NOT for fabrication.
    Draft,
    /// All supplied revision-bound checks passed. Still requires independent engineering
    /// review and fabricator DFM before ordering.
    InternallyVerifiedReleaseCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub check: String,
    pub revision: u64,
    pub outcome: Outcome,
    pub errors: usize,
    pub warnings: usize,
    pub bound_to_this_revision: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub project_id: String,
    pub project_name: String,
    pub revision: u64,
    pub input_sha256: String,
    pub generator: String,
    pub generated_ms: u128,
    pub status: PackageStatus,
    pub gates: Vec<GateResult>,
    pub blocking_reasons: Vec<String>,
    pub files: Vec<FileEntry>,
    pub statement: String,
    pub not_verified: Vec<String>,
}

fn sha(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

pub fn bom_csv(p: &Project) -> String {
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for s in p.schematic.symbols.values() {
        groups
            .entry((s.component_id.clone(), s.value.clone()))
            .or_default()
            .push(s.refdes.clone());
    }
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out =
        String::from("Comment,Designator,Footprint,LCSC Part #,Manufacturer,MPN,Quantity\n");
    for ((comp, value), mut refs) in groups {
        refs.sort_by_key(|a| natural(a));
        let def = &p.library[&comp];
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            q(&value),
            q(&refs.join(",")),
            q(&def.footprint.name),
            q(def.lcsc.as_deref().unwrap_or("")),
            q(def.manufacturer.as_deref().unwrap_or("")),
            q(def.mpn.as_deref().unwrap_or("")),
            refs.len()
        ));
    }
    out
}

fn natural(s: &str) -> (String, u64) {
    let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    (
        s.chars().filter(|c| !c.is_ascii_digit()).collect(),
        d.parse().unwrap_or(0),
    )
}

pub fn cpl_csv(p: &Project) -> String {
    let mut rows: Vec<_> = p.pcb.footprints.values().collect();
    rows.sort_by_key(|a| natural(&a.refdes));
    let mut out = String::from("Designator,Mid X,Mid Y,Layer,Rotation\n");
    for f in rows {
        out.push_str(&format!(
            "{},{:.4}mm,{:.4}mm,{},{}\n",
            f.refdes,
            to_mm(f.position.x),
            to_mm(f.position.y),
            if f.side == Side::Top { "Top" } else { "Bottom" },
            f.rotation.degrees()
        ));
    }
    out
}

/// Generate and validate a manufacturing package in `out_dir`.
pub fn compile(p: &Project, reports: &[&Report], out_dir: &Path) -> Result<Manifest, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let input = project_hash(p);
    let mut blocking = vec![];
    if p.pcb.board.outline.len() < 3 {
        return Err("cannot compile manufacturing outputs without a board outline".into());
    }
    let mut gates = vec![];
    for need in ["ERC", "DRC", "MECH"] {
        match reports.iter().find(|r| r.check == need) {
            None => blocking.push(format!("{need} report not supplied")),
            Some(r) => {
                let bound = r.revision == p.revision && r.input_sha256 == input;
                if !bound {
                    blocking.push(format!(
                        "{need} report is for revision {} (project is at {}); re-run it",
                        r.revision, p.revision
                    ));
                }
                if r.outcome == Outcome::Errors {
                    blocking.push(format!("{need} reports {} error(s)", r.errors));
                }
                gates.push(GateResult {
                    check: need.into(),
                    revision: r.revision,
                    outcome: r.outcome,
                    errors: r.errors,
                    warnings: r.warnings,
                    bound_to_this_revision: bound,
                });
            }
        }
    }
    let stem = p
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let files: Vec<(String, String, String)> = vec![
        (
            format!("{stem}-F_Cu.gtl"),
            "gerber".into(),
            gerber::copper_layer(p, CopperLayer::Top),
        ),
        (
            format!("{stem}-B_Cu.gbl"),
            "gerber".into(),
            gerber::copper_layer(p, CopperLayer::Bottom),
        ),
        (
            format!("{stem}-F_Mask.gts"),
            "gerber".into(),
            gerber::soldermask(p, CopperLayer::Top),
        ),
        (
            format!("{stem}-B_Mask.gbs"),
            "gerber".into(),
            gerber::soldermask(p, CopperLayer::Bottom),
        ),
        (
            format!("{stem}-F_Paste.gtp"),
            "gerber".into(),
            gerber::paste(p, CopperLayer::Top),
        ),
        (
            format!("{stem}-B_Paste.gbp"),
            "gerber".into(),
            gerber::paste(p, CopperLayer::Bottom),
        ),
        (
            format!("{stem}-F_Silkscreen.gto"),
            "gerber".into(),
            gerber::silkscreen(p, Side::Top),
        ),
        (
            format!("{stem}-B_Silkscreen.gbo"),
            "gerber".into(),
            gerber::silkscreen(p, Side::Bottom),
        ),
        (
            format!("{stem}-Edge_Cuts.gm1"),
            "gerber".into(),
            gerber::outline(p),
        ),
        (
            format!("{stem}-PTH.drl"),
            "drill".into(),
            drill::excellon(p),
        ),
        (format!("{stem}-BOM.csv"), "bom".into(), bom_csv(p)),
        (format!("{stem}-CPL.csv"), "cpl".into(), cpl_csv(p)),
    ];
    let mut entries = vec![];
    for (name, kind, content) in &files {
        let problems = match kind.as_str() {
            "gerber" => gerber::validate(content).problems,
            "drill" => drill::validate(content).problems,
            _ => vec![],
        };
        if !problems.is_empty() {
            return Err(format!(
                "generated {name} failed self-validation: {}",
                problems.join("; ")
            ));
        }
        let path: PathBuf = out_dir.join(name);
        eak_runtime::store::atomic_write(&path, content.as_bytes()).map_err(|e| e.to_string())?;
        // Re-read from disk so the hash covers exactly what was written.
        let written = std::fs::read(&path).map_err(|e| e.to_string())?;
        entries.push(FileEntry {
            name: name.clone(),
            bytes: written.len() as u64,
            sha256: sha(&written),
            kind: kind.clone(),
        });
    }
    let status = if blocking.is_empty() {
        PackageStatus::InternallyVerifiedReleaseCandidate
    } else {
        PackageStatus::Draft
    };
    let manifest = Manifest {
        project_id: p.id.clone(),
        project_name: p.name.clone(),
        revision: p.revision,
        input_sha256: input,
        generator: format!("eak-manufacturing {}", env!("CARGO_PKG_VERSION")),
        generated_ms: eak_runtime::store::now_ms(),
        status,
        gates,
        blocking_reasons: blocking,
        files: entries,
        statement: match status {
            PackageStatus::Draft => "DRAFT — blocking issues listed; NOT for fabrication.".into(),
            PackageStatus::InternallyVerifiedReleaseCandidate => "Internal automated checks passed for this exact revision. This is NOT a manufacturing-ready claim: independent electrical engineering review and fabricator DFM review are still required.".into(),
        },
        not_verified: vec![
            "Independent senior electronics engineering review".into(),
            "Fabricator DFM review of these exact files".into(),
            "Physical prototype / bench testing".into(),
            "Checks listed as not implemented in the ERC/DRC/MECH reports".into(),
        ],
    };
    let mjson = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    eak_runtime::store::atomic_write(&out_dir.join("manifest.json"), mjson.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(manifest)
}

/// Re-verify a package on disk against its manifest (hash + syntax).
pub fn verify_package(dir: &Path) -> Result<Vec<String>, String> {
    let m: Manifest = serde_json::from_str(
        &std::fs::read_to_string(dir.join("manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut problems = vec![];
    for f in &m.files {
        match std::fs::read(dir.join(&f.name)) {
            Err(e) => problems.push(format!("{}: {e}", f.name)),
            Ok(data) => {
                if sha(&data) != f.sha256 {
                    problems.push(format!(
                        "{}: hash mismatch (file modified after release)",
                        f.name
                    ));
                }
                let text = String::from_utf8_lossy(&data);
                let syn = match f.kind.as_str() {
                    "gerber" => gerber::validate(&text).problems,
                    "drill" => drill::validate(&text).problems,
                    _ => vec![],
                };
                problems.extend(syn.into_iter().map(|s| format!("{}: {s}", f.name)));
            }
        }
    }
    Ok(problems)
}
