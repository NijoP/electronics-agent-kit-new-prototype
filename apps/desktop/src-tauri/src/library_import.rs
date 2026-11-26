//! KiCad library folders into the local library database, search over it, and portable user
//! library files (`eak_library::{kicad_lib, db}`).
//!
//! The database lives at `<app config dir>/library/library.sqlite`. Every stored item keeps the
//! licence and source the user named and the maturity `imported`; nothing here verifies an item
//! or adds it to a design (that goes through the engine from the library browser).
//!
//! `library_db_search` is named so because `library_search` already searches the merged
//! component library.

use eak_core::libmeta::LibraryMaturity;
use eak_library::db::portable::{export_library, import_library, UserLibraryImport};
use eak_library::db::{ItemKind, LibraryDb, LibrarySummary, NewItem};
use eak_library::kicad_lib::{
    import_folder, kicad_env_vars, scan_folder, FolderScan, ImportOptions, LibraryImport, Pairing,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Set the database location from the app config dir. Call once at start-up.
pub fn init(config_dir: &Path) {
    let _ = ROOT.set(config_dir.join("library").join("library.sqlite"));
}

/// The database file. Without a config dir (headless use) it is under the OS temp directory.
pub fn db_path() -> PathBuf {
    ROOT.get().cloned().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("eak-library")
            .join("library.sqlite")
    })
}

fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// What a folder import stored, and everything the user needs to see about what it did not.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FolderImportSummary {
    pub folder: String,
    pub symbols: usize,
    pub footprints: usize,
    /// Items written to the database.
    pub stored: usize,
    /// `library:symbol` of symbols without a matching footprint (stored, but unpaired).
    pub unpaired: Vec<String>,
    pub models_found: usize,
    /// `library:footprint` whose 3D model was not found on disk (or has an unset variable).
    pub models_missing: Vec<String>,
    /// Libraries found but not imported, with the reason.
    pub skipped_libraries: Vec<String>,
    /// Files that could not be read, items the database refused, and table problems.
    pub problems: Vec<String>,
}

fn keywords_of(pairing: &Pairing) -> Vec<String> {
    match pairing {
        Pairing::Paired { library, footprint } => vec![format!("{library}:{footprint}")],
        Pairing::Unqualified { reference } | Pairing::NotFound { reference } => {
            vec![reference.clone()]
        }
        Pairing::NoFootprintProperty => vec![],
    }
}

fn store(db: &mut LibraryDb, item: NewItem, now: u64, sum: &mut FolderImportSummary) {
    match db.upsert(&item, now) {
        Ok(_) => sum.stored += 1,
        Err(e) => sum
            .problems
            .push(format!("{}:{}: {e}", item.library, item.name)),
    }
}

/// Store an import's symbols and footprints and summarise it.
pub fn store_import(
    db: &mut LibraryDb,
    imp: &LibraryImport,
    licence: &str,
    source: &str,
    now: u64,
) -> FolderImportSummary {
    let mut sum = FolderImportSummary {
        folder: imp.scan.folder.clone(),
        symbols: imp.symbols.len(),
        footprints: imp.footprints.len(),
        unpaired: imp.unpaired.clone(),
        ..Default::default()
    };
    let libs = imp.scan.symbol_libs.iter().chain(&imp.scan.footprint_libs);
    for lib in libs.filter(|l| !l.usable) {
        sum.skipped_libraries
            .push(format!("{}: {}", lib.name, lib.notes.join("; ")));
    }
    sum.problems.extend(imp.scan.problems.iter().cloned());
    for f in &imp.files {
        if let Some(e) = &f.error {
            sum.problems.push(format!("{}: {e}", f.file));
        }
    }
    for s in &imp.symbols {
        let mut keywords = keywords_of(&s.pairing);
        keywords.extend(s.symbol.value.iter().cloned());
        let item = NewItem {
            kind: ItemKind::Symbol,
            library: s.library.clone(),
            name: s.symbol.name.clone(),
            description: s.symbol.value.clone().unwrap_or_default(),
            keywords,
            maturity: s.meta.maturity,
            licence: licence.to_string(),
            source: source.to_string(),
            payload: serde_json::to_value(s).unwrap_or_default(),
        };
        store(db, item, now, &mut sum);
    }
    for f in &imp.footprints {
        match &f.model {
            Some(m) if m.found => sum.models_found += 1,
            Some(_) => sum.models_missing.push(format!("{}:{}", f.library, f.name)),
            None => {}
        }
        let item = NewItem {
            kind: ItemKind::Footprint,
            library: f.library.clone(),
            name: f.name.clone(),
            description: f.footprint.description.clone().unwrap_or_default(),
            keywords: vec![],
            maturity: f.meta.maturity,
            licence: licence.to_string(),
            source: source.to_string(),
            payload: serde_json::to_value(f).unwrap_or_default(),
        };
        store(db, item, now, &mut sum);
    }
    sum
}

