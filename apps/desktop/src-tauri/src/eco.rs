//! ECO preview for "Update PCB" (Phase 1 C2, ADR-0041).
//!
//! `sync_preview` answers "what would `SyncSchematicToPcb` change?" without changing anything.
//! It runs the engine's own command code on a copy of the project, exactly as
//! `Engine::execute` does (same `command::apply`, same integrity check, same `delta::diff`), so
//! the change list it returns is the delta the engine would journal if the sync were applied at
//! that revision. The only thing that can differ is the random id of a footprint the sync
//! creates; every value inside it is the same.
//!
//! Applying is not done here: the UI sends the sync as a `Batch` through `execute_command` with
//! the previewed revision, so a project that moved on after the preview rejects it as stale.

use crate::Session;
use eak_core::Project;
use eak_runtime::command::{self, Command};
use eak_runtime::delta::{self, Change};
use eak_runtime::{CommandError, Engine};
use serde::Serialize;

/// Most row ids an exported ECO report may list as not selected.
const MAX_EXCLUDED: usize = 10_000;
/// Longest row id accepted in an exported report.
const MAX_ROW_ID: usize = 256;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SyncPreview {
    /// The project revision the preview was computed for; apply only at this revision.
    pub revision: u64,
    /// The before/after delta the sync would journal (same shape as `operation_changes`).
    pub changes: Vec<Change>,
    /// The sync's own warnings, e.g. symbols it would leave off the PCB.
    pub warnings: Vec<String>,
}

/// The delta `SyncSchematicToPcb` would produce on `p`, computed on a copy (read-only).
pub fn preview_project(p: &Project) -> Result<SyncPreview, CommandError> {
    let mut next = p.clone();
    let applied = command::apply(&mut next, &Command::SyncSchematicToPcb)?;
    let problems = eak_core::integrity::check(&next);
    if !problems.is_empty() {
        return Err(CommandError::Integrity(problems.join("; ")));
    }
    Ok(SyncPreview {
        revision: p.revision,
        changes: delta::diff(p, &next),
        warnings: applied.warnings,
    })
}

/// Preview for one engine, refusing what `Engine::execute` would refuse before applying: a
/// project blocked by recovery and a stale revision (spec AG-03).
pub fn preview_for(e: &Engine, expected_revision: u64) -> Result<SyncPreview, CommandError> {
    if let Some(b) = e.blocked_reason() {
        return Err(CommandError::Precondition(b.to_string()));
    }
    let actual = e.project().revision;
    if expected_revision != actual {
        return Err(CommandError::StaleRevision {
            expected: expected_revision,
            actual,
        });
    }
    preview_project(e.project())
}

pub fn sync_preview(s: &Session, expected_revision: u64) -> Result<SyncPreview, String> {
    s.with_engine(|e| preview_for(e, expected_revision).map_err(|err| err.to_string()))?
}

#[derive(Debug, Serialize)]
struct EcoReport<'a> {
    kind: &'static str,
    revision: u64,
    changes: &'a [Change],
    warnings: &'a [String],
    /// Row ids (as the dialog shows them) the user left unchecked.
    excluded: &'a [String],
    note: &'static str,
}

/// Save the ECO preview at `expected_revision` as `evidence/eco-rNNNNNN.json` and return its
/// path. The change list is recomputed here, so the report is the engine's, not the UI's.
pub fn sync_preview_export(
    s: &Session,
    expected_revision: u64,
    excluded: Vec<String>,
) -> Result<String, String> {
    if excluded.len() > MAX_EXCLUDED || excluded.iter().any(|x| x.len() > MAX_ROW_ID) {
        return Err("the ECO report's excluded-row list is too long".into());
    }
    s.with_engine(|e| {
        let preview = preview_for(e, expected_revision).map_err(|err| err.to_string())?;
        if e.dir().is_none() {
            return Err("this project has no folder, so the ECO report cannot be saved".into());
        }
        let report = EcoReport {
            kind: "eco_preview",
            revision: preview.revision,
            changes: &preview.changes,
            warnings: &preview.warnings,
            excluded: &excluded,
            note: "preview of SyncSchematicToPcb; nothing was applied by exporting it",
        };
        let json = serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?;
        e.save_evidence("eco", &json)
            .map(|p| p.display().to_string())
            .ok_or_else(|| "the ECO report could not be saved".to_string())
    })?
}

/// Tauri command wrappers: argument decoding only, no logic. The preview is an interactive read
/// (P0); exporting writes one evidence file (P1, like an edit, never queued).
pub mod cmd {
    use crate::cmd::blocking;
    use crate::resources::Priority;
    use tauri::AppHandle;

    #[tauri::command(rename_all = "snake_case")]
    pub async fn sync_preview(
        app: AppHandle,
        expected_revision: u64,
    ) -> Result<super::SyncPreview, String> {
        blocking(app, Priority::P0, move |s| {
            super::sync_preview(s, expected_revision)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn sync_preview_export(
        app: AppHandle,
        expected_revision: u64,
        excluded: Vec<String>,
    ) -> Result<String, String> {
        blocking(app, Priority::P1, move |s| {
            super::sync_preview_export(s, expected_revision, excluded)
        })
        .await
    }
}
