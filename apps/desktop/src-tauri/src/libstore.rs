//! Library scopes beyond the built-in and project libraries (ADR-0033): the user's personal
//! library, imported open-source libraries, and per-user favourites, tags and recents. Plus the
//! merged, filterable browse over every scope.
//!
//! Layout, under `<app config dir>/library/`:
//!
//! ```text
//! personal/<id>.json                 one ComponentDef each
//! imported/<name>/manifest.json      {"name": "...", "licence": "...", "source_url": "..."}
//! imported/<name>/<id>.json          one ComponentDef each
//! meta.json                          {"favourites": [...], "tags": {...}, "recent": [...]}
//! ```
//!
//! Rules: every definition must pass `ComponentDef::validate` (pin/pad mapping) before it is
//! stored; nothing here invents a pinout, footprint or rating; an open-source library is
//! imported only with a licence and a source recorded in its manifest (no licence, no import)
//! and its definitions are marked as not verified by EAK; supplier data attached to an entry
//! comes from the project's stored sourcing records and is never evidence of suitability.
//! No third-party library assets are fetched or copied automatically: the user names a folder.
//!
//! The same id may exist in several scopes; entries are keyed by (scope, id) and never merged.

use crate::Session;
use eak_core::ComponentDef;
use eak_manufacturing::library_link::Scope;
use eak_manufacturing::sourcing::SourcingRecord;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const MAX_TAGS: usize = 8;
const MAX_TAG_LEN: usize = 24;
const MAX_RECENT: usize = 20;
const MAX_LIBRARY_FILES: usize = 5000;

/// Ids and library names become file names: letters, digits, `_`, `-`, `.`; no leading dot.
pub fn safe_name(s: &str) -> Result<&str, String> {
    let ok = !s.is_empty()
        && s.len() <= 80
        && !s.starts_with('.')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if ok {
        Ok(s)
    } else {
        Err(format!(
            "{s:?} is not a usable name (1-80 letters, digits, `_`, `-`, `.`; no leading dot)"
        ))
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))
}

fn read_def(path: &Path) -> Result<ComponentDef, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let def: ComponentDef = serde_json::from_str(&text)
        .map_err(|e| format!("{}: not a component definition: {e}", path.display()))?;
    let problems = def.validate();
    if problems.is_empty() {
        Ok(def)
    } else {
        Err(format!("{}: {}", path.display(), problems.join("; ")))
    }
}

// ---------------------------------------------------------------------------------------
// User metadata
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Meta {
    #[serde(default)]
    pub favourites: BTreeSet<String>,
    #[serde(default)]
    pub tags: BTreeMap<String, BTreeSet<String>>,
    /// Most recent first.
    #[serde(default)]
    pub recent: Vec<String>,
}

/// Normalise user tags: trimmed, lowercase, bounded, de-duplicated.
pub fn clean_tags(tags: &[String]) -> Result<BTreeSet<String>, String> {
    let mut out = BTreeSet::new();
    for t in tags {
        let t = t.trim().to_lowercase();
        if t.is_empty() {
            continue;
        }
        if t.chars().count() > MAX_TAG_LEN || t.chars().any(char::is_control) {
            return Err(format!("tag {t:?} is too long or has control characters"));
        }
        out.insert(t);
    }
    if out.len() > MAX_TAGS {
        return Err(format!("at most {MAX_TAGS} tags per part"));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub licence: String,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedLibrary {
    pub manifest: Manifest,
    pub components: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportSummary {
    pub library: ImportedLibrary,
    pub accepted: Vec<String>,
    /// Files that were not imported, with the reason.
    pub rejected: Vec<String>,
}

pub struct LibraryStore {
    root: PathBuf,
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut files: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "json")
                && p.file_name().is_some_and(|n| n != "manifest.json")
        })
        .collect();
    files.sort();
    files
}

fn read_dir_defs(dir: &Path) -> (Vec<ComponentDef>, Vec<String>) {
    let mut defs = vec![];
    let mut problems = vec![];
    for f in json_files(dir).into_iter().take(MAX_LIBRARY_FILES) {
        match read_def(&f) {
            Ok(d) => defs.push(d),
            Err(e) => problems.push(e),
        }
    }
    (defs, problems)
}

impl LibraryStore {
    pub fn new(root: PathBuf) -> LibraryStore {
        LibraryStore { root }
    }