/// Import a folder into `db`. Licence and source are required (see `kicad_lib::import_folder`).
pub fn import_into(
    db: &mut LibraryDb,
    folder: &Path,
    licence: &str,
    source: &str,
) -> Result<FolderImportSummary, String> {
    let opts = ImportOptions {
        vars: kicad_env_vars(),
        licence: licence.to_string(),
        source: source.to_string(),
    };
    let imp = import_folder(folder, &opts)?;
    Ok(store_import(
        db,
        &imp,
        licence.trim(),
        source.trim(),
        now_s(),
    ))
}

/// One search hit, without the stored payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRow {
    pub kind: ItemKind,
    pub library: String,
    pub name: String,
    pub description: String,
    pub maturity: LibraryMaturity,
    pub licence: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DbSearch {
    pub libraries: Vec<LibrarySummary>,
    pub items: Vec<SearchRow>,
}

pub fn search(db: &LibraryDb, query: &str, kind: Option<&str>) -> Result<DbSearch, String> {
    let kind = match kind.map(str::trim).filter(|k| !k.is_empty()) {
        None => None,
        Some(k) => Some(ItemKind::parse(k).ok_or_else(|| format!("unknown item kind {k:?}"))?),
    };
    let items = db
        .search(query, kind, 200)?
        .into_iter()
        .map(|i| SearchRow {
            kind: i.kind,
            library: i.library,
            name: i.name,
            description: i.description,
            maturity: i.maturity,
            licence: i.licence,
            source: i.source,
        })
        .collect();
    Ok(DbSearch {
        libraries: db.libraries()?,
        items,
    })
}

/// File name for an exported library: the library name restricted to a safe set.
pub fn export_file_name(library: &str) -> String {
    let safe: String = library
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{safe}.eak-library.json")
}

/// Write `library` as `<folder>/<library>.eak-library.json`; returns the path written.
pub fn export_to(db: &LibraryDb, library: &str, folder: &Path) -> Result<String, String> {
    let text = export_library(db, library, now_s())?;
    let path = folder.join(export_file_name(library));
    std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

/// Read a user library file into `db`, under `rename` when given.
pub fn import_from(
    db: &mut LibraryDb,
    file: &Path,
    rename: Option<&str>,
) -> Result<UserLibraryImport, String> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    import_library(db, &text, rename, now_s())
}

pub mod cmd {
    use super::*;
    use crate::cmd::blocking;
    use crate::resources::Priority;
    use tauri::AppHandle;

    fn open() -> Result<LibraryDb, String> {
        LibraryDb::open(&db_path())
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn kicad_library_scan(app: AppHandle, folder: String) -> Result<FolderScan, String> {
        blocking(app, Priority::P0, move |_| {
            Ok(scan_folder(Path::new(folder.trim()), &kicad_env_vars()))
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn kicad_library_import(
        app: AppHandle,
        folder: String,
        licence: String,
        source: String,
    ) -> Result<FolderImportSummary, String> {
        blocking(app, Priority::P1, move |_| {
            import_into(&mut open()?, Path::new(folder.trim()), &licence, &source)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn library_db_search(
        app: AppHandle,
        query: String,
        kind: Option<String>,
    ) -> Result<DbSearch, String> {
        blocking(app, Priority::P0, move |_| {
            search(&open()?, &query, kind.as_deref())
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn user_library_export(
        app: AppHandle,
        library: String,
        folder: String,
    ) -> Result<String, String> {
        blocking(app, Priority::P1, move |_| {
            export_to(&open()?, &library, Path::new(folder.trim()))
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn user_library_import(
        app: AppHandle,
        path: String,
        library: Option<String>,
    ) -> Result<UserLibraryImport, String> {
        blocking(app, Priority::P1, move |_| {
            import_from(&mut open()?, Path::new(path.trim()), library.as_deref())
        })
        .await
    }
}
