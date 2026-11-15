//! Library catalogue for the desktop (ADR-0035): the built-in, project, personal and imported
//! libraries as `eak_core::libmeta` manifests, with provenance, version, content hash and an
//! enabled flag the user can change. A disabled library is hidden from search and placement
//! (`libstore::all_defs` consults [`disabled`]). The state file lives beside the libraries, outside
//! any project; React never touches the filesystem.

use crate::libstore::store;
use crate::Session;
use eak_core::libmeta::{LibraryCatalog, LibraryManifest, LibraryScope};
use eak_core::ComponentDef;
use eak_manufacturing::library_link::Scope;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const DISABLED_FILE: &str = "disabled-libraries.json";

/// Lower-case id fragment from a user-visible name: `[a-z0-9._-]`, at most 40 characters.
pub fn slug(name: &str) -> String {
    let mut s: String = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    s.truncate(40);
    if s.is_empty() {
        s.push('x');
    }
    s
}

/// Stable library id of a scope, as used in the catalogue and the disabled list.
pub fn scope_id(sc: &Scope) -> String {
    match sc {
        Scope::Builtin => "eak.builtin".to_string(),
        Scope::Project => "project".to_string(),
        Scope::Personal => "personal".to_string(),
        Scope::Imported(n) => format!("imported.{}", slug(n)),
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DisabledFile {
    #[serde(default)]
    disabled: BTreeSet<String>,
}

fn disabled_path(root: &Path) -> PathBuf {
    root.join(DISABLED_FILE)
}

/// Ids of disabled libraries. A missing or unreadable file means nothing is disabled.
pub fn disabled(root: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(disabled_path(root))
        .ok()
        .and_then(|t| serde_json::from_str::<DisabledFile>(&t).ok())
        .map(|f| f.disabled)
        .unwrap_or_default()
}

fn save_disabled(root: &Path, set: &BTreeSet<String>) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| format!("cannot create {}: {e}", root.display()))?;
    let text = serde_json::to_string_pretty(&DisabledFile {
        disabled: set.clone(),
    })
    .map_err(|e| e.to_string())?;
    let tmp = root.join(format!("{DISABLED_FILE}.tmp"));
    std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, disabled_path(root))
        .map_err(|e| format!("cannot replace state file: {e}"))
}

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub source: String,
    pub license: String,
    pub description: String,
    pub content_hash: String,
    pub scope: LibraryScope,
    pub enabled: bool,
    pub components: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Catalog {
    pub libraries: Vec<Row>,
    /// Libraries that could not be listed (invalid manifest, id collision), never dropped silently.
    pub problems: Vec<String>,
}

struct Spec<'a> {
    id: String,
    name: String,
    namespace: &'a str,
    version: String,
    source: String,
    license: String,
    description: &'a str,
    scope: LibraryScope,
}

fn add(
    cat: &mut LibraryCatalog,
    rows: &mut Vec<Row>,
    problems: &mut Vec<String>,
    off: &BTreeSet<String>,
    spec: Spec<'_>,
    defs: &[ComponentDef],
) {
    let bytes = serde_json::to_vec(defs).unwrap_or_default();
    let enabled = !off.contains(&spec.id);
    let manifest = LibraryManifest {
        id: spec.id,
        name: spec.name,
        namespace: spec.namespace.to_string(),
        version: spec.version,
        source: spec.source,
        license: spec.license,
        description: spec.description.to_string(),
        content_hash: eak_intelligence::knowledge::sha256_hex(&bytes),
        min_schema: 1,
        scope: spec.scope,
        enabled,
    };
    match cat.install(manifest.clone()) {
        Ok(()) => rows.push(Row {
            id: manifest.id,
            name: manifest.name,
            namespace: manifest.namespace,
            version: manifest.version,
            source: manifest.source,
            license: manifest.license,
            description: manifest.description,
            content_hash: manifest.content_hash,
            scope: manifest.scope,
            enabled: manifest.enabled,
            components: defs.len(),
        }),
        Err(e) => problems.push(format!("{}: {e}", manifest.id)),
    }
}

