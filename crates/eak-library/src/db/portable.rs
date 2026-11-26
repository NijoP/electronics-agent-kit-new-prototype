//! Portable user library file: one library of the database written as JSON, and read back.
//!
//! ```text
//! { "format": "eak-user-library", "version": 1, "library": "Mine",
//!   "exported_at_unix_s": 1795680000,
//!   "items": [ { "kind": "symbol", "name": "...", "description": "...", "keywords": [],
//!                "maturity": "imported", "licence": "...", "source": "...",
//!                "payload": { ... }, "content_hash": "<sha-256 of the payload JSON>" } ] }
//! ```
//!
//! Reading a file never raises trust: an item whose content hash does not match its payload, or
//! that has no licence or source, is rejected with the reason; and a maturity above `imported`
//! claimed by a file is not kept (the item is stored as `imported` and listed in the report),
//! because nothing in the file proves it.

use super::{ItemKind, LibraryDb, NewItem};
use eak_core::libmeta::LibraryMaturity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const FORMAT: &str = "eak-user-library";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortableItem {
    pub kind: ItemKind,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub maturity: LibraryMaturity,
    pub licence: String,
    pub source: String,
    pub payload: serde_json::Value,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserLibraryFile {
    pub format: String,
    pub version: u32,
    pub library: String,
    pub exported_at_unix_s: u64,
    pub items: Vec<PortableItem>,
}

/// What reading a file did.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UserLibraryImport {
    pub library: String,
    pub imported: usize,
    /// `(item name, reason)` for every item not stored.
    pub rejected: Vec<(String, String)>,
    /// Items whose claimed maturity was above `imported` and was not kept.
    pub maturity_not_kept: Vec<String>,
}

fn hash(payload: &serde_json::Value) -> Result<String, String> {
    let text = serde_json::to_string(payload).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(text.as_bytes())))
}

/// Write `library` from the database as a portable file (pretty JSON). An empty or unknown
/// library is an error, so an empty file is never written by mistake.
pub fn export_library(db: &LibraryDb, library: &str, now_unix_s: u64) -> Result<String, String> {
    let items = db.library_items(library)?;
    if items.is_empty() {
        return Err(format!("library {library:?} has no items to export"));
    }
    let file = UserLibraryFile {
        format: FORMAT.into(),
        version: VERSION,
        library: library.to_string(),
        exported_at_unix_s: now_unix_s,
        items: items
            .into_iter()
            .map(|i| PortableItem {
                kind: i.kind,
                name: i.name,
                description: i.description,
                keywords: i.keywords,
                maturity: i.maturity,
                licence: i.licence,
                source: i.source,
                payload: i.payload,
                content_hash: i.content_hash,
            })
            .collect(),
    };
    serde_json::to_string_pretty(&file).map_err(|e| e.to_string())
}

