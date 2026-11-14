//! # eak-manufacturing — manufacturing compiler
//!
//! Generates Gerber (RS-274X with X2 file attributes), Excellon drill, BOM and CPL from
//! EAK-IR, binds them to the exact project revision via a SHA-256 manifest, and
//! re-validates the written files. A package is only marked a release candidate when the
//! ERC, DRC and mechanical reports supplied were produced for the same revision and input
//! hash and contain no errors. Even then it is "internally verified", never
//! "manufacturing-ready": independent engineering review and fabricator DFM are required.

pub mod bom;
pub mod drill;
pub mod fabpack;
pub mod gerber;
pub mod jlcpcb;
pub mod lcsc;
pub mod sourcing;
pub mod stackup;
pub mod zip;

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

/// BOM grouped by value, footprint and MPN: one line per distinct part to buy and place.
/// LCSC and manufacturer come from the first component definition of each group.
pub fn bom_csv(p: &Project) -> String {
    type Row = (Vec<String>, String, String);
    let mut groups: BTreeMap<(String, String, String), Row> = BTreeMap::new();
    for s in p.schematic.symbols.values() {
        let def = p.library.get(&s.component_id);
        let footprint = def.map(|d| d.footprint.name.clone()).unwrap_or_default();
        let mpn = def.and_then(|d| d.mpn.clone()).unwrap_or_default();
        let row = groups
            .entry((s.value.clone(), footprint, mpn))
            .or_insert_with(|| {
                (
                    vec![],
                    def.and_then(|d| d.lcsc.clone()).unwrap_or_default(),
                    def.and_then(|d| d.manufacturer.clone()).unwrap_or_default(),
                )
            });
        row.0.push(s.refdes.clone());
    }
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out =
        String::from("Comment,Designator,Footprint,LCSC Part #,Manufacturer,MPN,Quantity\n");
    for ((value, footprint, mpn), (mut refs, lcsc, manufacturer)) in groups {
        refs.sort_by_key(|a| natural(a));
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            q(&value),
            q(&refs.join(",")),
            q(&footprint),
            q(&lcsc),
            q(&manufacturer),
            q(&mpn),
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
    for need in ["ERC", "DRC", "MECH", "DFM"] {
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
    // Unknown mechanical fit is not a pass: without a height limit MECH-001 checks nothing, and
    // parts without body data (MECH-003) cannot be checked at all.
    if p.constraints.max_component_height.is_none() {
        blocking.push(
            "no component height limit declared: MECH-001 was not evaluated (mechanical fit unknown)"
                .into(),
        );
    }
    if let Some(m) = reports.iter().find(|r| r.check == "MECH") {
        let unknown = m
            .findings
            .iter()
            .filter(|f| f.rule_id == "MECH-003")
            .count();
        if unknown > 0 {
            blocking.push(format!(
                "{unknown} part(s) have no 3D body data (MECH-003): mechanical fit unknown"
            ));
        }
    }
    // Fail-closed engineering gates, evaluated independently on this revision: every required
    // gate must be PASS (FAIL and UNKNOWN both block). Fabricator capability is covered by the
    // supplied, revision-bound DFM report above, so that gate is skipped here.
    let gate_report = eak_validation::gates::evaluate(p, None);
    for g in &gate_report.gates {
        if g.required
            && g.id != "fabricator_capability"
            && g.state != eak_validation::GateState::Pass
        {
            blocking.push(format!(
                "gate {} is {:?}: {}",
                g.id,
                g.state,
                g.reasons.first().cloned().unwrap_or_default()
            ));
        }
    }
    let stem = p
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();
    // One copper Gerber per layer in the stackup (top, inner layers, bottom).
    let st = &p.pcb.board.stackup;
    let mut files: Vec<(String, String, String)> = st
        .layer_ids()
        .into_iter()
        .map(|layer| {
            let (tag, ext) = match layer {
                CopperLayer::Top => ("F_Cu".to_string(), "gtl".to_string()),
                CopperLayer::Inner(n) => (format!("In{n}_Cu"), format!("g{}", n + 1)),
                CopperLayer::Bottom => ("B_Cu".to_string(), "gbl".to_string()),
            };
            (
                format!("{stem}-{tag}.{ext}"),
                "gerber".to_string(),
                gerber::copper_layer(p, layer),
            )
        })
        .collect();
    files.extend(vec![
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
        (
            format!("{stem}-stackup.json"),
            "stackup".into(),
            stackup::stackup_json(p),
        ),
    ]);
    // Blind and buried vias are drilled in their own files, one per layer span.
    for span in drill::span_holes(p).into_keys() {
        files.push((
            format!(
                "{stem}-{}-{}.drl",
                span.from.name().replace('.', "_"),
                span.to.name().replace('.', "_")
            ),
            "drill".into(),
            drill::excellon_span(p, span),
        ));
    }
    let mut entries = vec![];
    for (name, kind, content) in &files {
        let problems = match kind.as_str() {
            "gerber" => gerber::validate(content).problems,
            "drill" => drill::validate(content).problems,
            "stackup" => stackup::validate(content),
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
    // Fabrication pack: Gerber + drill files and a README of layers in one ZIP.
    let fab: Vec<&(String, String, String)> = files
        .iter()
        .filter(|(_, kind, _)| kind == "gerber" || kind == "drill")
        .collect();
    let readme = pack_readme(p, &input, &blocking, &fab);
    let mut zip_entries: Vec<(String, Vec<u8>)> = fab
        .iter()
        .map(|(name, _, content)| (name.clone(), content.as_bytes().to_vec()))
        .collect();
    zip_entries.push(("README.txt".into(), readme.as_bytes().to_vec()));
    let pack = fabpack::zip(&zip_entries);
    fabpack::list(&pack).map_err(|e| format!("generated fab pack failed self-validation: {e}"))?;
    let readme_name = format!("{stem}-fab-README.txt");
    eak_runtime::store::atomic_write(&out_dir.join(&readme_name), readme.as_bytes())
        .map_err(|e| e.to_string())?;
    let pack_name = format!("{stem}-fab-pack.zip");
    eak_runtime::store::atomic_write(&out_dir.join(&pack_name), &pack)
        .map_err(|e| e.to_string())?;
    for (name, kind) in [(readme_name, "readme"), (pack_name, "fab-pack")] {
        let written = std::fs::read(out_dir.join(&name)).map_err(|e| e.to_string())?;
        entries.push(FileEntry {
            name,
            bytes: written.len() as u64,
            sha256: sha(&written),
            kind: kind.into(),
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
            "Checks listed as not implemented in the ERC/DRC/MECH/DFM reports".into(),
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
                    "fab-pack" => fabpack::list(&data).err().into_iter().collect(),
                    "gerber" => gerber::validate(&text).problems,
                    "drill" => drill::validate(&text).problems,
                    "stackup" => stackup::validate(&text),
                    _ => vec![],
                };
                problems.extend(syn.into_iter().map(|s| format!("{}: {s}", f.name)));
            }
        }
    }
    Ok(problems)
}

/// README shipped inside the fab pack: what each file is, the stackup and the package status.
fn pack_readme(
    p: &Project,
    input: &str,
    blocking: &[String],
    files: &[&(String, String, String)],
) -> String {
    let st = &p.pcb.board.stackup;
    let mut r = format!(
        "Fabrication files for {} (revision {})\nGenerated by eak-manufacturing {} from EAK-IR sha256 {input}\n\n",
        p.name,
        p.revision,
        env!("CARGO_PKG_VERSION")
    );
    r.push_str(if blocking.is_empty() {
        "Status: internal automated checks passed for this revision. NOT a manufacturing-ready claim: independent engineering review and the fabricator's DFM review are still required.\n\n"
    } else {
        "Status: DRAFT - NOT for fabrication. Blocking issues:\n"
    });
    for b in blocking {
        r.push_str(&format!("  - {b}\n"));
    }
    if !blocking.is_empty() {
        r.push('\n');
    }
    r.push_str("Files (Gerber RS-274X, Excellon drill; units mm):\n");
    for (name, _, _) in files {
        r.push_str(&format!("  {name:<40} {}\n", fabpack::describe(name)));
    }
    r.push_str(&format!(
        "\nStackup: {} copper layers, finished thickness {:.2} mm, {}\n",
        st.layers.len(),
        to_mm(st.board_thickness),
        st.dielectric
    ));
    for l in &st.layers {
        r.push_str(&format!("  {:<8} {} oz\n", l.layer.name(), l.copper_oz));
    }
    r.push_str("\nBOM and pick-and-place (CPL) CSVs are delivered next to this archive.\n");
    r
}
