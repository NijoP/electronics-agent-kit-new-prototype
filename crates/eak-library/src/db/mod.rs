//! Local library database (SQLite): stored library items with a token index for prefix search,
//! and a cache of source answers that always carries its retrieval time.
//!
//! The database is library data, never design data: a stored item enters a project only through
//! `Engine::execute`. Storing an item never raises its maturity, and an item without a licence
//! and a source is refused. A cached answer is returned with its age; whether it is still fresh
//! is the caller's stated maximum age, never assumed.

pub mod portable;

use eak_core::libmeta::LibraryMaturity;
use rusqlite::{params, Connection, OptionalExtension, ToSql};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

/// What a stored item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Symbol,
    Footprint,
    Part,
    Model3d,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Symbol => "symbol",
            ItemKind::Footprint => "footprint",
            ItemKind::Part => "part",
            ItemKind::Model3d => "model3d",
        }
    }

    pub fn parse(s: &str) -> Option<ItemKind> {
        [
            ItemKind::Symbol,
            ItemKind::Footprint,
            ItemKind::Part,
            ItemKind::Model3d,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }
}

/// An item to store. `(kind, library, name)` is its identity in the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewItem {
    pub kind: ItemKind,
    pub library: String,
    pub name: String,
    pub description: String,
    /// Extra search words (keywords, MPN, manufacturer, package ...).
    pub keywords: Vec<String>,
    pub maturity: LibraryMaturity,
    pub licence: String,
    pub source: String,
    /// The item itself (for example a `SymbolImport` or a `PartRecord`) as JSON.
    pub payload: serde_json::Value,
}

/// An item as stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredItem {
    pub id: i64,
    pub kind: ItemKind,
    pub library: String,
    pub name: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub maturity: LibraryMaturity,
    pub licence: String,
    pub source: String,
    pub payload: serde_json::Value,
    /// SHA-256 (lower-case hex) of the payload's JSON text.
    pub content_hash: String,
    pub added_at_unix_s: u64,
}

/// One cached answer with its retrieval time and age.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntry {
    pub key: String,
    pub source: String,
    pub value: String,
    pub retrieved_at_unix_s: u64,
    pub age_s: u64,
    /// `age_s <= max_age_s` for the maximum age the caller asked with.
    pub fresh: bool,
}

/// A library with its item count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibrarySummary {
    pub library: String,
    pub items: u64,
}

const SCHEMA_VERSION: &str = "1";

const COLUMNS: &str =
    "id, kind, library, name, description, maturity, licence, source, payload, content_hash, added_at, \
     keywords";

/// Lower-case search tokens: alphanumeric runs, plus the whole text without separators.
pub fn tokens(text: &str) -> Vec<String> {
    let lower = text.trim().to_lowercase();
    let mut out: Vec<String> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    let compact: String = lower.chars().filter(|c| c.is_alphanumeric()).collect();
    if !compact.is_empty() {
        out.push(compact);
    }
    out.sort();
    out.dedup();
    out
}

fn db_err(e: rusqlite::Error) -> String {
    format!("library database: {e}")
}

fn maturity_text(m: LibraryMaturity) -> String {
    serde_json::to_value(m)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn maturity_from(s: &str) -> Option<LibraryMaturity> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
}

struct RawItem {
    id: i64,
    kind: String,
    library: String,
    name: String,
    description: String,
    maturity: String,
    licence: String,
    source: String,
    payload: String,
    content_hash: String,
    added_at: i64,
    keywords: String,
}

