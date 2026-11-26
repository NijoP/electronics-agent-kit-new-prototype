//! Native KiCad library import: a folder of `.kicad_sym` files and `.pretty` footprint folders,
//! found through the folder's `sym-lib-table` / `fp-lib-table` and by looking in the folder.
//!
//! * Every imported item carries [`ItemMeta`]: maturity is always
//!   [`LibraryMaturity::Imported`] (import proves nothing about correctness), and a licence and a
//!   source are required from the caller, so no item enters without them.
//! * A symbol is paired with a footprint through its `Footprint` property (`lib:name`). A symbol
//!   that cannot be paired is kept and listed in [`LibraryImport::unpaired`], never dropped.
//! * A footprint's 3D model is a [`vars::ModelRef`]: found only when the file was on disk.
//! * Every file's import report, and every file that failed, is in [`LibraryImport::files`].
//!
//! Nothing here touches a design: imported definitions enter a project only through
//! `Engine::execute` in the layers above.

pub mod table;
pub mod vars;

use eak_core::kicad_import::{
    import_footprint, import_symbol_lib, FootprintImport, ImportReport, SymbolImport,
};
use eak_core::libmeta::LibraryMaturity;
use eak_core::sexpr::Limits;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
pub use table::{parse_table, LibTable, TableEntry, TableKind};
pub use vars::{expand, kicad_env_vars, resolve_model, ModelFormat, ModelRef};

/// How a library was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibOrigin {
    /// Listed in the folder's `sym-lib-table` / `fp-lib-table`.
    Table,
    /// Found by looking in the folder.
    Folder,
}

/// One library found by [`scan_folder`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundLibrary {
    /// Nickname: the table name, or the file / folder stem.
    pub name: String,
    pub kind: TableKind,
    /// Resolved path; `None` while a path variable is unresolved.
    pub path: Option<String>,
    pub origin: LibOrigin,
    /// True only when the file (symbols) or folder (footprints) was found.
    pub exists: bool,
    /// True when it exists, is not disabled and is a KiCad-format library: what import reads.
    pub usable: bool,
    pub notes: Vec<String>,
}

/// What a folder holds, before anything is imported.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FolderScan {
    pub folder: String,
    pub symbol_libs: Vec<FoundLibrary>,
    pub footprint_libs: Vec<FoundLibrary>,
    pub problems: Vec<String>,
}

/// What the caller states about the libraries being imported. Both fields are required.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImportOptions {
    /// Path variables (for example `KICAD8_3DMODEL_DIR`); `KIPRJMOD` is always the folder.
    pub vars: BTreeMap<String, String>,
    /// Licence of the libraries, as the user states it (for example `CC-BY-SA-4.0`).
    pub licence: String,
    /// Where the libraries came from (for example a URL or "my own").
    pub source: String,
}

/// Metadata every imported item carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemMeta {
    pub maturity: LibraryMaturity,
    pub licence: String,
    pub source: String,
    /// The file the item was read from.
    pub file: String,
}

