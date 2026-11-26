//! Proposal review IPC (slice S9, ADR-0051): the desktop side of propose mode.
//!
//! An agent session in propose mode stores its edits as a proposal under
//! `<project>/proposals/` (`eak_harness::proposal`); nothing in the project changes. These
//! commands let the person at the window list proposals, review one (EAK recomputes the delta and
//! the ERC/DRC check delta on a copy every time), apply it, discard it or export the review as
//! evidence. Apply is the person's: it goes through `eak_harness::proposal::apply` with the Human
//! actor, which refuses a stale revision, saves a checkpoint and executes the chosen steps as one
//! `Batch` through `Engine::execute` (one revision, one undo step). There is no other write path.

use crate::Session;
use eak_harness::proposal::{self, ApplyOutcome, Review};
use eak_runtime::Actor;
use serde_json::Value;

fn checked(steps: Option<Vec<usize>>) -> Result<Option<Vec<usize>>, String> {
    match &steps {
        Some(s) if s.len() > proposal::MAX_STEPS => Err("too many steps selected".into()),
        _ => Ok(steps),
    }
}

/// Every proposal of the active project (oldest first), one summary each.
pub fn proposal_list(s: &Session) -> Result<Vec<Value>, String> {
    s.with_engine(|e| {
        let current = e.project().revision;
        let id = e.project().id.clone();
        e.dir()
            .map(|d| {
                proposal::list(d)
                    .iter()
                    .filter(|p| p.project_id == id)
                    .map(|p| proposal::summary(p, current))
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// EAK's review of one proposal for the selected steps (all when `None`). Read-only.
pub fn proposal_review(s: &Session, id: &str, steps: Option<Vec<usize>>) -> Result<Review, String> {
    let steps = checked(steps)?;
    s.with_engine(|e| proposal::review(e, id, steps.as_deref()).map_err(|err| err.to_string()))?
}

/// Apply the selected steps (all when `None`) as the person at the window.
pub fn proposal_apply(
    s: &Session,
    id: &str,
    expected_revision: u64,
    steps: Option<Vec<usize>>,
) -> Result<ApplyOutcome, String> {
    let steps = checked(steps)?;
    s.with_engine(|e| {
        proposal::apply(e, id, expected_revision, steps.as_deref(), &Actor::human())
            .map_err(|err| err.to_string())
    })?
}

/// Discard a proposal (the person may discard any).
pub fn proposal_discard(s: &Session, id: &str) -> Result<Value, String> {
    s.with_engine(|e| {
        let current = e.project().revision;
        proposal::discard(e, id, &Actor::human(), None)
            .map(|p| proposal::summary(&p, current))
            .map_err(|err| err.to_string())
    })?
}

/// Save EAK's review of the selection as `evidence/proposal-<id>-rNNNNNN.json` and return the
/// path. The review is recomputed here, so the file is the engine's, not the UI's.
pub fn proposal_export(s: &Session, id: &str, steps: Option<Vec<usize>>) -> Result<String, String> {
    let steps = checked(steps)?;
    s.with_engine(|e| {
        let review = proposal::review(e, id, steps.as_deref()).map_err(|err| err.to_string())?;
        if e.dir().is_none() {
            return Err("this project has no folder, so the review cannot be saved".to_string());
        }
        let report = serde_json::json!({
            "kind": "proposal_review",
            "review": review,
            "note": "EAK's review of an agent proposal; nothing was applied by exporting it",
        });
        let json = serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?;
        e.save_evidence(&format!("proposal-{id}"), &json)
            .map(|p| p.display().to_string())
            .ok_or_else(|| "the review could not be saved".to_string())
    })?
}

/// Tauri command wrappers: argument decoding only, no logic. Listing is an interactive read
/// (P0); a review runs ERC and DRC on copies (P2, like a check); apply, discard and export write
/// (P1, never queued behind checks).
pub mod cmd {
    use crate::cmd::blocking;
    use crate::resources::Priority;
    use eak_harness::proposal::{ApplyOutcome, Review};
    use serde_json::Value;
    use tauri::AppHandle;

    #[tauri::command(rename_all = "snake_case")]
    pub async fn proposal_list(app: AppHandle) -> Result<Vec<Value>, String> {
        blocking(app, Priority::P0, super::proposal_list).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn proposal_review(
        app: AppHandle,
        id: String,
        steps: Option<Vec<usize>>,
    ) -> Result<Review, String> {
        blocking(app, Priority::P2, move |s| {
            super::proposal_review(s, &id, steps)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn proposal_apply(
        app: AppHandle,
        id: String,
        expected_revision: u64,
        steps: Option<Vec<usize>>,
    ) -> Result<ApplyOutcome, String> {
        blocking(app, Priority::P1, move |s| {
            super::proposal_apply(s, &id, expected_revision, steps)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn proposal_discard(app: AppHandle, id: String) -> Result<Value, String> {
        blocking(app, Priority::P1, move |s| super::proposal_discard(s, &id)).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn proposal_export(
        app: AppHandle,
        id: String,
        steps: Option<Vec<usize>>,
    ) -> Result<String, String> {
        blocking(app, Priority::P1, move |s| {
            super::proposal_export(s, &id, steps)
        })
        .await
    }
}

/// The agent activity timeline of the active project (ADR-0053): committed revisions, proposal
/// events, WEFT messages and saved check reports, read from the project's own journals. Read-only.
pub fn activity_timeline(s: &Session) -> Result<Vec<eak_harness::activity::Item>, String> {
    s.with_engine(|e| {
        e.dir()
            .map(eak_harness::activity::timeline)
            .unwrap_or_default()
    })
}

pub mod activity_cmd {
    use crate::cmd::blocking;
    use crate::resources::Priority;
    use tauri::AppHandle;

    #[tauri::command(rename_all = "snake_case")]
    pub async fn activity_timeline(
        app: AppHandle,
    ) -> Result<Vec<eak_harness::activity::Item>, String> {
        blocking(app, Priority::P0, super::activity_timeline).await
    }
}

/// WEFT conflicts among the open proposals (ADR-0053): stale ones and overlapping pairs.
pub fn weft_conflicts(s: &Session) -> Result<Vec<Value>, String> {
    s.with_engine(|e| eak_harness::weft::conflict_list(e))?
}

/// Review requests agents routed to the engineer and the engineer has not answered.
pub fn weft_engineer_queue(s: &Session) -> Result<Vec<Value>, String> {
    s.with_engine(|e| eak_harness::weft::engineer_queue(e))
}

/// The engineer's verdict on a proposal, sent to its author on the WEFT bus (approve,
/// request_changes, reject); with `discard` the proposal is discarded first. Applies nothing.
pub fn weft_engineer_verdict(
    s: &Session,
    proposal_id: &str,
    verdict: &str,
    body: &str,
    discard: bool,
) -> Result<Value, String> {
    let verdict: eak_harness::weft::Verdict = serde_json::from_value(Value::from(verdict))
        .map_err(|_| format!("unknown verdict {verdict}: approve, request_changes or reject"))?;
    s.with_engine(|e| eak_harness::weft::engineer_verdict(e, proposal_id, verdict, body, discard))?
}

pub mod weft_cmd {
    use crate::cmd::blocking;
    use crate::resources::Priority;
    use serde_json::Value;
    use tauri::AppHandle;

    #[tauri::command(rename_all = "snake_case")]
    pub async fn weft_conflicts(app: AppHandle) -> Result<Vec<Value>, String> {
        // Reviews each open proposal on a copy, like a check.
        blocking(app, Priority::P2, super::weft_conflicts).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn weft_engineer_queue(app: AppHandle) -> Result<Vec<Value>, String> {
        blocking(app, Priority::P0, super::weft_engineer_queue).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn weft_engineer_verdict(
        app: AppHandle,
        proposal_id: String,
        verdict: String,
        body: String,
        discard: bool,
    ) -> Result<Value, String> {
        blocking(app, Priority::P1, move |s| {
            super::weft_engineer_verdict(s, &proposal_id, &verdict, &body, discard)
        })
        .await
    }
}