/// Read a portable file into the database, under `rename` when given, else the file's library
/// name. Fails only when the file is not a user library of a supported version; each bad item
/// is rejected with the reason and the rest are stored.
pub fn import_library(
    db: &mut LibraryDb,
    text: &str,
    rename: Option<&str>,
    now_unix_s: u64,
) -> Result<UserLibraryImport, String> {
    let file: UserLibraryFile =
        serde_json::from_str(text).map_err(|e| format!("not a user library file: {e}"))?;
    if file.format != FORMAT {
        return Err(format!("format is {:?}, expected {FORMAT:?}", file.format));
    }
    if file.version != VERSION {
        return Err(format!(
            "user library version {} is not supported (this build reads {VERSION})",
            file.version
        ));
    }
    let library = rename
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(file.library.trim())
        .to_string();
    if library.is_empty() {
        return Err("the file names no library; give a library name".into());
    }
    let mut report = UserLibraryImport {
        library: library.clone(),
        ..Default::default()
    };
    for item in file.items {
        let reason = match hash(&item.payload) {
            Ok(h) if h == item.content_hash => None,
            Ok(_) => Some("content hash does not match the payload".to_string()),
            Err(e) => Some(e),
        };
        if let Some(reason) = reason {
            report.rejected.push((item.name, reason));
            continue;
        }
        let maturity = if item.maturity > LibraryMaturity::Imported {
            report.maturity_not_kept.push(item.name.clone());
            LibraryMaturity::Imported
        } else {
            item.maturity
        };
        let new = NewItem {
            kind: item.kind,
            library: library.clone(),
            name: item.name.clone(),
            description: item.description,
            keywords: item.keywords,
            maturity,
            licence: item.licence,
            source: item.source,
            payload: item.payload,
        };
        match db.upsert(&new, now_unix_s) {
            Ok(_) => report.imported += 1,
            Err(e) => report.rejected.push((item.name, e)),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn put(db: &mut LibraryDb, name: &str, maturity: LibraryMaturity) {
        let item = NewItem {
            kind: ItemKind::Part,
            library: "Mine".into(),
            name: name.into(),
            description: format!("{name} description"),
            keywords: vec!["kw".into()],
            maturity,
            licence: "CC-BY-4.0".into(),
            source: "my bench notes".into(),
            payload: json!({ "mpn": name }),
        };
        db.upsert(&item, 10).unwrap();
    }

    fn file_of(db: &LibraryDb) -> UserLibraryFile {
        serde_json::from_str(&export_library(db, "Mine", 99).unwrap()).unwrap()
    }

    #[test]
    fn export_then_import_round_trips() {
        let mut a = LibraryDb::open_in_memory().unwrap();
        put(&mut a, "TPS2051B", LibraryMaturity::Imported);
        put(&mut a, "LM358", LibraryMaturity::Draft);
        let text = export_library(&a, "Mine", 99).unwrap();
        let file = file_of(&a);
        assert_eq!(file.format, FORMAT);
        assert_eq!(file.items.len(), 2);

        let mut b = LibraryDb::open_in_memory().unwrap();
        let r = import_library(&mut b, &text, None, 200).unwrap();
        assert_eq!(r.imported, 2);
        assert!(r.rejected.is_empty() && r.maturity_not_kept.is_empty());
        // A second export of the imported copy carries the same items.
        assert_eq!(file_of(&b).items, file.items);

        let r = import_library(&mut b, &text, Some("Copy"), 200).unwrap();
        assert_eq!(r.library, "Copy");
        assert_eq!(b.library_items("Copy").unwrap().len(), 2);
    }

    #[test]
    fn tampered_or_overclaiming_items_are_not_trusted() {
        let mut a = LibraryDb::open_in_memory().unwrap();
        put(&mut a, "GOOD", LibraryMaturity::Imported);
        put(&mut a, "BAD", LibraryMaturity::Imported);
        put(&mut a, "CLAIM", LibraryMaturity::Imported);
        let mut file = file_of(&a);
        for item in &mut file.items {
            match item.name.as_str() {
                "BAD" => item.payload = json!({ "mpn": "SOMETHING ELSE" }),
                "CLAIM" => item.maturity = LibraryMaturity::EngineerReviewed,
                _ => {}
            }
        }
        let text = serde_json::to_string(&file).unwrap();
        let mut b = LibraryDb::open_in_memory().unwrap();
        let r = import_library(&mut b, &text, None, 2).unwrap();
        assert_eq!(r.imported, 2);
        assert_eq!(r.rejected.len(), 1);
        assert_eq!(r.rejected[0].0, "BAD");
        assert_eq!(r.maturity_not_kept, vec!["CLAIM".to_string()]);
        let claim = b.get(ItemKind::Part, "Mine", "CLAIM").unwrap().unwrap();
        assert_eq!(claim.maturity, LibraryMaturity::Imported);
    }

    #[test]
    fn wrong_files_are_refused() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        assert!(export_library(&db, "Nothing", 1).is_err());
        assert!(import_library(&mut db, "{}", None, 1).is_err());
        let other = json!({
            "format": "something-else", "version": 1, "library": "X",
            "exported_at_unix_s": 1, "items": []
        });
        assert!(import_library(&mut db, &other.to_string(), None, 1).is_err());
        let future = json!({
            "format": FORMAT, "version": 2, "library": "X",
            "exported_at_unix_s": 1, "items": []
        });
        assert!(import_library(&mut db, &future.to_string(), None, 1).is_err());
    }
}