/// How a symbol's `Footprint` property was matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum Pairing {
    Paired {
        library: String,
        footprint: String,
    },
    /// The symbol has no `Footprint` property.
    NoFootprintProperty,
    /// The property has no `lib:` part, so it names no library.
    Unqualified {
        reference: String,
    },
    /// No imported footprint library holds the named footprint.
    NotFound {
        reference: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedSymbol {
    pub library: String,
    pub symbol: SymbolImport,
    pub pairing: Pairing,
    pub meta: ItemMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedFootprint {
    pub library: String,
    /// The `.kicad_mod` file stem: the name `lib:name` references use.
    pub name: String,
    pub footprint: FootprintImport,
    pub model: Option<ModelRef>,
    pub meta: ItemMeta,
}

/// One file's outcome: its import report, or why it could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReport {
    pub file: String,
    pub report: Option<ImportReport>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryImport {
    pub scan: FolderScan,
    pub symbols: Vec<ImportedSymbol>,
    pub footprints: Vec<ImportedFootprint>,
    /// `library:symbol` for every symbol whose pairing is not `Paired`.
    pub unpaired: Vec<String>,
    pub files: Vec<FileReport>,
}

fn with_prjmod(dir: &Path, vars: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut v = vars.clone();
    v.insert("KIPRJMOD".into(), dir.to_string_lossy().into_owned());
    v
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn exists_as(kind: TableKind, p: &Path) -> bool {
    match kind {
        TableKind::Symbol => p.is_file(),
        TableKind::Footprint => p.is_dir(),
    }
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn from_table(
    dir: &Path,
    kind: TableKind,
    vars: &BTreeMap<String, String>,
    out: &mut Vec<FoundLibrary>,
    problems: &mut Vec<String>,
) {
    let file = dir.join(kind.file_name());
    if !file.is_file() {
        return;
    }
    let table = match std::fs::read_to_string(&file)
        .map_err(|e| e.to_string())
        .and_then(|s| parse_table(&s, kind))
    {
        Ok(t) => t,
        Err(e) => {
            problems.push(format!("{}: {e}", file.display()));
            return;
        }
    };
    problems.extend(table.problems);
    for e in table.entries {
        let exp = expand(&e.uri, vars);
        let mut notes = Vec::new();
        let path = if exp.unresolved.is_empty() {
            let p = PathBuf::from(&exp.text);
            Some(if p.is_absolute() { p } else { dir.join(p) })
        } else {
            notes.push(format!(
                "path variable(s) not set: {}",
                exp.unresolved.join(", ")
            ));
            None
        };
        let exists = path.as_deref().is_some_and(|p| exists_as(kind, p));
        if path.is_some() && !exists {
            notes.push("not found at the resolved path".into());
        }
        if e.disabled {
            notes.push("disabled in the table".into());
        }
        let kicad_type = e.lib_type.is_empty() || e.lib_type.eq_ignore_ascii_case("kicad");
        if !kicad_type {
            notes.push(format!(
                "library type {:?} is not read (only KiCad format is)",
                e.lib_type
            ));
        }
        out.push(FoundLibrary {
            name: e.name,
            kind,
            path: path.map(|p| p.to_string_lossy().into_owned()),
            origin: LibOrigin::Table,
            exists,
            usable: exists && !e.disabled && kicad_type,
            notes,
        });
    }
}

/// Find the libraries in `dir`: every table entry, then every `.kicad_sym` file and `.pretty`
/// folder directly in `dir` that no table entry already names.
pub fn scan_folder(dir: &Path, vars: &BTreeMap<String, String>) -> FolderScan {
    let vars = with_prjmod(dir, vars);
    let mut scan = FolderScan {
        folder: dir.to_string_lossy().into_owned(),
        ..Default::default()
    };
    from_table(
        dir,
        TableKind::Symbol,
        &vars,
        &mut scan.symbol_libs,
        &mut scan.problems,
    );
    from_table(
        dir,
        TableKind::Footprint,
        &vars,
        &mut scan.footprint_libs,
        &mut scan.problems,
    );
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(e) => {
            scan.problems
                .push(format!("{}: cannot read folder: {e}", dir.display()));
            return scan;
        }
    };
    entries.sort();
    for p in entries {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        let kind = match ext {
            "kicad_sym" if p.is_file() => TableKind::Symbol,
            "pretty" if p.is_dir() => TableKind::Footprint,
            _ => continue,
        };
        let list = match kind {
            TableKind::Symbol => &mut scan.symbol_libs,
            TableKind::Footprint => &mut scan.footprint_libs,
        };
        let listed = list.iter().any(|l| {
            l.path
                .as_deref()
                .is_some_and(|q| same_path(Path::new(q), &p))
        });
        if listed {
            continue;
        }
        let name = stem(&p);
        let mut notes = Vec::new();
        if list.iter().any(|l| l.name == name) {
            notes.push(format!(
                "a library named {name:?} is already listed; this one is not imported"
            ));
        }
        let usable = notes.is_empty();
        list.push(FoundLibrary {
            name,
            kind,
            path: Some(p.to_string_lossy().into_owned()),
            origin: LibOrigin::Folder,
            exists: true,
            usable,
            notes,
        });
    }
    scan
}

fn read_file<T, E: std::fmt::Display>(
    path: &Path,
    parse: impl FnOnce(&str) -> Result<T, E>,
) -> Result<T, String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(&src).map_err(|e| e.to_string())
}

fn pair(reference: Option<&str>, available: &BTreeSet<(String, String)>) -> Pairing {
    let Some(reference) = reference.map(str::trim) else {
        return Pairing::NoFootprintProperty;
    };
    match reference.split_once(':') {
        None => Pairing::Unqualified {
            reference: reference.to_string(),
        },
        Some((l, n)) if available.contains(&(l.to_string(), n.to_string())) => Pairing::Paired {
            library: l.to_string(),
            footprint: n.to_string(),
        },
        Some(_) => Pairing::NotFound {
            reference: reference.to_string(),
        },
    }
}

/// Scan `dir` and import every usable library in it. Fails only when the licence or source is
/// missing; every per-file failure is reported in the result.
pub fn import_folder(dir: &Path, opts: &ImportOptions) -> Result<LibraryImport, String> {
    if opts.licence.trim().is_empty() {
        return Err(
            "a licence is required to import a library (state \"unknown\" if it is not known)"
                .into(),
        );
    }
    if opts.source.trim().is_empty() {
        return Err("a source is required to import a library".into());
    }
    let scan = scan_folder(dir, &opts.vars);
    let vars = with_prjmod(dir, &opts.vars);
    let meta = |file: &Path| ItemMeta {
        maturity: LibraryMaturity::Imported,
        licence: opts.licence.trim().to_string(),
        source: opts.source.trim().to_string(),
        file: file.to_string_lossy().into_owned(),
    };
    let mut files = Vec::new();
    let mut footprints = Vec::new();
    for lib in scan.footprint_libs.iter().filter(|l| l.usable) {
        let Some(folder) = lib.path.as_deref().map(Path::new) else {
            continue;
        };
        let mut mods: Vec<PathBuf> = match std::fs::read_dir(folder) {
            Ok(rd) => rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("kicad_mod"))
                .collect(),
            Err(e) => {
                files.push(FileReport {
                    file: folder.to_string_lossy().into_owned(),
                    report: None,
                    error: Some(format!("cannot read folder: {e}")),
                });
                continue;
            }
        };
        mods.sort();
        for m in mods {
            let file = m.to_string_lossy().into_owned();
            match read_file(&m, |s| import_footprint(s, Limits::default())) {
                Ok(r) => {
                    if let Some(fp) = r.footprint {
                        let model = fp
                            .model_path
                            .as_deref()
                            .map(|w| resolve_model(w, &vars, folder));
                        footprints.push(ImportedFootprint {
                            library: lib.name.clone(),
                            name: stem(&m),
                            footprint: fp,
                            model,
                            meta: meta(&m),
                        });
                    }
                    files.push(FileReport {
                        file,
                        report: Some(r.report),
                        error: None,
                    });
                }
                Err(e) => files.push(FileReport {
                    file,
                    report: None,
                    error: Some(e),
                }),
            }
        }
    }
    let available: BTreeSet<(String, String)> = footprints
        .iter()
        .map(|f| (f.library.clone(), f.name.clone()))
        .collect();
    let mut symbols = Vec::new();
    let mut unpaired = Vec::new();
    for lib in scan.symbol_libs.iter().filter(|l| l.usable) {
        let Some(path) = lib.path.as_deref().map(Path::new) else {
            continue;
        };
        let file = path.to_string_lossy().into_owned();
        let r = match read_file(path, |s| import_symbol_lib(s, Limits::default())) {
            Ok(r) => r,
            Err(e) => {
                files.push(FileReport {
                    file,
                    report: None,
                    error: Some(e),
                });
                continue;
            }
        };
        for s in r.symbols {
            let pairing = pair(s.footprint.as_deref(), &available);
            if !matches!(pairing, Pairing::Paired { .. }) {
                unpaired.push(format!("{}:{}", lib.name, s.name));
            }
            symbols.push(ImportedSymbol {
                library: lib.name.clone(),
                symbol: s,
                pairing,
                meta: meta(path),
            });
        }
        files.push(FileReport {
            file,
            report: Some(r.report),
            error: None,
        });
    }
    Ok(LibraryImport {
        scan,
        symbols,
        footprints,
        unpaired,
        files,
    })
}