impl RawItem {
    fn read(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawItem> {
        Ok(RawItem {
            id: r.get(0)?,
            kind: r.get(1)?,
            library: r.get(2)?,
            name: r.get(3)?,
            description: r.get(4)?,
            maturity: r.get(5)?,
            licence: r.get(6)?,
            source: r.get(7)?,
            payload: r.get(8)?,
            content_hash: r.get(9)?,
            added_at: r.get(10)?,
            keywords: r.get(11)?,
        })
    }

    fn decode(self) -> Result<StoredItem, String> {
        Ok(StoredItem {
            id: self.id,
            kind: ItemKind::parse(&self.kind)
                .ok_or_else(|| format!("unknown item kind {:?}", self.kind))?,
            library: self.library,
            name: self.name,
            description: self.description,
            keywords: serde_json::from_str(&self.keywords).map_err(|e| e.to_string())?,
            maturity: maturity_from(&self.maturity)
                .ok_or_else(|| format!("unknown maturity {:?}", self.maturity))?,
            licence: self.licence,
            source: self.source,
            payload: serde_json::from_str(&self.payload).map_err(|e| e.to_string())?,
            content_hash: self.content_hash,
            added_at_unix_s: self.added_at.max(0) as u64,
        })
    }
}

pub struct LibraryDb {
    conn: Connection,
}

impl LibraryDb {
    /// Open (creating if needed) the database file at `path`.
    pub fn open(path: &Path) -> Result<LibraryDb, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        Self::init(Connection::open(path).map_err(db_err)?)
    }

    pub fn open_in_memory() -> Result<LibraryDb, String> {
        Self::init(Connection::open_in_memory().map_err(db_err)?)
    }

