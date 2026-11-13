//! Local SQLite metadata index: recent projects and an event log for lookup.
//! It is a rebuildable cache — never canonical design data (Spec §2).

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    pub path: String,
    pub project_id: String,
    pub name: String,
    pub revision: u64,
    pub last_opened_ms: i64,
}

pub struct Index {
    db: Connection,
}

impl Index {
    pub fn open(path: &Path) -> Result<Index, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        Self::init(db)
    }

    pub fn in_memory() -> Result<Index, String> {
        Self::init(Connection::open_in_memory().map_err(|e| e.to_string())?)
    }

    fn init(db: Connection) -> Result<Index, String> {
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS recent_projects(path TEXT PRIMARY KEY, project_id TEXT NOT NULL, name TEXT NOT NULL, revision INTEGER NOT NULL, last_opened_ms INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS events(project_id TEXT NOT NULL, seq INTEGER NOT NULL, ts INTEGER NOT NULL, kind TEXT NOT NULL, actor TEXT NOT NULL, summary TEXT NOT NULL, revision INTEGER NOT NULL);
             CREATE INDEX IF NOT EXISTS events_project ON events(project_id, ts);",
        )
        .map_err(|e| e.to_string())?;
        Ok(Index { db })
    }

    pub fn touch_project(
        &self,
        path: &str,
        project_id: &str,
        name: &str,
        revision: u64,
    ) -> Result<(), String> {
        self.db
            .execute(
                "INSERT INTO recent_projects(path, project_id, name, revision, last_opened_ms) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(path) DO UPDATE SET project_id=excluded.project_id, name=excluded.name, revision=excluded.revision, last_opened_ms=excluded.last_opened_ms",
                params![path, project_id, name, revision as i64, crate::store::now_ms() as i64],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<RecentProject>, String> {
        let mut st = self.db.prepare("SELECT path, project_id, name, revision, last_opened_ms FROM recent_projects ORDER BY last_opened_ms DESC LIMIT ?1").map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![limit as i64], |r| {
                Ok(RecentProject {
                    path: r.get(0)?,
                    project_id: r.get(1)?,
                    name: r.get(2)?,
                    revision: r.get::<_, i64>(3)? as u64,
                    last_opened_ms: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn forget(&self, path: &str) -> Result<(), String> {
        self.db
            .execute("DELETE FROM recent_projects WHERE path=?1", params![path])
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn log_event(&self, e: &crate::engine::Event) -> Result<(), String> {
        self.db
            .execute(
                "INSERT INTO events(project_id, seq, ts, kind, actor, summary, revision) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![e.project_id, e.seq as i64, e.timestamp_ms as i64, e.kind, e.actor, e.summary, e.revision as i64],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn event_count(&self, project_id: &str) -> Result<u64, String> {
        self.db
            .query_row(
                "SELECT COUNT(*) FROM events WHERE project_id=?1",
                params![project_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(|e| e.to_string())
    }
}
