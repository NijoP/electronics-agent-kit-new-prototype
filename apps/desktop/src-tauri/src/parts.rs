//! Parts search, library import, BOM view and JLCPCB preparation for the desktop shell.
//!
//! Plain functions over [`Session`] (tested headless in `tests/parts_ipc.rs`); the Tauri
//! wrappers in [`cmd`] only decode arguments. Rules:
//! * the only network traffic is the LCSC part number or keyword the user typed, sent to the
//!   documented, signed LCSC OpenAPI with the user's own credentials (ADR-0032); no design data
//!   is ever sent anywhere;
//! * an import is a typed `UpsertComponentDefinition` command through the engine as the
//!   Human actor, with the revision the UI last rendered;
//! * JLCPCB output is written only to a folder the user names, is labelled "prepared for
//!   review", and nothing is uploaded or ordered.

use crate::Session;
use eak_core::builtin;
use eak_manufacturing::bom::{self, BomLine};
use eak_manufacturing::jlcpcb::{self, DfmReport, PrepResult};
use eak_manufacturing::lcsc::{self, UreqFetch};
use eak_manufacturing::lcsc_api::LcscApi;
use eak_manufacturing::provider::{Answer, ErrorKind, Resilient, Served, SupplierProvider};
use eak_manufacturing::sourcing::{self, SourcingRecord};
use eak_runtime::{Actor, Command, CommandRequest, CommandResponse};
use eak_validation::{drc, erc, Report};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A part already in the EAK library that matches the query.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryHit {
    pub id: String,
    pub name: String,
    pub category: String,
    pub lcsc: Option<String>,
    pub mpn: Option<String>,
}

/// What happened when LCSC was asked.
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

fn sourcing_dir(s: &Session) -> Option<PathBuf> {
    s.with_engine(|e| e.dir().map(|d| d.join("sourcing")))
        .ok()
        .flatten()
}

fn stored_records(s: &Session) -> Vec<SourcingRecord> {
    sourcing_dir(s)
        .filter(|d| d.is_dir())
        .and_then(|d| sourcing::read_records(&d).ok())
        .unwrap_or_default()
}

