//! Library identity, scope, maturity and a vendor-neutral catalogue (ADR-0035).
//!
//! This module describes *libraries* (the shared, versioned definition containers) and how a
//! definition is addressed. It is deliberately separate from placed design instances: a project
//! stores a [`DefRef`] (stable id + content hash) next to what it placed, so updating a library
//! never silently changes a committed design. Large assets stay outside the project IR.
//!
//! Nothing here knows any supplier. Supplier data stays generic metadata on `ComponentDef`.
//! Use these types by path (`eak_core::libmeta::...`); they are not re-exported at the root.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where a library comes from and who owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryScope {
    /// Shipped with EAK.
    System,
    /// Installed by the user for all projects.
    User,
    /// Stored inside one project.
    Project,
    /// Converted from a third-party format.
    Imported,
    /// Managed by an organisation.
    Organisation,
}

/// Transparent maturity of a definition. Ordered: a higher level implies the lower checks were
/// satisfied. A level is only ever raised with evidence; it is never inferred from import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryMaturity {
    Draft,
    Imported,
    StructurallyValidated,
    PinVerified,
    FootprintVerified,
    DatasheetVerified,
    EngineerReviewed,
}

/// Evidence for one maturity claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaturityEvidence {
    pub level: LibraryMaturity,
    /// What was checked, for example a datasheet revision and page.
    pub source: String,
    /// Who or what made the claim (a reviewer name or a tool id).
    pub by: String,
}

impl LibraryMaturity {
    /// Levels from `PinVerified` upward are claims about reality, not about structure, and
    /// need a recorded source.
    pub fn needs_evidence(self) -> bool {
        self >= LibraryMaturity::PinVerified
    }
}

/// Check a maturity claim against its evidence list. A level that needs evidence is only
/// accepted when some evidence entry at that level or higher names a source and a reviewer.
pub fn maturity_problems(claimed: LibraryMaturity, evidence: &[MaturityEvidence]) -> Vec<String> {
    let mut out = Vec::new();
    if claimed.needs_evidence() {
        let has = evidence
            .iter()
            .any(|e| e.level >= claimed && !e.source.trim().is_empty() && !e.by.trim().is_empty());
        if !has {
            out.push(format!("maturity {claimed:?} has no recorded evidence"));
        }
    }
    out
}

/// Metadata of one installed library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryManifest {
    /// Stable id, unique in a catalogue (`[a-z0-9._-]`, 1..=64).
    pub id: String,
    pub name: String,
    /// Publisher namespace, same charset as `id`.
    pub namespace: String,
    /// Dotted numeric version, for example `1.2.0`.
    pub version: String,
    /// Where the library came from (project, URL, file name); free text, never trusted as a path.
    pub source: String,
    /// SPDX licence expression or `LicenseRef-...`.
    pub license: String,
    #[serde(default)]
    pub description: String,
    /// Lower-case hex SHA-256 of the canonical library content (64 chars).
    pub content_hash: String,
    /// Lowest EAK-IR schema version able to read the library.
    pub min_schema: u32,
    pub scope: LibraryScope,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

fn ident_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

fn version_ok(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    (1..=4).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit()))
}

/// 64 lower-case hex characters.
pub fn is_content_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl LibraryManifest {
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !ident_ok(&self.id) {
            out.push(format!(
                "library id '{}' is not a valid identifier",
                self.id
            ));
        }
        if !ident_ok(&self.namespace) {
            out.push(format!(
                "namespace '{}' is not a valid identifier",
                self.namespace
            ));
        }
        if self.name.trim().is_empty() {
            out.push(format!("{}: name is empty", self.id));
        }
        if !version_ok(&self.version) {
            out.push(format!(
                "{}: version '{}' is not dotted numeric",
                self.id, self.version
            ));
        }
        if !is_content_hash(&self.content_hash) {
            out.push(format!(
                "{}: content hash is not 64 lower-case hex characters",
                self.id
            ));
        }
        if self.license.trim().is_empty() {
            out.push(format!("{}: licence is not recorded", self.id));
        }
        if self.source.trim().is_empty() {
            out.push(format!("{}: source/provenance is not recorded", self.id));
        }
        out
    }
}

/// Stable address of a definition: `library_id:item_id`. Display names are never identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct QualifiedId {
    pub library: String,
    pub item: String,
}

