//! Live synchronisation: a typed event the desktop emits after the engine commits a revision that
//! the UI did not make itself (a terminal agent's harness call; see docs/AGENT_HARNESS.md).
//!
//! The payload names the project and the revision range, who committed it and which IR areas
//! changed (derived from the engine's own journalled change list, never from the tool request),
//! so views refetch only what moved and ignore events that are older than what they show.

use eak_runtime::store::{self, EntryState, JournalEntry};
use eak_runtime::Engine;
use serde::Serialize;
use std::collections::BTreeSet;

/// Tauri event name carrying a [`RevisionChange`].
pub const REVISION_EVENT: &str = "eak://revision-changed";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevisionChange {
    pub project_id: String,
    pub dir: Option<String>,
    pub revision_before: u64,
    pub revision_after: u64,
    /// `harness` for terminal-agent calls.
    pub source: String,
    /// Terminal session id (`EAK_SESSION_ID`) of the caller, when it gave one.
    pub session_id: Option<String>,
    /// Top-level IR sections that changed (`schematic`, `pcb`, `library`, ...). Empty means the
    /// change list was unavailable: treat everything as changed.
    pub areas: Vec<String>,
}

/// Top-level IR sections touched by the committed operations in `(before, after]`.
pub fn changed_areas(journal: &[JournalEntry], before: u64, after: u64) -> Vec<String> {
    let mut areas = BTreeSet::new();
    for e in journal {
        // The engine journals the change list on the pending record and clears it on the
        // committed one (see workspace::operation_changes).
        if e.state == EntryState::Pending && e.revision_after > before && e.revision_after <= after
        {
            areas.extend(e.changes.iter().filter_map(|c| c.path.first().cloned()));
        }
    }
    areas.into_iter().collect()
}

/// The change event for a commit that moved `engine` from `before`, or `None` when the revision
/// did not move (reads, rejected commands).
pub fn revision_change(
    engine: &Engine,
    before: u64,
    source: &str,
    session_id: Option<&str>,
) -> Option<RevisionChange> {
    let p = engine.project();
    if p.revision == before {
        return None;
    }
    let areas = engine
        .dir()
        .map(|d| changed_areas(&store::read_journal(d), before, p.revision))
        .unwrap_or_default();
    Some(RevisionChange {
        project_id: p.id.clone(),
        dir: engine.dir().map(|d| d.display().to_string()),
        revision_before: before,
        revision_after: p.revision,
        source: source.into(),
        session_id: session_id.map(String::from),
        areas,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_runtime::delta::Change;
    use eak_runtime::store::EntryKind;

    fn entry(state: EntryState, after: u64, paths: &[&[&str]]) -> JournalEntry {
        JournalEntry {
            op_id: format!("op{after}"),
            state,
            kind: EntryKind::Do,
            target_op: None,
            revision_before: after - 1,
            revision_after: after,
            actor: "agent".into(),
            command: "X".into(),
            changes: paths
                .iter()
                .map(|p| Change {
                    path: p.iter().map(|s| s.to_string()).collect(),
                    key: None,
                    before: None,
                    after: None,
                })
                .collect(),
            timestamp_ms: 0,
        }
    }

    #[test]
    fn areas_come_from_pending_changes_inside_the_revision_range() {
        let j = vec![
            entry(EntryState::Pending, 1, &[&["library"]]),
            entry(EntryState::Committed, 1, &[]),
            entry(EntryState::Pending, 2, &[&["schematic", "symbols"]]),
            entry(
                EntryState::Pending,
                3,
                &[&["pcb", "tracks"], &["pcb", "vias"]],
            ),
            entry(EntryState::Pending, 4, &[&["manufacturing"]]),
        ];
        assert_eq!(changed_areas(&j, 1, 3), vec!["pcb", "schematic"]);
        assert_eq!(changed_areas(&j, 0, 1), vec!["library"]);
        assert!(changed_areas(&j, 4, 4).is_empty());
    }
}
