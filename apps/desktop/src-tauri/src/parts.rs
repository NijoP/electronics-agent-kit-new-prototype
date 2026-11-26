//! Parts search, library import, BOM view and Fabricator preparation for the desktop shell.
//!
//! Plain functions over [`Session`] (tested headless in `tests/parts_ipc.rs`); the Tauri
//! wrappers in [`cmd`] only decode arguments. Rules:
//! * the only network traffic is the Distributor part number or keyword the user typed, sent to the
//!   documented, signed Distributor OpenAPI with the user's own credentials (ADR-0032); no design data
//!   is ever sent anywhere;
//! * an import is a typed `UpsertComponentDefinition` command through the engine as the
//!   Human actor, with the revision the UI last rendered;
//! * supplier and assembly records are written into `<project>/sourcing` only through
//!   `Engine::write_project_record` (ADR-0039): journalled, never a direct file write;
//! * Fabricator output is written only to a folder the user names, is labelled "prepared for
//!   review", and nothing is uploaded or ordered.

use crate::Session;
use eak_core::builtin;
use eak_manufacturing::assembly::{self, AssemblyClass, AssemblyRecord, AssemblyReport};
use eak_manufacturing::bom::{self, BomLine};
use eak_manufacturing::distributor::{self, UreqFetch};
use eak_manufacturing::distributor_api::DistributorApi;
use eak_manufacturing::fabricator::{self, DfmReport, PrepResult};
use eak_manufacturing::provider::{Answer, ErrorKind, Resilient, Served, SupplierProvider};
use eak_manufacturing::sourcing::{self, SourcingRecord};
use eak_runtime::{Actor, Command, CommandRequest, CommandResponse, Engine};
use eak_validation::{drc, erc, footprint, pin_pad, CheckState, Report};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A part already in the EAK library that matches the query.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryHit {
    pub id: String,
    pub name: String,
    pub category: String,
    pub distributor: Option<String>,
    pub mpn: Option<String>,
}

/// What happened when Distributor was asked.
#[derive(Debug, Clone, Serialize)]
pub struct LiveSearch {
    /// `ok`, `stale` (earlier cached answer, supplier down), `needs_credentials`,
    /// `unauthorized`, `rate_limited`, `blocked`, `not_found`, `unreachable`, `bad_query` or
    /// `error`.
    pub status: &'static str,
    /// Human-readable explanation, shown as-is. Empty when `ok`.
    pub message: String,
    /// How the records were obtained (live, fresh cache, stale cache); `None` on failure.
    pub served: Option<Served>,
    pub records: Vec<SourcingRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PartsSearch {
    pub query: String,
    pub library: Vec<LibraryHit>,
    /// Records already stored with the open project (`<project>/sourcing`).
    pub stored: Vec<SourcingRecord>,
    pub live: LiveSearch,
}

pub(crate) fn sourcing_dir(s: &Session) -> Option<PathBuf> {
    s.with_engine(|e| e.dir().map(|d| d.join("sourcing")))
        .ok()
        .flatten()
}

pub(crate) fn stored_records(s: &Session) -> Vec<SourcingRecord> {
    sourcing_dir(s)
        .filter(|d| d.is_dir())
        .and_then(|d| sourcing::read_records(&d).ok())
        .unwrap_or_default()
}

/// Built-in parts matching every term; none when the built-in library is disabled (ADR-0035).
fn library_hits(query: &str) -> Vec<LibraryHit> {
    let off = crate::libcatalog::disabled(crate::libstore::store().root());
    if off.contains(&crate::libcatalog::scope_id(
        &eak_manufacturing::library_link::Scope::Builtin,
    )) {
        return vec![];
    }
    let terms: Vec<String> = query
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    if terms.is_empty() {
        return vec![];
    }
    builtin::components()
        .into_iter()
        .filter(|c| {
            let hay = format!(
                "{} {} {} {} {}",
                c.id,
                c.name,
                c.category,
                c.mpn.as_deref().unwrap_or(""),
                c.distributor.as_deref().unwrap_or("")
            )
            .to_ascii_lowercase();
            terms.iter().all(|t| hay.contains(t))
        })
        .take(25)
        .map(|c| LibraryHit {
            id: c.id,
            name: c.name,
            category: c.category,
            distributor: c.distributor,
            mpn: c.mpn,
        })
        .collect()
}

pub fn live_status(k: ErrorKind) -> &'static str {
    match k {
        ErrorKind::BadQuery => "bad_query",
        ErrorKind::Offline => "unreachable",
        ErrorKind::Blocked => "blocked",
        ErrorKind::NotFound => "not_found",
        ErrorKind::Unusable => "error",
        ErrorKind::NeedsCredentials => "needs_credentials",
        ErrorKind::Unauthorized => "unauthorized",
        ErrorKind::RateLimited => "rate_limited",
    }
}