    fn init(conn: Connection) -> Result<LibraryDb, String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS items(
               id INTEGER PRIMARY KEY,
               kind TEXT NOT NULL, library TEXT NOT NULL, name TEXT NOT NULL,
               description TEXT NOT NULL, keywords TEXT NOT NULL, maturity TEXT NOT NULL,
               licence TEXT NOT NULL, source TEXT NOT NULL,
               payload TEXT NOT NULL, content_hash TEXT NOT NULL, added_at INTEGER NOT NULL,
               UNIQUE(kind, library, name));
             CREATE TABLE IF NOT EXISTS tokens(
               token TEXT NOT NULL, item_id INTEGER NOT NULL,
               PRIMARY KEY(token, item_id)) WITHOUT ROWID;
             CREATE INDEX IF NOT EXISTS tokens_item ON tokens(item_id);
             CREATE TABLE IF NOT EXISTS cache(
               key TEXT PRIMARY KEY, source TEXT NOT NULL, value TEXT NOT NULL,
               retrieved_at INTEGER NOT NULL);",
        )
        .map_err(db_err)?;
        let version: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        match version.as_deref() {
            None => {
                conn.execute(
                    "INSERT INTO meta(key, value) VALUES ('schema_version', ?1)",
                    params![SCHEMA_VERSION],
                )
                .map_err(db_err)?;
            }
            Some(SCHEMA_VERSION) => {}
            Some(other) => {
                return Err(format!(
                    "library database schema version {other} is not supported \
                     (this build reads {SCHEMA_VERSION})"
                ));
            }
        }
        Ok(LibraryDb { conn })
    }

    /// Insert or replace an item (same kind, library and name). Returns its id. Refuses an item
    /// without a name, library, licence or source.
    pub fn upsert(&mut self, item: &NewItem, now_unix_s: u64) -> Result<i64, String> {
        for (field, v) in [
            ("library", &item.library),
            ("name", &item.name),
            ("licence", &item.licence),
            ("source", &item.source),
        ] {
            if v.trim().is_empty() {
                return Err(format!("a library item needs a {field}"));
            }
        }
        let payload = serde_json::to_string(&item.payload).map_err(|e| e.to_string())?;
        let hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
        let keywords = serde_json::to_string(&item.keywords).map_err(|e| e.to_string())?;
        let (library, name) = (item.library.trim(), item.name.trim());
        let tx = self.conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO items(kind, library, name, description, maturity, licence, source,
               payload, content_hash, added_at, keywords)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(kind, library, name) DO UPDATE SET description=excluded.description,
               maturity=excluded.maturity, licence=excluded.licence, source=excluded.source,
               payload=excluded.payload, content_hash=excluded.content_hash,
               added_at=excluded.added_at, keywords=excluded.keywords",
            params![
                item.kind.as_str(),
                library,
                name,
                item.description,
                maturity_text(item.maturity),
                item.licence.trim(),
                item.source.trim(),
                payload,
                hash,
                now_unix_s as i64,
                keywords,
            ],
        )
        .map_err(db_err)?;
        let id: i64 = tx
            .query_row(
                "SELECT id FROM items WHERE kind = ?1 AND library = ?2 AND name = ?3",
                params![item.kind.as_str(), library, name],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        tx.execute("DELETE FROM tokens WHERE item_id = ?1", params![id])
            .map_err(db_err)?;
        let mut all: Vec<String> = [name, library, item.description.as_str()]
            .into_iter()
            .chain(item.keywords.iter().map(String::as_str))
            .flat_map(tokens)
            .collect();
        all.sort();
        all.dedup();
        for t in all {
            tx.execute(
                "INSERT INTO tokens(token, item_id) VALUES (?1, ?2)",
                params![t, id],
            )
            .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(id)
    }

    fn query(&self, sql: &str, args: &[&dyn ToSql]) -> Result<Vec<StoredItem>, String> {
        let mut stmt = self.conn.prepare(sql).map_err(db_err)?;
        let rows = stmt.query_map(args, RawItem::read).map_err(db_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err)?.decode()?);
        }
        Ok(out)
    }

    pub fn get(
        &self,
        kind: ItemKind,
        library: &str,
        name: &str,
    ) -> Result<Option<StoredItem>, String> {
        let sql =
            format!("SELECT {COLUMNS} FROM items WHERE kind = ?1 AND library = ?2 AND name = ?3");
        Ok(self
            .query(&sql, &[&kind.as_str(), &library, &name])?
            .into_iter()
            .next())
    }

    /// Items where every query word is a prefix of one of the item's tokens (name, library,
    /// description and keywords), optionally of one kind, ordered by library then name. An empty
    /// query lists items.
    pub fn search(
        &self,
        query: &str,
        kind: Option<ItemKind>,
        limit: usize,
    ) -> Result<Vec<StoredItem>, String> {
        let words: Vec<String> = query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect();
        let mut sql = format!("SELECT {COLUMNS} FROM items WHERE 1 = 1");
        let mut args: Vec<Box<dyn ToSql>> = Vec::new();
        for w in words {
            args.push(Box::new(w));
            let n = args.len();
            sql.push_str(&format!(
                " AND id IN (SELECT item_id FROM tokens \
                 WHERE token >= ?{n} AND token < ?{n} || char(1114111))"
            ));
        }
        if let Some(k) = kind {
            args.push(Box::new(k.as_str()));
            sql.push_str(&format!(" AND kind = ?{}", args.len()));
        }
        args.push(Box::new(limit.min(10_000) as i64));
        sql.push_str(&format!(" ORDER BY library, name LIMIT ?{}", args.len()));
        let refs: Vec<&dyn ToSql> = args.iter().map(|b| b.as_ref()).collect();
        self.query(&sql, &refs)
    }

    /// Libraries with their item counts, by name.
    pub fn libraries(&self) -> Result<Vec<LibrarySummary>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT library, COUNT(*) FROM items GROUP BY library ORDER BY library")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(LibrarySummary {
                    library: r.get(0)?,
                    items: r.get::<_, i64>(1)?.max(0) as u64,
                })
            })
            .map_err(db_err)?;
        rows.collect::<Result<_, _>>().map_err(db_err)
    }

    /// Every item of a library, by kind then name.
    pub fn library_items(&self, library: &str) -> Result<Vec<StoredItem>, String> {
        let sql = format!("SELECT {COLUMNS} FROM items WHERE library = ?1 ORDER BY kind, name");
        self.query(&sql, &[&library])
    }

    /// Remove a library's items and their index entries. Returns how many items were removed.
    pub fn remove_library(&mut self, library: &str) -> Result<usize, String> {
        let tx = self.conn.transaction().map_err(db_err)?;
        tx.execute(
            "DELETE FROM tokens WHERE item_id IN (SELECT id FROM items WHERE library = ?1)",
            params![library],
        )
        .map_err(db_err)?;
        let n = tx
            .execute("DELETE FROM items WHERE library = ?1", params![library])
            .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(n)
    }

    /// Store a source's answer under `key` with the time it was retrieved.
    pub fn cache_put(
        &self,
        key: &str,
        source: &str,
        value: &str,
        retrieved_at_unix_s: u64,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO cache(key, source, value, retrieved_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(key) DO UPDATE SET source=excluded.source, value=excluded.value,
                   retrieved_at=excluded.retrieved_at",
                params![key, source, value, retrieved_at_unix_s as i64],
            )
            .map_err(db_err)?;
        Ok(())
    }

    /// The cached answer for `key`, with its age at `now_unix_s` and whether it is within
    /// `max_age_s`. A stale entry is still returned, marked not fresh, so the caller can show
    /// it with its date instead of presenting it as current.
    pub fn cache_get(
        &self,
        key: &str,
        now_unix_s: u64,
        max_age_s: u64,
    ) -> Result<Option<CacheEntry>, String> {
        self.conn
            .query_row(
                "SELECT source, value, retrieved_at FROM cache WHERE key = ?1",
                params![key],
                |r| {
                    let at = r.get::<_, i64>(2)?.max(0) as u64;
                    let age_s = now_unix_s.saturating_sub(at);
                    Ok(CacheEntry {
                        key: key.to_string(),
                        source: r.get(0)?,
                        value: r.get(1)?,
                        retrieved_at_unix_s: at,
                        age_s,
                        fresh: age_s <= max_age_s,
                    })
                },
            )
            .optional()
            .map_err(db_err)
    }

    /// Drop cache entries retrieved before `before_unix_s`. Returns how many were dropped.
    pub fn cache_prune(&self, before_unix_s: u64) -> Result<usize, String> {
        self.conn
            .execute(
                "DELETE FROM cache WHERE retrieved_at < ?1",
                params![before_unix_s as i64],
            )
            .map_err(db_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(kind: ItemKind, library: &str, name: &str, description: &str) -> NewItem {
        NewItem {
            kind,
            library: library.into(),
            name: name.into(),
            description: description.into(),
            keywords: vec![],
            maturity: LibraryMaturity::Imported,
            licence: "CC0-1.0".into(),
            source: "test".into(),
            payload: json!({ "name": name }),
        }
    }

    fn names(v: Vec<StoredItem>) -> Vec<String> {
        v.into_iter().map(|i| i.name).collect()
    }

    #[test]
    fn tokens_split_and_keep_the_compact_form() {
        assert_eq!(
            tokens("R_0805 2012Metric"),
            vec!["0805", "2012metric", "r", "r08052012metric"]
        );
        assert_eq!(tokens("LM358"), vec!["lm358"]);
        assert!(tokens(" - ").is_empty());
    }

    #[test]
    fn upsert_get_search_and_replace() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let mut a = item(ItemKind::Footprint, "DemoFP", "R_0805", "SMD 0805");
        a.keywords = vec!["resistor".into()];
        let id = db.upsert(&a, 100).unwrap();
        let op = item(ItemKind::Symbol, "Demo", "LM358", "Dual op amp");
        db.upsert(&op, 100).unwrap();
        db.upsert(&item(ItemKind::Symbol, "Demo", "R", "Resistor"), 100)
            .unwrap();

        let got = db.get(ItemKind::Footprint, "DemoFP", "R_0805").unwrap();
        let got = got.unwrap();
        assert_eq!(got.id, id);
        assert_eq!(got.maturity, LibraryMaturity::Imported);
        assert_eq!(got.payload, json!({ "name": "R_0805" }));
        assert_eq!(got.content_hash.len(), 64);
        assert_eq!(got.added_at_unix_s, 100);
        assert_eq!(got.keywords, vec!["resistor".to_string()]);

        let s = |q: &str, k: Option<ItemKind>| names(db.search(q, k, 50).unwrap());
        assert_eq!(s("resis", None), vec!["R", "R_0805"]);
        assert_eq!(s("res 0805", None), vec!["R_0805"]);
        assert_eq!(s("r0805", None), vec!["R_0805"]);
        assert_eq!(s("lm3", Some(ItemKind::Symbol)), vec!["LM358"]);
        assert!(s("lm3", Some(ItemKind::Footprint)).is_empty());
        assert!(s("nothing", None).is_empty());
        assert_eq!(db.search("", None, 2).unwrap().len(), 2);

        // Replacing re-indexes: the old words no longer match.
        let cmp = item(ItemKind::Symbol, "Demo", "LM358", "Comparator");
        db.upsert(&cmp, 200).unwrap();
        let s = |q: &str| names(db.search(q, None, 50).unwrap());
        assert!(s("dual").is_empty());
        assert_eq!(s("comp"), vec!["LM358"]);
        assert_eq!(
            db.libraries().unwrap(),
            vec![
                LibrarySummary {
                    library: "Demo".into(),
                    items: 2
                },
                LibrarySummary {
                    library: "DemoFP".into(),
                    items: 1
                },
            ]
        );
        assert_eq!(db.library_items("Demo").unwrap().len(), 2);
        assert_eq!(db.remove_library("Demo").unwrap(), 2);
        assert!(db.search("comp", None, 50).unwrap().is_empty());
    }

    #[test]
    fn items_without_licence_or_source_are_refused() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let mut a = item(ItemKind::Symbol, "Demo", "X", "");
        a.licence = " ".into();
        assert!(db.upsert(&a, 1).is_err());
        let mut a = item(ItemKind::Symbol, "Demo", "X", "");
        a.source = String::new();
        assert!(db.upsert(&a, 1).is_err());
        assert!(db.search("", None, 10).unwrap().is_empty());
    }

    #[test]
    fn cache_keeps_retrieval_time_and_reports_staleness() {
        let db = LibraryDb::open_in_memory().unwrap();
        assert_eq!(db.cache_get("k", 0, 60).unwrap(), None);
        db.cache_put("k", "supplier:x", "{\"stock\":5}", 1_000)
            .unwrap();
        let fresh = db.cache_get("k", 1_030, 60).unwrap().unwrap();
        assert_eq!(fresh.retrieved_at_unix_s, 1_000);
        assert_eq!(fresh.age_s, 30);
        assert!(fresh.fresh);
        let stale = db.cache_get("k", 2_000, 60).unwrap().unwrap();
        assert!(!stale.fresh, "a stale answer is returned marked stale");
        assert_eq!(stale.value, "{\"stock\":5}");
        assert_eq!(db.cache_prune(1_001).unwrap(), 1);
        assert_eq!(db.cache_get("k", 2_000, 60).unwrap(), None);
    }

    #[test]
    fn file_database_persists_and_checks_its_schema_version() {
        let path = std::env::temp_dir().join(format!("eak-libdb-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let mut db = LibraryDb::open(&path).unwrap();
            let part = item(ItemKind::Part, "Mine", "TPS2051B", "switch");
            db.upsert(&part, 5).unwrap();
        }
        let db = LibraryDb::open(&path).unwrap();
        assert_eq!(db.search("tps", None, 10).unwrap().len(), 1);
        db.conn
            .execute(
                "UPDATE meta SET value = '99' WHERE key = 'schema_version'",
                [],
            )
            .unwrap();
        drop(db);
        assert!(LibraryDb::open(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