impl QualifiedId {
    /// Parse `library:item`. The library part must be a valid library identifier; the item part
    /// is up to 128 characters of `[A-Za-z0-9._+/-]` with no `..`.
    pub fn parse(s: &str) -> Result<Self, String> {
        let (lib, item) = s
            .split_once(':')
            .ok_or_else(|| format!("'{s}' has no library prefix (want library:item)"))?;
        if !ident_ok(lib) {
            return Err(format!("library part '{lib}' is not a valid identifier"));
        }
        if item.is_empty()
            || item.len() > 128
            || item.contains("..")
            || !item
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'/' | b'-'))
        {
            return Err(format!("item part '{item}' is not a valid identifier"));
        }
        Ok(Self {
            library: lib.to_string(),
            item: item.to_string(),
        })
    }
}

impl std::fmt::Display for QualifiedId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.library, self.item)
    }
}

/// What a project stores for a placed definition: the stable id plus the content hash of the
/// definition as it was when committed, so a later library update is detectable, not silent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefRef {
    pub id: QualifiedId,
    pub library_version: String,
    pub content_hash: String,
}

/// Result of checking a committed reference against the currently installed library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefStatus {
    /// Same library version and content.
    Current,
    /// The library is installed but differs from what the project committed. The project keeps
    /// its own snapshot until the user accepts the update.
    Drifted {
        installed_version: String,
        installed_hash: String,
    },
    /// Library not installed.
    MissingLibrary,
    /// Library installed but disabled.
    Disabled,
}

/// Errors raised when changing a catalogue.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    #[error("invalid library manifest: {0:?}")]
    Invalid(Vec<String>),
    #[error("library id '{0}' is already installed")]
    DuplicateId(String),
    #[error("namespace '{0}' with name '{1}' is already installed under another id")]
    DuplicateName(String, String),
    #[error("library '{0}' is not installed")]
    Unknown(String),
}

/// Installed libraries of any scope. Pure in-memory index; persistence and file access belong to
/// the runtime, never to React.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryCatalog {
    libraries: BTreeMap<String, LibraryManifest>,
}