/// Supplier lookups shared by every `parts_*` command: one cache, rate limit and retry policy
/// per process (ADR-0028). The wrapper never holds a lock while waiting on the network, so a
/// slow or dead supplier cannot freeze other commands.
pub type DistributorResilient = Resilient<DistributorApi<UreqFetch>>;

pub fn live_provider() -> &'static DistributorResilient {
    static P: OnceLock<DistributorResilient> = OnceLock::new();
    P.get_or_init(|| Resilient::new(crate::catalogue::lookup_api()))
}

/// True when `base` names a reserved `.invalid` host (RFC 6761), which never resolves: this
/// build has no distributor configured, so no request may be sent to it (finding S20).
pub fn reserved_host(base: &str) -> bool {
    let rest = base.split_once("://").map_or(base, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = host.split(':').next().unwrap_or("").trim_end_matches('.');
    let host = host.to_ascii_lowercase();
    host.is_empty() || host == "invalid" || host.ends_with(".invalid")
}

fn not_configured(base: &str) -> String {
    format!(
        "Distributor is not configured in this build ({base} is a reserved name that never \
         resolves); no request was sent"
    )
}

/// Refuse a live lookup when the distributor `host` is reserved (see [`reserved_host`]).
pub fn require_configured(host: &str) -> Result<(), String> {
    if reserved_host(host) {
        return Err(not_configured(host));
    }
    Ok(())
}

/// Search the built-in library and the project's stored records offline, then ask Distributor
/// at `host`, unless `host` is reserved: then the live part is `not_configured` and nothing is
/// sent. A failure of the live search is reported in `live`, never replaced by invented rows.
pub fn parts_search<P: SupplierProvider>(
    s: &Session,
    provider: &Resilient<P>,
    host: &str,
    query: &str,
) -> Result<PartsSearch, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("enter an Distributor part number (C25804) or a keyword".into());
    }
    let stored = sourcing::search(&stored_records(s), q)
        .into_iter()
        .cloned()
        .collect();
    let live = if reserved_host(host) {
        LiveSearch {
            status: "not_configured",
            message: not_configured(host),
            served: None,
            records: vec![],
        }
    } else {
        live_search(provider, q)
    };
    Ok(PartsSearch {
        query: q.to_string(),
        library: library_hits(q),
        stored,
        live,
    })
}

fn live_search<P: SupplierProvider>(provider: &Resilient<P>, q: &str) -> LiveSearch {
    match provider.search(q) {
        Ok(Answer {
            served: served @ Served::StaleCache { age_s, because },
            records,
            ..
        }) => LiveSearch {
            status: "stale",
            message: format!(
                "Distributor is not answering ({because:?}); showing an earlier answer from {age_s} s ago. \
                 Stock and prices may have changed."
            ),
            served: Some(served),
            records,
        },
        Ok(Answer {
            served, records, ..
        }) => LiveSearch {
            status: if records.is_empty() {
                "not_found"
            } else {
                "ok"
            },
            message: if records.is_empty() {
                "Distributor returned no matching parts".into()
            } else {
                String::new()
            },
            served: Some(served),
            records,
        },
        Err(e) => LiveSearch {
            status: live_status(e.kind),
            message: e.message,
            served: None,
            records: vec![],
        },
    }
}

/// A supplier listing kept with the project without any engineering definition.
#[derive(Debug, Clone, Serialize)]
pub struct SavedRecord {
    pub record: SourcingRecord,
    pub path: String,
    /// What the listing is relative to the libraries (existing part, supplier-only, needs authoring).
    pub link: eak_manufacturing::library_link::Link,
}