fn library_hits(query: &str) -> Vec<LibraryHit> {
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
                c.lcsc.as_deref().unwrap_or("")
            )
            .to_ascii_lowercase();
            terms.iter().all(|t| hay.contains(t))
        })
        .take(25)
        .map(|c| LibraryHit {
            id: c.id,
            name: c.name,
            category: c.category,
            lcsc: c.lcsc,
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
pub type LcscResilient = Resilient<LcscApi<UreqFetch>>;

pub fn live_provider() -> &'static LcscResilient {
    static P: OnceLock<LcscResilient> = OnceLock::new();
    P.get_or_init(|| Resilient::new(crate::catalogue::lookup_api()))
}

/// Search the built-in library and the project's stored records offline, then ask LCSC.
/// A failure of the live search is reported in `live`, never replaced by invented rows.
pub fn parts_search<P: SupplierProvider>(
    s: &Session,
    provider: &Resilient<P>,
    query: &str,
) -> Result<PartsSearch, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("enter an LCSC part number (C25804) or a keyword".into());
    }
    let stored = sourcing::search(&stored_records(s), q)
        .into_iter()
        .cloned()
        .collect();
    let live = match provider.search(q) {
        Ok(Answer {
            served: served @ Served::StaleCache { age_s, because },
            records,
            ..
        }) => LiveSearch {
            status: "stale",
            message: format!(
                "LCSC is not answering ({because:?}); showing an earlier answer from {age_s} s ago. \
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
                "LCSC returned no matching parts".into()
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
    };
    Ok(PartsSearch {
        query: q.to_string(),
        library: library_hits(q),
        stored,
        live,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub component_id: String,
    pub response: CommandResponse,
    pub notes: Vec<String>,
    pub record: SourcingRecord,
}

/// Re-fetch `lcsc` from LCSC, store the record with the project and add the part to the
/// project library through `Command::UpsertComponentDefinition`.
pub fn parts_import<P: SupplierProvider>(
    s: &Session,
    provider: &Resilient<P>,
    expected_revision: u64,
    lcsc_part: &str,
) -> Result<ImportResult, String> {
    let answer = provider.lookup(lcsc_part).map_err(|e| e.message)?;
    let stale = match answer.served {
        Served::StaleCache { age_s, .. } => Some(age_s),
        _ => None,
    };
    let record = answer
        .records
        .into_iter()
        .next()
        .ok_or("LCSC returned no record for that part number")?;
    let mut plan = lcsc::import_plan(&record)?;
    if let Some(age_s) = stale {
        // The record keeps its original retrieval time and hash; say so in the notes.
        plan.notes.push(format!(
            "LCSC was unreachable: this record is an earlier answer from {age_s} s ago, not a fresh fetch"
        ));
    }
    let component_id = plan.definition.id.clone();
    let response = s.with_engine(|e| {
        let dir = e
            .dir()
            .ok_or("importing a part needs a saved project folder")?
            .join("sourcing");
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
            sourcing::write_record(&dir, &record).map_err(|err| err.to_string())?;
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
    pub default_out_dir: Option<String>,
}

/// The BOM (joined with stored LCSC records) and the JLCPCB DFM rule check for the
/// current revision. Writes nothing.
pub fn manufacturing_preview(s: &Session) -> Result<ManufacturingPreview, String> {
    let records = stored_records(s);
    s.with_engine(|e| {
        let p = e.project();
        let lines = bom::build(p, &records);
        let dfm = jlcpcb::dfm(p, &lines, &jlcpcb::Profile::two_layer_1oz());
        ManufacturingPreview {
            revision: p.revision,
            bom: lines,
            dfm,
            default_out_dir: e
                .dir()
                .map(|d| d.join("output").join(format!("jlcpcb-rev-{}", p.revision)))
                .map(|d| d.display().to_string()),
        }
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
        if entries.next().is_some() && !dir.join("jlcpcb-prep-manifest.json").is_file() {
            return Err(format!(
                "{} is not empty and is not an earlier EAK JLCPCB bundle; choose an empty folder",
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
/// JLCPCB preparation bundle into `out_dir` (the default folder when empty).
pub fn jlcpcb_prepare(s: &Session, out_dir: &str) -> Result<PrepareResult, String> {
    let records = stored_records(s);
    s.with_engine(|e| {
        let target = if out_dir.trim().is_empty() {
            let d = e
                .dir()
                .ok_or("preparing a bundle needs a saved project folder")?;
            d.join("output")
                .join(format!("jlcpcb-rev-{}", e.project().revision))
        } else {
            PathBuf::from(out_dir.trim())
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
        let prep = jlcpcb::prepare(e.project(), &refs, &records, &target)?;
        Ok(PrepareResult { prep, reports })
    })?
}

/// Tauri command wrappers: argument decoding only, no logic. Network and file work runs on the
/// blocking pool (`crate::cmd::blocking`): supplier search is P4 (bounded background), the
/// import is an edit (P1, never queued), fab previews/preparation are P3 (bounded).
pub mod cmd {
    use crate::cmd::blocking;
    use crate::resources::Priority;
    use tauri::AppHandle;

    #[tauri::command(rename_all = "snake_case")]
    pub async fn parts_search(app: AppHandle, query: String) -> Result<super::PartsSearch, String> {
        blocking(app, Priority::P4, move |s| {
            super::parts_search(s, super::live_provider(), &query)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn parts_import(
        app: AppHandle,
        expected_revision: u64,
        lcsc: String,
    ) -> Result<super::ImportResult, String> {
        blocking(app, Priority::P1, move |s| {
            super::parts_import(s, super::live_provider(), expected_revision, &lcsc)
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
    pub async fn jlcpcb_prepare(
        app: AppHandle,
        out_dir: String,
    ) -> Result<super::PrepareResult, String> {
        blocking(app, Priority::P3, move |s| {
            super::jlcpcb_prepare(s, &out_dir)
        })
        .await
    }
}