/// Build the catalogue from the built-in, project, personal and imported libraries.
pub fn build(project: &BTreeMap<String, ComponentDef>, root: &Path) -> Catalog {
    let off = disabled(root);
    let mut cat = LibraryCatalog::new();
    let mut rows = Vec::new();
    let mut problems = Vec::new();
    let builtin = eak_core::builtin::components();
    let builtin_license = if env!("CARGO_PKG_LICENSE").is_empty() {
        "LicenseRef-EAK"
    } else {
        env!("CARGO_PKG_LICENSE")
    };
    add(
        &mut cat,
        &mut rows,
        &mut problems,
        &off,
        Spec {
            id: scope_id(&Scope::Builtin),
            name: "EAK built-in".into(),
            namespace: "eak",
            version: env!("CARGO_PKG_VERSION").to_string(),
            source: "built into EAK".into(),
            license: builtin_license.to_string(),
            description: "Starter components shipped with EAK; provenance is recorded per part.",
            scope: LibraryScope::System,
        },
        &builtin,
    );
    let proj: Vec<ComponentDef> = project.values().cloned().collect();
    add(
        &mut cat,
        &mut rows,
        &mut problems,
        &off,
        Spec {
            id: scope_id(&Scope::Project),
            name: "Open project".into(),
            namespace: "project",
            version: "1.0.0".into(),
            source: "the open project's library".into(),
            license: "LicenseRef-project-local".into(),
            description: "Definitions stored inside the open project.",
            scope: LibraryScope::Project,
        },
        &proj,
    );
    let (personal, personal_problems) = store().personal();
    for p in personal_problems {
        problems.push(format!("personal library: {p}"));
    }
    add(
        &mut cat,
        &mut rows,
        &mut problems,
        &off,
        Spec {
            id: scope_id(&Scope::Personal),
            name: "Personal".into(),
            namespace: "user",
            version: "1.0.0".into(),
            source: "your personal library folder".into(),
            license: "LicenseRef-user-supplied".into(),
            description: "Definitions you saved for use in every project.",
            scope: LibraryScope::User,
        },
        &personal,
    );
    for (m, defs, bad) in store().imported() {
        for p in bad {
            problems.push(format!("imported library {}: {p}", m.name));
        }
        add(
            &mut cat,
            &mut rows,
            &mut problems,
            &off,
            Spec {
                id: scope_id(&Scope::Imported(m.name.clone())),
                name: m.name.clone(),
                namespace: "imported",
                version: "1.0.0".into(),
                source: m.source_url.clone(),
                license: m.licence.clone(),
                description: "Imported library; not verified by EAK.",
                scope: LibraryScope::Imported,
            },
            &defs,
        );
    }
    Catalog {
        libraries: rows,
        problems,
    }
}

pub fn catalog_for(s: &Session) -> Catalog {
    build(&crate::libstore::project_library(s), store().root())
}

/// Enable or disable a listed library and return the new catalogue.
pub fn set_enabled(s: &Session, id: &str, enabled: bool) -> Result<Catalog, String> {
    let current = catalog_for(s);
    if !current.libraries.iter().any(|r| r.id == id) {
        return Err(format!("no library {id:?} in the catalogue"));
    }
    let root = store().root();
    let mut off = disabled(root);
    if enabled {
        off.remove(id);
    } else {
        off.insert(id.to_string());
    }
    save_disabled(root, &off)?;
    Ok(catalog_for(s))
}

/// Tauri command wrappers: argument decoding only.
pub mod cmd {
    use super::*;
    use crate::cmd::blocking;
    use crate::resources::Priority;
    use tauri::AppHandle;

    #[tauri::command(rename_all = "snake_case")]
    pub async fn library_catalog(app: AppHandle) -> Result<Catalog, String> {
        blocking(app, Priority::P0, move |s| Ok(catalog_for(s))).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn library_set_enabled(
        app: AppHandle,
        id: String,
        enabled: bool,
    ) -> Result<Catalog, String> {
        blocking(app, Priority::P1, move |s| set_enabled(s, &id, enabled)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_and_scope_ids_are_stable_and_safe() {
        assert_eq!(slug("My Lib/1"), "my-lib-1");
        assert_eq!(slug("  "), "x");
        assert_eq!(scope_id(&Scope::Builtin), "eak.builtin");
        assert_eq!(
            scope_id(&Scope::Imported("Open Parts".into())),
            "imported.open-parts"
        );
        assert!(slug(&"a".repeat(100)).len() <= 40);
    }

    #[test]
    fn disabled_state_round_trips_and_garbage_means_nothing_disabled() {
        let dir = std::env::temp_dir().join(format!("eak-libcatalog-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(disabled(&dir).is_empty());
        let mut set = BTreeSet::new();
        set.insert("imported.x".to_string());
        save_disabled(&dir, &set).unwrap();
        assert_eq!(disabled(&dir), set);
        std::fs::write(disabled_path(&dir), "not json").unwrap();
        assert!(disabled(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