impl LibraryCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install a library. Identity collisions are rejected, never merged or overwritten.
    pub fn install(&mut self, m: LibraryManifest) -> Result<(), CatalogError> {
        let problems = m.validate();
        if !problems.is_empty() {
            return Err(CatalogError::Invalid(problems));
        }
        if self.libraries.contains_key(&m.id) {
            return Err(CatalogError::DuplicateId(m.id));
        }
        if self
            .libraries
            .values()
            .any(|o| o.namespace == m.namespace && o.name.eq_ignore_ascii_case(&m.name))
        {
            return Err(CatalogError::DuplicateName(m.namespace, m.name));
        }
        self.libraries.insert(m.id.clone(), m);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&LibraryManifest> {
        self.libraries.get(id)
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), CatalogError> {
        let m = self
            .libraries
            .get_mut(id)
            .ok_or_else(|| CatalogError::Unknown(id.to_string()))?;
        m.enabled = enabled;
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Result<LibraryManifest, CatalogError> {
        self.libraries
            .remove(id)
            .ok_or_else(|| CatalogError::Unknown(id.to_string()))
    }

    /// Libraries in a scope, in id order.
    pub fn in_scope(&self, scope: LibraryScope) -> Vec<&LibraryManifest> {
        self.libraries
            .values()
            .filter(|m| m.scope == scope)
            .collect()
    }

    /// Enabled libraries, in id order. Placement and search only ever see these.
    pub fn enabled(&self) -> Vec<&LibraryManifest> {
        self.libraries.values().filter(|m| m.enabled).collect()
    }

    /// Compare a committed reference with the installed library.
    pub fn check_ref(&self, r: &DefRef) -> RefStatus {
        match self.libraries.get(&r.id.library) {
            None => RefStatus::MissingLibrary,
            Some(m) if !m.enabled => RefStatus::Disabled,
            Some(m) if m.content_hash == r.content_hash && m.version == r.library_version => {
                RefStatus::Current
            }
            Some(m) => RefStatus::Drifted {
                installed_version: m.version.clone(),
                installed_hash: m.content_hash.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn manifest(id: &str, scope: LibraryScope) -> LibraryManifest {
        LibraryManifest {
            id: id.into(),
            name: format!("Library {id}"),
            namespace: "eak".into(),
            version: "1.0.0".into(),
            source: "built-in".into(),
            license: "MIT".into(),
            description: String::new(),
            content_hash: hash('a'),
            min_schema: 1,
            scope,
            enabled: true,
        }
    }

    #[test]
    fn qualified_id_round_trips_and_rejects_bad_input() {
        let q = QualifiedId::parse("passives.r:R_0805/metric").unwrap();
        assert_eq!(q.to_string(), "passives.r:R_0805/metric");
        assert!(QualifiedId::parse("no-prefix").is_err());
        assert!(QualifiedId::parse("Bad Lib:x").is_err());
        assert!(QualifiedId::parse("lib:../etc").is_err());
        assert!(QualifiedId::parse("lib:").is_err());
        assert!(QualifiedId::parse("lib:a b").is_err());
    }

    #[test]
    fn manifest_validation_reports_each_problem() {
        let mut m = manifest("x", LibraryScope::User);
        assert!(m.validate().is_empty());
        m.content_hash = "xyz".into();
        m.license = " ".into();
        m.version = "1.x".into();
        let p = m.validate();
        assert_eq!(p.len(), 3, "{p:?}");
    }

    #[test]
    fn install_rejects_collisions_instead_of_overwriting() {
        let mut c = LibraryCatalog::new();
        c.install(manifest("a", LibraryScope::System)).unwrap();
        assert_eq!(
            c.install(manifest("a", LibraryScope::User)),
            Err(CatalogError::DuplicateId("a".into()))
        );
        let mut same_name = manifest("b", LibraryScope::User);
        same_name.name = "LIBRARY A".into();
        assert!(matches!(
            c.install(same_name),
            Err(CatalogError::DuplicateName(..))
        ));
        assert_eq!(c.get("a").unwrap().scope, LibraryScope::System);
    }

    #[test]
    fn disabled_libraries_are_hidden_and_scopes_filter() {
        let mut c = LibraryCatalog::new();
        c.install(manifest("a", LibraryScope::System)).unwrap();
        c.install(manifest("b", LibraryScope::Project)).unwrap();
        c.set_enabled("b", false).unwrap();
        assert_eq!(c.enabled().len(), 1);
        assert_eq!(c.in_scope(LibraryScope::Project).len(), 1);
        assert!(c.set_enabled("zz", true).is_err());
    }

    #[test]
    fn library_update_is_visible_as_drift_not_a_silent_change() {
        let mut c = LibraryCatalog::new();
        c.install(manifest("a", LibraryScope::System)).unwrap();
        let r = DefRef {
            id: QualifiedId::parse("a:r0805").unwrap(),
            library_version: "1.0.0".into(),
            content_hash: hash('a'),
        };
        assert_eq!(c.check_ref(&r), RefStatus::Current);
        c.remove("a").unwrap();
        let mut newer = manifest("a", LibraryScope::System);
        newer.version = "1.1.0".into();
        newer.content_hash = hash('b');
        c.install(newer).unwrap();
        assert_eq!(
            c.check_ref(&r),
            RefStatus::Drifted {
                installed_version: "1.1.0".into(),
                installed_hash: hash('b')
            }
        );
        c.set_enabled("a", false).unwrap();
        assert_eq!(c.check_ref(&r), RefStatus::Disabled);
        c.remove("a").unwrap();
        assert_eq!(c.check_ref(&r), RefStatus::MissingLibrary);
    }

    #[test]
    fn verified_maturity_requires_recorded_evidence() {
        assert!(maturity_problems(LibraryMaturity::Imported, &[]).is_empty());
        assert!(!maturity_problems(LibraryMaturity::PinVerified, &[]).is_empty());
        let ev = [MaturityEvidence {
            level: LibraryMaturity::DatasheetVerified,
            source: "datasheet rev C p.12".into(),
            by: "reviewer".into(),
        }];
        assert!(maturity_problems(LibraryMaturity::PinVerified, &ev).is_empty());
        assert!(!maturity_problems(LibraryMaturity::EngineerReviewed, &ev).is_empty());
        assert!(LibraryMaturity::Draft < LibraryMaturity::EngineerReviewed);
    }
}