    /// Library root folder (for state files kept beside the libraries).
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn personal_dir(&self) -> PathBuf {
        self.root.join("personal")
    }
    fn imported_dir(&self) -> PathBuf {
        self.root.join("imported")
    }
    fn meta_file(&self) -> PathBuf {
        self.root.join("meta.json")
    }

    // ---- personal ----

    /// Every valid personal definition; unreadable or invalid files are listed in the second
    /// value, never silently dropped.
    pub fn personal(&self) -> (Vec<ComponentDef>, Vec<String>) {
        read_dir_defs(&self.personal_dir())
    }

    pub fn personal_save(&self, mut def: ComponentDef) -> Result<(), String> {
        safe_name(&def.id)?;
        let problems = def.validate();
        if !problems.is_empty() {
            return Err(format!(
                "the definition is not consistent and was not saved: {}",
                problems.join("; ")
            ));
        }
        if !def.provenance.contains("personal library") {
            def.provenance = format!("{}; saved to the personal library", def.provenance);
        }
        let text = serde_json::to_string_pretty(&def).map_err(|e| e.to_string())?;
        write_atomic(&self.personal_dir().join(format!("{}.json", def.id)), &text)
    }

    pub fn personal_remove(&self, id: &str) -> Result<(), String> {
        safe_name(id)?;
        let f = self.personal_dir().join(format!("{id}.json"));
        match std::fs::remove_file(&f) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("cannot remove {}: {e}", f.display())),
        }
    }

    // ---- imported open-source libraries ----

    pub fn imported(&self) -> Vec<(Manifest, Vec<ComponentDef>, Vec<String>)> {
        let Ok(rd) = std::fs::read_dir(self.imported_dir()) else {
            return vec![];
        };
        let mut dirs: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        dirs.sort();
        let mut out = vec![];
        for d in dirs.into_iter().filter(|d| d.is_dir()) {
            let Ok(text) = std::fs::read_to_string(d.join("manifest.json")) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<Manifest>(&text) else {
                continue;
            };
            let (defs, problems) = read_dir_defs(&d);
            out.push((manifest, defs, problems));
        }
        out
    }

    pub fn imported_summary(&self) -> Vec<ImportedLibrary> {
        self.imported()
            .into_iter()
            .map(|(manifest, defs, _)| ImportedLibrary {
                manifest,
                components: defs.len(),
            })
            .collect()
    }

    /// Import a folder of component-definition JSON files described by a `manifest.json`
    /// (name, licence, source). Fails closed without a licence and a source. Only definitions
    /// that validate are kept; their provenance names the library, licence and source and says
    /// EAK has not verified them.
    pub fn import_library(&self, from: &Path) -> Result<ImportSummary, String> {
        if !from.is_absolute() {
            return Err("choose an absolute folder path".into());
        }
        let text = std::fs::read_to_string(from.join("manifest.json")).map_err(|e| {
            format!(
                "{} needs a manifest.json with the library's name, licence and source URL: {e}",
                from.display()
            )
        })?;
        let manifest: Manifest = serde_json::from_str(&text)
            .map_err(|e| format!("manifest.json is not valid (name, licence, source_url): {e}"))?;
        safe_name(&manifest.name)?;
        if manifest.licence.trim().is_empty() {
            return Err(
                "the manifest names no licence: a library is not imported without one".into(),
            );
        }
        if !manifest.source_url.starts_with("https://")
            && !manifest.source_url.starts_with("http://")
        {
            return Err("the manifest's source_url must be an http(s) URL".into());
        }
        let target = self.imported_dir().join(&manifest.name);
        if target.exists() {
            return Err(format!(
                "a library named {:?} is already imported; remove it first",
                manifest.name
            ));
        }
        let files = json_files(from);
        if files.len() > MAX_LIBRARY_FILES {
            return Err(format!(
                "more than {MAX_LIBRARY_FILES} files; split the library"
            ));
        }
        let mut accepted = vec![];
        let mut rejected = vec![];
        let mut seen = BTreeSet::new();
        for f in files {
            match read_def(&f) {
                Ok(mut def) => {
                    if safe_name(&def.id).is_err() || !seen.insert(def.id.clone()) {
                        rejected.push(format!(
                            "{}: unusable or duplicate id {:?}",
                            f.display(),
                            def.id
                        ));
                        continue;
                    }
                    def.provenance = format!(
                        "imported open-source library {} (licence {}; source {}); definition not \
                         verified by EAK; original provenance: {}",
                        manifest.name, manifest.licence, manifest.source_url, def.provenance
                    );
                    let text = serde_json::to_string_pretty(&def).map_err(|e| e.to_string())?;
                    write_atomic(&target.join(format!("{}.json", def.id)), &text)?;
                    accepted.push(def.id);
                }
                Err(why) => rejected.push(why),
            }
        }
        if accepted.is_empty() {
            return Err(format!(
                "no valid component definition found in {}: {}",
                from.display(),
                rejected.join(" | ")
            ));
        }
        write_atomic(
            &target.join("manifest.json"),
            &serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
        )?;
        Ok(ImportSummary {
            library: ImportedLibrary {
                manifest,
                components: accepted.len(),
            },
            accepted,
            rejected,
        })
    }

    pub fn remove_imported(&self, name: &str) -> Result<(), String> {
        safe_name(name)?;
        let d = self.imported_dir().join(name);
        match std::fs::remove_dir_all(&d) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("cannot remove {}: {e}", d.display())),
        }
    }

    // ---- metadata ----

    pub fn meta(&self) -> Meta {
        std::fs::read_to_string(self.meta_file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn update_meta(&self, f: impl FnOnce(&mut Meta)) -> Result<Meta, String> {
        let mut m = self.meta();
        f(&mut m);
        write_atomic(
            &self.meta_file(),
            &serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?,
        )?;
        Ok(m)
    }

    pub fn set_favourite(&self, id: &str, on: bool) -> Result<Meta, String> {
        safe_name(id)?;
        self.update_meta(|m| {
            if on {
                m.favourites.insert(id.to_string());
            } else {
                m.favourites.remove(id);
            }
        })
    }

    pub fn set_tags(&self, id: &str, tags: &[String]) -> Result<Meta, String> {
        safe_name(id)?;
        let tags = clean_tags(tags)?;
        self.update_meta(|m| {
            if tags.is_empty() {
                m.tags.remove(id);
            } else {
                m.tags.insert(id.to_string(), tags);
            }
        })
    }

    /// Record that `id` was just used (placed, inspected). Most recent first, bounded.
    pub fn touch(&self, id: &str) -> Result<Meta, String> {
        safe_name(id)?;
        self.update_meta(|m| {
            m.recent.retain(|x| x != id);
            m.recent.insert(0, id.to_string());
            m.recent.truncate(MAX_RECENT);
        })
    }
}

// ---------------------------------------------------------------------------------------
// Merged browse
// ---------------------------------------------------------------------------------------

/// Every definition of every scope, tagged. The built-in catalogue first, then project,
/// personal, imported (by library name).
pub fn all_defs(
    store: &LibraryStore,
    project: &BTreeMap<String, ComponentDef>,
) -> Vec<(Scope, ComponentDef)> {
    let mut out: Vec<(Scope, ComponentDef)> = eak_core::builtin::components()
        .into_iter()
        .map(|d| (Scope::Builtin, d))
        .collect();
    out.extend(project.values().cloned().map(|d| (Scope::Project, d)));
    out.extend(store.personal().0.into_iter().map(|d| (Scope::Personal, d)));
    for (m, defs, _) in store.imported() {
        out.extend(
            defs.into_iter()
                .map(|d| (Scope::Imported(m.name.clone()), d)),
        );
    }
    // A disabled library is invisible to search and placement (ADR-0035).
    let off = crate::libcatalog::disabled(store.root());
    out.retain(|(sc, _)| !off.contains(&crate::libcatalog::scope_id(sc)));
    out
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BrowseQuery {
    pub query: Option<String>,
    /// `all`/empty, `builtin`, `project`, `personal`, `imported` (any) or `imported:<name>`.
    pub source: Option<String>,
    pub category: Option<String>,
    pub manufacturer: Option<String>,
    /// Footprint name (the package as the library knows it).
    pub package: Option<String>,
    pub tag: Option<String>,
    pub favourites_only: Option<bool>,
    pub recent_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupplierInfo {
    pub distributor: String,
    pub stock: Option<u64>,
    pub as_of_unix_s: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub scope: Scope,
    pub scope_label: String,
    pub id: String,
    pub def: ComponentDef,
    pub favourite: bool,
    pub tags: Vec<String>,
    /// 0 = most recently used.
    pub recent_rank: Option<usize>,
    pub unverified_ratings: usize,
    pub has_datasheet: bool,
    /// The open project's library holds a definition with this id.
    pub in_project: bool,
    /// From a stored sourcing record; a snapshot, never proof of suitability.
    pub supplier: Option<SupplierInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Facet {
    pub value: String,
    pub count: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Facets {
    pub sources: Vec<Facet>,
    pub categories: Vec<Facet>,
    pub manufacturers: Vec<Facet>,
    pub packages: Vec<Facet>,
    pub tags: Vec<Facet>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Browse {
    pub entries: Vec<Entry>,
    /// Counts over the entries that match the text query (before the other filters), so the
    /// user can see what a filter would leave.
    pub facets: Facets,
    pub total: usize,
    /// Personal/imported files that could not be read or failed validation.
    pub problems: Vec<String>,
    /// Imported open-source libraries with their recorded licence and source.
    pub imported: Vec<ImportedLibrary>,
}

fn facet(counts: BTreeMap<String, usize>) -> Vec<Facet> {
    let mut v: Vec<Facet> = counts
        .into_iter()
        .map(|(value, count)| Facet { value, count })
        .collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then(a.value.cmp(&b.value)));
    v
}

fn source_matches(want: &str, scope: &Scope) -> bool {
    match (want, scope) {
        ("" | "all", _) => true,
        ("builtin", Scope::Builtin)
        | ("project", Scope::Project)
        | ("personal", Scope::Personal)
        | ("imported", Scope::Imported(_)) => true,
        (w, s) => w == s.label(),
    }
}

/// `true` when `filter` is absent/blank or equals `value` (case-insensitive).
fn filter_eq(value: &str, filter: &Option<String>) -> bool {
    match filter.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        None => true,
        Some(t) => value.eq_ignore_ascii_case(t),
    }
}

pub fn browse(
    store: &LibraryStore,
    project: &BTreeMap<String, ComponentDef>,
    records: &[SourcingRecord],
    q: &BrowseQuery,
) -> Browse {
    let meta = store.meta();
    let mut problems = store.personal().1;
    for (_, _, p) in store.imported() {
        problems.extend(p);
    }
    let text = q.query.clone().unwrap_or_default();
    let want_source = q.source.clone().unwrap_or_default();

    let mut facets_src: BTreeMap<String, usize> = BTreeMap::new();
    let mut facets_cat: BTreeMap<String, usize> = BTreeMap::new();
    let mut facets_mfr: BTreeMap<String, usize> = BTreeMap::new();
    let mut facets_pkg: BTreeMap<String, usize> = BTreeMap::new();
    let mut facets_tag: BTreeMap<String, usize> = BTreeMap::new();
    let mut entries = vec![];
    for (scope, def) in all_defs(store, project) {
        if !def.matches_query(&text) {
            continue;
        }
        let tags: Vec<String> = meta
            .tags
            .get(&def.id)
            .map(|t| t.iter().cloned().collect())
            .unwrap_or_default();
        *facets_src.entry(scope.label()).or_default() += 1;
        *facets_cat.entry(def.category.clone()).or_default() += 1;
        if let Some(m) = def.manufacturer.as_deref().filter(|m| !m.trim().is_empty()) {
            *facets_mfr.entry(m.to_string()).or_default() += 1;
        }
        *facets_pkg.entry(def.footprint.name.clone()).or_default() += 1;
        for t in &tags {
            *facets_tag.entry(t.clone()).or_default() += 1;
        }
        let favourite = meta.favourites.contains(&def.id);
        let recent_rank = meta.recent.iter().position(|r| *r == def.id);
        let tag_ok = match q.tag.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            None => true,
            Some(t) => tags.iter().any(|x| *x == t.to_lowercase()),
        };
        let pass = source_matches(&want_source, &scope)
            && filter_eq(&def.category, &q.category)
            && filter_eq(def.manufacturer.as_deref().unwrap_or(""), &q.manufacturer)
            && filter_eq(&def.footprint.name, &q.package)
            && tag_ok
            && (!q.favourites_only.unwrap_or(false) || favourite)
            && (!q.recent_only.unwrap_or(false) || recent_rank.is_some());
        if !pass {
            continue;
        }
        let supplier = def.supplier_part("distributor").and_then(|part| {
            records
                .iter()
                .find(|r| {
                    r.distributor_part
                        .as_deref()
                        .is_some_and(|x| x.eq_ignore_ascii_case(part))
                })
                .map(|r| SupplierInfo {
                    distributor: part.to_string(),
                    stock: r.stock,
                    as_of_unix_s: r.retrieved_at_unix_s,
                })
        });
        entries.push(Entry {
            scope_label: scope.label(),
            in_project: project.contains_key(&def.id),
            unverified_ratings: def.ratings.iter().filter(|r| !r.verified).count(),
            has_datasheet: def
                .datasheet
                .as_deref()
                .is_some_and(|d| !d.trim().is_empty()),
            supplier,
            favourite,
            tags,
            recent_rank,
            id: def.id.clone(),
            scope,
            def,
        });
    }
    entries.sort_by_cached_key(|e| {
        (
            !e.favourite,
            e.recent_rank.unwrap_or(usize::MAX),
            e.scope_label.clone(),
            e.id.clone(),
        )
    });
    let total = entries.len();
    Browse {
        entries,
        facets: Facets {
            sources: facet(facets_src),
            categories: facet(facets_cat),
            manufacturers: facet(facets_mfr),
            packages: facet(facets_pkg),
            tags: facet(facets_tag),
        },
        total,
        problems,
        imported: store.imported_summary(),
    }
}

// ---------------------------------------------------------------------------------------
// Process-wide store
// ---------------------------------------------------------------------------------------

static ROOT: OnceLock<PathBuf> = OnceLock::new();
static STORE: OnceLock<LibraryStore> = OnceLock::new();

/// Set the library root (`<app config dir>/library`). Call once at start-up.
pub fn init(config_dir: &Path) {
    let _ = ROOT.set(config_dir.join("library"));
}

/// The process-wide store. Without a config directory (headless use) it falls back to a folder
/// under the OS temp directory so nothing is ever written to the working directory.
pub fn store() -> &'static LibraryStore {
    STORE.get_or_init(|| {
        LibraryStore::new(
            ROOT.get()
                .cloned()
                .unwrap_or_else(|| std::env::temp_dir().join("eak-library")),
        )
    })
}

// ---------------------------------------------------------------------------------------
// Session-aware helpers and IPC wrappers
// ---------------------------------------------------------------------------------------

/// The open project's library, or an empty one when no project is open (the personal and
/// imported scopes stay browsable without a project).
pub fn project_library(s: &Session) -> BTreeMap<String, ComponentDef> {
    s.shared_project()
        .map(|(_, p)| p.library.clone())
        .unwrap_or_default()
}

pub fn browse_for(s: &Session, q: &BrowseQuery) -> Browse {
    let records = crate::parts::stored_records(s);
    browse(store(), &project_library(s), &records, q)
}

/// Make a placement of a personal or imported part work (finding F8). The engine only knows the
/// project and built-in libraries, so a `PlaceSymbol` naming a part that lives only in the
/// user's personal library or an enabled imported library is sent as one `Batch`: an
/// `UpsertComponentDefinition` copying the definition into the project library, then the
/// original command(s). One revision, one undo step; the copy keeps its provenance (which says
/// it is not verified by EAK). The personal copy wins over an imported one; two imported
/// libraries holding the same id are refused as ambiguous. A part found nowhere (or only in a
/// disabled library) is left unchanged for the engine to reject.
/// Returns the command to execute and a note for each copied part.
pub fn resolve_scoped_parts(
    store: &LibraryStore,
    project: &BTreeMap<String, ComponentDef>,
    command: eak_runtime::Command,
) -> Result<(eak_runtime::Command, Vec<String>), String> {
    use eak_runtime::Command as C;
    let missing = |id: &str| !project.contains_key(id) && eak_core::builtin::get(id).is_none();
    let wanted: Vec<String> = {
        let steps: Vec<&C> = match &command {
            C::Batch { commands, .. } => commands.iter().collect(),
            c => vec![c],
        };
        let mut v: Vec<String> = vec![];
        for s in steps {
            if let C::PlaceSymbol { component_id, .. } = s {
                if missing(component_id.as_str()) && !v.contains(component_id) {
                    v.push(component_id.clone());
                }
            }
        }
        v
    };
    if wanted.is_empty() {
        return Ok((command, vec![]));
    }
    let defs = all_defs(store, project);
    let mut upserts = vec![];
    let mut notes = vec![];
    for id in &wanted {
        let personal = defs
            .iter()
            .find(|(sc, d)| matches!(sc, Scope::Personal) && d.id == *id);
        let imported: Vec<&(Scope, ComponentDef)> = defs
            .iter()
            .filter(|(sc, d)| matches!(sc, Scope::Imported(_)) && d.id == *id)
            .collect();
        let pick: &(Scope, ComponentDef) = match (personal, imported.as_slice()) {
            (Some(p), _) => p,
            (None, [one]) => *one,
            // Nowhere (or only in a disabled library): the engine rejects it with the reason.
            (None, []) => continue,
            (None, many) => {
                return Err(format!(
                    "component {id} exists in several imported libraries ({}); disable all but \
                     one of them in Libraries, or save the one you mean to your personal library",
                    many.iter()
                        .map(|(sc, _)| sc.label())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        };
        notes.push(format!(
            "copied {id} from the {} library into the project library",
            pick.0.label()
        ));
        upserts.push(C::UpsertComponentDefinition {
            definition: Box::new(pick.1.clone()),
        });
    }
    if upserts.is_empty() {
        return Ok((command, vec![]));
    }
    let (steps, label) = match command {
        C::Batch { commands, label } => (commands, label),
        c => (vec![c], None),
    };
    let label = label.or_else(|| Some(format!("Place {}", wanted.join(", "))));
    upserts.extend(steps);
    Ok((
        C::Batch {
            commands: upserts,
            label,
        },
        notes,
    ))
}

/// Copy a definition (project first, then built-in, then an imported library) into the
/// personal library. The copy keeps its pin/pad mapping and ratings exactly as they were.
pub fn save_to_personal(s: &Session, id: &str) -> Result<(), String> {
    let defs = all_defs(store(), &project_library(s));
    let pick = |want: fn(&Scope) -> bool| defs.iter().find(|(sc, d)| d.id == id && want(sc));
    let found = pick(|sc| matches!(sc, Scope::Project))
        .or_else(|| pick(|sc| matches!(sc, Scope::Builtin)))
        .or_else(|| pick(|sc| matches!(sc, Scope::Imported(_))))
        .ok_or_else(|| {
            format!("no component {id:?} in the project, built-in or imported libraries")
        })?;
    store().personal_save(found.1.clone())
}

/// Tauri command wrappers: argument decoding only.
pub mod cmd {
    use super::*;
    use crate::cmd::blocking;
    use crate::resources::Priority;
    use tauri::AppHandle;

    #[tauri::command(rename_all = "snake_case")]
    pub async fn library_browse(app: AppHandle, query: BrowseQuery) -> Result<Browse, String> {
        blocking(app, Priority::P0, move |s| Ok(browse_for(s, &query))).await
    }

    /// Favourite, tags and "used just now" for one part; only the fields given change.
    #[tauri::command(rename_all = "snake_case")]
    pub async fn library_meta(
        app: AppHandle,
        id: String,
        favourite: Option<bool>,
        tags: Option<Vec<String>>,
        touch: Option<bool>,
    ) -> Result<Meta, String> {
        blocking(app, Priority::P0, move |_| {
            let st = store();
            if let Some(on) = favourite {
                st.set_favourite(&id, on)?;
            }
            if let Some(t) = &tags {
                st.set_tags(&id, t)?;
            }
            if touch.unwrap_or(false) {
                st.touch(&id)?;
            }
            Ok(st.meta())
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn library_personal_save(app: AppHandle, id: String) -> Result<(), String> {
        blocking(app, Priority::P1, move |s| save_to_personal(s, &id)).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn library_personal_remove(app: AppHandle, id: String) -> Result<(), String> {
        blocking(app, Priority::P1, move |_| store().personal_remove(&id)).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn library_import_open_source(
        app: AppHandle,
        dir: String,
    ) -> Result<ImportSummary, String> {
        blocking(app, Priority::P1, move |_| {
            store().import_library(Path::new(dir.trim()))
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn library_imported_remove(app: AppHandle, name: String) -> Result<(), String> {
        blocking(app, Priority::P1, move |_| store().remove_imported(&name)).await
    }
}