/// Supplier discovery, separate from engineering import: fetch one Distributor listing and store it in
/// `<project>/sourcing` so the MPN and Distributor number are kept and the engineering definition can
/// be completed later. The engine writes and journals the record (ADR-0039); no design command
/// runs, the project revision does not change, and no symbol, footprint or rating is created.
pub fn parts_save_record<P: SupplierProvider>(
    s: &Session,
    provider: &Resilient<P>,
    host: &str,
    distributor_part: &str,
) -> Result<SavedRecord, String> {
    require_configured(host)?;
    let answer = provider.lookup(distributor_part).map_err(|e| e.message)?;
    let record = answer
        .records
        .into_iter()
        .next()
        .ok_or("Distributor returned no record for that part number")?;
    let defs = crate::libstore::all_defs(
        crate::libstore::store(),
        &crate::libstore::project_library(s),
    );
    let refs: Vec<_> = defs.iter().map(|(sc, d)| (sc.clone(), d)).collect();
    let link = eak_manufacturing::library_link::link_record(&record, &refs);
    let (file, bytes) = sourcing::record_file(&record).map_err(|e| e.to_string())?;
    let path = s.with_engine(|e| {
        let dir = e
            .dir()
            .ok_or("saving a supplier record needs a saved project folder")?
            .join("sourcing")
            .join(&file);
        write_record(e, &file, &bytes, "supplier listing saved").map(|()| dir)
    })??;
    Ok(SavedRecord {
        record,
        path: path.display().to_string(),
        link,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub component_id: String,
    pub response: CommandResponse,
    pub notes: Vec<String>,
    pub record: SourcingRecord,
}

/// Re-fetch `distributor` from Distributor, store the record with the project and add the part to the
/// project library through `Command::UpsertComponentDefinition`.
pub fn parts_import<P: SupplierProvider>(
    s: &Session,
    provider: &Resilient<P>,
    host: &str,
    expected_revision: u64,
    distributor_part: &str,
) -> Result<ImportResult, String> {
    require_configured(host)?;
    let answer = provider.lookup(distributor_part).map_err(|e| e.message)?;
    let stale = match answer.served {
        Served::StaleCache { age_s, .. } => Some(age_s),
        _ => None,
    };
    let record = answer
        .records
        .into_iter()
        .next()
        .ok_or("Distributor returned no record for that part number")?;
    let mut plan = distributor::import_plan(&record)?;
    if let Some(age_s) = stale {
        // The record keeps its original retrieval time and hash; say so in the notes.
        plan.notes.push(format!(
            "Distributor was unreachable: this record is an earlier answer from {age_s} s ago, not a fresh fetch"
        ));
    }
    let component_id = plan.definition.id.clone();
    let (file, bytes) = sourcing::record_file(&record).map_err(|e| e.to_string())?;
    let response = s.with_engine(|e| {
        e.dir()
            .ok_or("importing a part needs a saved project folder")?;
        let response = e.execute(CommandRequest {
            expected_revision,
            actor: Actor::human(),
            command: Command::UpsertComponentDefinition {
                definition: Box::new(plan.definition),
            },
            approval: None,
        });
        // The record is evidence for the imported definition: keep it only if the engine
        // accepted the command.
        if response.accepted {
            write_record(e, &file, &bytes, "supplier record for an imported part")?;
        }
        Ok::<_, String>(response)
    })??;
    Ok(ImportResult {
        component_id,
        response,
        notes: plan.notes,
        record,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ManufacturingPreview {
    pub revision: u64,
    pub bom: Vec<BomLine>,
    pub dfm: DfmReport,
    /// Fabricator assembly classification rows and fail-closed BOM warnings (ADR-0034).
    pub assembly: AssemblyReport,
    pub default_out_dir: Option<String>,
}

/// The BOM (joined with stored Distributor records) and the Fabricator DFM rule check for the
/// current revision. Writes nothing.
pub fn manufacturing_preview(s: &Session) -> Result<ManufacturingPreview, String> {
    let records = stored_records(s);
    s.with_engine(|e| {
        let p = e.project();
        let lines = bom::build(p, &records);
        let dfm = fabricator::dfm(p, &lines, &fabricator::Profile::two_layer_1oz());
        let stored = e
            .dir()
            .map(|d| assembly::read_records(&d.join("sourcing")))
            .unwrap_or_default();
        let assembly = assembly::warnings(&lines, &records, &stored, now_s());
        ManufacturingPreview {
            revision: p.revision,
            bom: lines,
            dfm,
            assembly,
            default_out_dir: e
                .dir()
                .map(|d| {
                    d.join("output")
                        .join(format!("fabricator-rev-{}", p.revision))
                })
                .map(|d| d.display().to_string()),
        }
    })
}

/// Part standardisation (Phase 1 D1, gap #38): the `SOURCING` check of `eak-validation` over
/// the parts placed in the schematic and the supplier records stored with the project. Read-only
/// and offline: nothing is fetched and nothing is written. The report lists what it does not
/// check (`not_implemented`, `limitations`); no errors never means the parts are orderable.
pub fn part_standardisation(s: &Session) -> Result<Report, String> {
    let parts: Vec<sourcing::SupplierPart> = stored_records(s)
        .iter()
        .filter_map(SourcingRecord::supplier_part)
        .collect();
    let (_, p) = s.shared_project()?;
    Ok(eak_validation::sourcing::run(&p, &parts))
}

fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Record the user's own statement of a part's Fabricator assembly class (basic, extended, preferred
/// extended or not assemblable), with the time. `Unknown` is not an attestation and is refused.
pub fn assembly_attest(
    s: &Session,
    distributor: &str,
    class: AssemblyClass,
) -> Result<AssemblyRecord, String> {
    if class == AssemblyClass::Unknown {
        return Err("choose basic, extended, preferred extended or not assemblable".into());
    }
    let rec = AssemblyRecord {
        distributor: distributor.trim().to_ascii_uppercase(),
        class,
        source: "user_attested".into(),
        verified_at_unix_s: now_s(),
        stock: None,
        unit_price: None,
    };
    let (file, bytes) = assembly::record_file(&rec)?;
    s.with_engine(|e| {
        e.dir()
            .ok_or("recording assembly data needs a saved project folder")?;
        write_record(e, &file, &bytes, "assembly class attested by the user")
    })??;
    Ok(rec)
}

/// The one way this module writes into a project folder: `Engine::write_project_record`
/// (ADR-0039), as the Human actor, journalled in `sourcing/journal.jsonl`.
fn write_record(e: &mut Engine, file: &str, bytes: &[u8], summary: &str) -> Result<(), String> {
    e.write_project_record(&Actor::human(), "sourcing", file, bytes, summary)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct AssemblyImport {
    pub file: String,
    /// Parts of this project's BOM found in the file and recorded.
    pub recorded: usize,
    pub unknown_class: usize,
}

/// Read a Fabricator parts-library CSV the user obtained (absolute path) and record the rows for the
/// Distributor numbers on this project's BOM. Rows for other parts are ignored; unrecognised type text
/// stays `Unknown`.
pub fn assembly_import_parts_file(s: &Session, path: &str) -> Result<AssemblyImport, String> {
    let p = Path::new(path.trim());
    if !p.is_absolute() {
        return Err("choose an absolute file path".into());
    }
    let len = std::fs::metadata(p)
        .map_err(|e| format!("{}: {e}", p.display()))?
        .len();
    if len > 256 * 1024 * 1024 {
        return Err("the file is larger than 256 MiB".into());
    }
    let text =
        String::from_utf8_lossy(&std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?)
            .into_owned();
    let records = stored_records(s);
    let wanted: std::collections::BTreeSet<String> = s.with_engine(|e| {
        bom::build(e.project(), &records)
            .into_iter()
            .filter_map(|l| l.distributor)
            .map(|c| c.to_ascii_uppercase())
            .collect()
    })?;
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let rows = assembly::parse_parts_csv(&text, &name, now_s(), Some(&wanted))?;
    let files = rows
        .iter()
        .map(assembly::record_file)
        .collect::<Result<Vec<_>, _>>()?;
    let summary = format!("assembly class from parts file {name}");
    s.with_engine(|e| {
        e.dir()
            .ok_or("recording assembly data needs a saved project folder")?;
        files
            .iter()
            .try_for_each(|(file, bytes)| write_record(e, file, bytes, &summary))
    })??;
    Ok(AssemblyImport {
        file: name,
        recorded: rows.len(),
        unknown_class: rows
            .iter()
            .filter(|r| r.class == AssemblyClass::Unknown)
            .count(),
    })
}

/// Refuse relative paths and folders that already hold other files, so a bundle never
/// lands in, or overwrites, a folder the user did not mean.
fn check_out_dir(dir: &Path) -> Result<(), String> {
    if !dir.is_absolute() {
        return Err("choose an absolute folder path".into());
    }
    if dir.exists() {
        if !dir.is_dir() {
            return Err(format!("{} is not a folder", dir.display()));
        }
        let mut entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        if entries.next().is_some() && !dir.join("fabricator-prep-manifest.json").is_file() {
            return Err(format!(
                "{} is not empty and is not an earlier EAK Fabricator bundle; choose an empty folder",
                dir.display()
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct PrepareResult {
    pub prep: PrepResult,
    pub reports: Vec<Report>,
}

/// Run ERC, DRC and MECH on the current revision (saving evidence), then write the
/// Fabricator preparation bundle into `out_dir` (the default folder when empty).
pub fn fabricator_prepare(s: &Session, out_dir: &str) -> Result<PrepareResult, String> {
    let records = stored_records(s);
    s.with_engine(|e| {
        let out_dir = crate::clean_path_input(out_dir);
        let target = if out_dir.is_empty() {
            let d = e
                .dir()
                .ok_or("preparing a bundle needs a saved project folder")?;
            d.join("output")
                .join(format!("fabricator-rev-{}", e.project().revision))
        } else {
            PathBuf::from(out_dir)
        };
        check_out_dir(&target)?;
        let reports = vec![
            erc::run(e.project()),
            drc::run(e.project()),
            eak_validation::mech::run(e.project()),
        ];
        for r in &reports {
            let json = serde_json::to_string_pretty(r).map_err(|err| err.to_string())?;
            e.save_evidence(&r.check, &json)
                .ok_or_else(|| format!("{} evidence could not be saved", r.check))?;
        }
        let refs: Vec<&Report> = reports.iter().collect();
        let stored = e
            .dir()
            .map(|d| assembly::read_records(&d.join("sourcing")))
            .unwrap_or_default();
        let prep = fabricator::prepare_with(e.project(), &refs, &records, &stored, &target)?;
        Ok(PrepareResult { prep, reports })
    })?
}

// ---------------------------------------------------------------------------------------
// Library verification and component authoring (slice C)
// ---------------------------------------------------------------------------------------

/// A definition found in one of the library scopes, with the scope it came from.
#[derive(Debug, Clone, Serialize)]
pub struct ScopedDef {
    pub scope: String,
    pub def: eak_core::ComponentDef,
}

/// The definition `id` as placement sees it: project first, then personal, then imported,
/// then built-in. A disabled library is not searched.
pub fn find_def(
    store: &crate::libstore::LibraryStore,
    project: &BTreeMap<String, eak_core::ComponentDef>,
    id: &str,
) -> Result<ScopedDef, String> {
    use eak_manufacturing::library_link::Scope;
    let rank = |sc: &Scope| match sc {
        Scope::Project => 0,
        Scope::Personal => 1,
        Scope::Imported(_) => 2,
        Scope::Builtin => 3,
    };
    crate::libstore::all_defs(store, project)
        .into_iter()
        .filter(|(_, d)| d.id == id)
        .min_by_key(|(sc, _)| rank(sc))
        .map(|(sc, def)| ScopedDef {
            scope: sc.label(),
            def,
        })
        .ok_or_else(|| format!("no component {id:?} in any enabled library"))
}

/// Footprint verification (FP-001..FP-009) of one library part. Library parts carry no stored
/// land-pattern table or fabrication limits yet, so the rules that need them come back
/// `UNKNOWN`: the report never reads as verified without that evidence.
pub fn footprint_verify_in(
    store: &crate::libstore::LibraryStore,
    project: &BTreeMap<String, eak_core::ComponentDef>,
    id: &str,
) -> Result<footprint::FootprintReport, String> {
    let found = find_def(store, project, id)?;
    Ok(footprint::verify(
        &found.def,
        None,
        None,
        footprint::DEFAULT_TOLERANCE,
    ))
}

/// Pin-to-pad compatibility (PP-001..PP-005) of one library part.
pub fn pin_pad_verify_in(
    store: &crate::libstore::LibraryStore,
    project: &BTreeMap<String, eak_core::ComponentDef>,
    id: &str,
) -> Result<pin_pad::PinPadReport, String> {
    Ok(pin_pad::verify(&find_def(store, project, id)?.def))
}

/// What saving a created or edited component did.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentSaved {
    pub id: String,
    /// `personal`: created and edited parts live in the user's personal library.
    pub scope: String,
    /// True when a personal definition with this id was replaced.
    pub replaced: bool,
    pub pin_pad: pin_pad::PinPadReport,
    pub footprint: footprint::FootprintReport,
}

/// Create or edit a component in the user's personal library through the existing store
/// (`LibraryStore::personal_save`, which refuses inconsistent definitions). A pin map that
/// fails PP-001 (a pin on a missing pad, a pad joining two pins) is refused; the other results
/// are returned for the editor to show. Built-in parts never change: an edited built-in part is
/// saved as a personal copy with the same id.
pub fn component_save_in(
    store: &crate::libstore::LibraryStore,
    def: eak_core::ComponentDef,
) -> Result<ComponentSaved, String> {
    let pp = pin_pad::verify(&def);
    if let Some(r) = pp
        .rule_results
        .iter()
        .find(|r| r.rule_id == "PP-001" && r.state == CheckState::Fail)
    {
        return Err(format!(
            "the pin map is wrong and was not saved: {}",
            r.reason
        ));
    }
    let replaced = store.personal().0.iter().any(|d| d.id == def.id);
    let fp = footprint::verify(&def, None, None, footprint::DEFAULT_TOLERANCE);
    let id = def.id.clone();
    store.personal_save(def)?;
    Ok(ComponentSaved {
        id,
        scope: "personal".into(),
        replaced,
        pin_pad: pp,
        footprint: fp,
    })
}

pub fn footprint_verify(s: &Session, id: &str) -> Result<footprint::FootprintReport, String> {
    let project = crate::libstore::project_library(s);
    footprint_verify_in(crate::libstore::store(), &project, id)
}

pub fn pin_pad_verify(s: &Session, id: &str) -> Result<pin_pad::PinPadReport, String> {
    let project = crate::libstore::project_library(s);
    pin_pad_verify_in(crate::libstore::store(), &project, id)
}

pub fn component_save(def: eak_core::ComponentDef) -> Result<ComponentSaved, String> {
    component_save_in(crate::libstore::store(), def)
}

/// Tauri command wrappers: argument decoding only, no logic. Network and file work runs on the
/// blocking pool (`crate::cmd::blocking`): supplier search is P4 (bounded background), the
/// import is an edit (P1, never queued), fab previews/preparation are P3 (bounded).
pub mod cmd {
    use crate::cmd::blocking;
    use crate::resources::Priority;
    use eak_manufacturing::distributor_api::PRODUCTION_BASE;
    use tauri::AppHandle;

    #[tauri::command(rename_all = "snake_case")]
    pub async fn parts_search(app: AppHandle, query: String) -> Result<super::PartsSearch, String> {
        blocking(app, Priority::P4, move |s| {
            super::parts_search(s, super::live_provider(), PRODUCTION_BASE, &query)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn parts_import(
        app: AppHandle,
        expected_revision: u64,
        distributor: String,
    ) -> Result<super::ImportResult, String> {
        blocking(app, Priority::P1, move |s| {
            super::parts_import(
                s,
                super::live_provider(),
                PRODUCTION_BASE,
                expected_revision,
                &distributor,
            )
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn parts_save_record(
        app: AppHandle,
        distributor: String,
    ) -> Result<super::SavedRecord, String> {
        blocking(app, Priority::P1, move |s| {
            super::parts_save_record(s, super::live_provider(), PRODUCTION_BASE, &distributor)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn manufacturing_preview(
        app: AppHandle,
    ) -> Result<super::ManufacturingPreview, String> {
        blocking(app, Priority::P3, super::manufacturing_preview).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn part_standardisation(app: AppHandle) -> Result<super::Report, String> {
        blocking(app, Priority::P2, super::part_standardisation).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn assembly_attest(
        app: AppHandle,
        distributor: String,
        class: super::AssemblyClass,
    ) -> Result<super::AssemblyRecord, String> {
        blocking(app, Priority::P1, move |s| {
            super::assembly_attest(s, &distributor, class)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn assembly_import_parts_file(
        app: AppHandle,
        path: String,
    ) -> Result<super::AssemblyImport, String> {
        blocking(app, Priority::P3, move |s| {
            super::assembly_import_parts_file(s, &path)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn fabricator_prepare(
        app: AppHandle,
        out_dir: String,
    ) -> Result<super::PrepareResult, String> {
        blocking(app, Priority::P3, move |s| {
            super::fabricator_prepare(s, &out_dir)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn footprint_verify(
        app: AppHandle,
        id: String,
    ) -> Result<eak_validation::footprint::FootprintReport, String> {
        blocking(app, Priority::P2, move |s| super::footprint_verify(s, &id)).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn pin_pad_verify(
        app: AppHandle,
        id: String,
    ) -> Result<eak_validation::pin_pad::PinPadReport, String> {
        blocking(app, Priority::P2, move |s| super::pin_pad_verify(s, &id)).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn component_save(
        app: AppHandle,
        def: eak_core::ComponentDef,
    ) -> Result<super::ComponentSaved, String> {
        blocking(app, Priority::P1, move |_| super::component_save(def)).await
    }
}
