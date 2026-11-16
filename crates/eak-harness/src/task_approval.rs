//! Human approval as a suspended node (ADR-0036).
//!
//! An approval node does not run a model: it asks a person a question and waits. Asking writes an
//! `ApprovalRequested` entry (with the evidence the person should look at) and suspends the node in
//! the journal, so the wait survives a restart and releases any worker. Deciding is a separate,
//! attributable act: it needs a named approver, a pending request, and the design must still be at
//! the revision the request was made against; otherwise the person approved something that no
//! longer exists and must be asked again. An approval that was invalidated is no longer pending.

use crate::task_journal::{Entry, Event, Journal};
use std::collections::BTreeMap;

/// A question waiting for a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub node: String,
    pub question: String,
    /// References (output refs, report ids) the person is shown with the question.
    pub evidence: Vec<String>,
    /// Project revision when the question was asked.
    pub revision: u64,
    pub requested_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approved,
    Denied,
}

/// Open requests for one run, in node order. A request is closed by a decision or by the node
/// being invalidated (the design moved on, so the old question is void).
pub fn pending(entries: &[Entry], run: &str) -> Vec<Pending> {
    let mut open: BTreeMap<String, Pending> = BTreeMap::new();
    for e in entries.iter().filter(|e| e.run == run) {
        match &e.event {
            Event::ApprovalRequested {
                node,
                question,
                evidence,
            } => {
                open.insert(
                    node.clone(),
                    Pending {
                        node: node.clone(),
                        question: question.clone(),
                        evidence: evidence.clone(),
                        revision: e.revision,
                        requested_seq: e.seq,
                    },
                );
            }
            Event::ApprovalDecided { node, .. } | Event::NodeInvalidated { node, .. } => {
                open.remove(node);
            }
            _ => {}
        }
    }
    open.into_values().collect()
}

/// Ask for approval and suspend the node. Returns `false` (and writes nothing) when the node
/// already has an open request, so a restart that re-asks does not duplicate the question.
pub fn request(
    journal: &Journal,
    run: &str,
    revision: u64,
    at_ms: u64,
    node: &str,
    question: &str,
    evidence: &[String],
) -> Result<bool, String> {
    if question.trim().is_empty() {
        return Err("an approval needs a question".to_string());
    }
    let loaded = journal.read()?;
    if pending(&loaded.entries, run).iter().any(|p| p.node == node) {
        return Ok(false);
    }
    journal.append(
        run,
        revision,
        at_ms,
        Event::ApprovalRequested {
            node: node.to_string(),
            question: question.to_string(),
            evidence: evidence.to_vec(),
        },
    )?;
    journal.append(
        run,
        revision,
        at_ms,
        Event::NodeSuspended {
            node: node.to_string(),
            reason: format!("awaiting approval: {question}"),
        },
    )?;
    Ok(true)
}

/// Record a person's decision. Refused without a named approver, without an open request, or when
/// the design is no longer at the revision the request was made against. Approval completes the
/// node; denial fails it, with the reason.
#[allow(clippy::too_many_arguments)]
pub fn decide(
    journal: &Journal,
    run: &str,
    current_revision: u64,
    at_ms: u64,
    node: &str,
    approve: bool,
    by: &str,
    note: &str,
) -> Result<Decision, String> {
    if by.trim().is_empty() {
        return Err("a decision needs a named approver".to_string());
    }
    let loaded = journal.read()?;
    let open = pending(&loaded.entries, run)
        .into_iter()
        .find(|p| p.node == node)
        .ok_or_else(|| format!("no pending approval for {node}"))?;
    if open.revision != current_revision {
        return Err(format!(
            "the design changed since the request (revision {} to {current_revision}); \
             request approval again",
            open.revision
        ));
    }
    journal.append(
        run,
        current_revision,
        at_ms,
        Event::ApprovalDecided {
            node: node.to_string(),
            approve,
            by: by.to_string(),
            note: note.to_string(),
        },
    )?;
    if approve {
        journal.append(
            run,
            current_revision,
            at_ms,
            Event::NodeCompleted {
                node: node.to_string(),
                output_ref: format!("approval-{}", open.requested_seq),
            },
        )?;
        Ok(Decision::Approved)
    } else {
        journal.append(
            run,
            current_revision,
            at_ms,
            Event::NodeFailed {
                node: node.to_string(),
                error: format!("denied by {by}: {note}"),
            },
        )?;
        Ok(Decision::Denied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_journal::{fold, NodeStatus};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "eak-approval-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn ask(j: &Journal) -> bool {
        let ev = vec!["out-erc-1".to_string()];
        request(j, "r1", 4, 10, "appr", "Approve the schematic?", &ev).unwrap()
    }

    #[test]
    fn a_request_suspends_the_node_and_is_not_duplicated() {
        let d = dir();
        let j = Journal::open(&d);
        assert!(ask(&j));
        assert!(!ask(&j), "a second ask while pending writes nothing");
        let l = j.read().unwrap();
        assert_eq!(l.entries.len(), 2);
        let p = pending(&l.entries, "r1");
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].evidence, vec!["out-erc-1"]);
        assert_eq!(p[0].revision, 4);
        let st = fold(&l.entries, "r1");
        assert_eq!(st.suspended(), vec!["appr"]);
        assert!(request(&j, "r1", 4, 11, "appr2", "  ", &[]).is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn approval_completes_the_node_and_survives_a_restart() {
        let d = dir();
        let j = Journal::open(&d);
        ask(&j);
        // A restarted process sees the same pending question.
        let again = Journal::open(&d);
        assert_eq!(pending(&again.read().unwrap().entries, "r1").len(), 1);
        let r = decide(&again, "r1", 4, 20, "appr", true, "owner", "looks right").unwrap();
        assert_eq!(r, Decision::Approved);
        let l = again.read().unwrap();
        assert!(pending(&l.entries, "r1").is_empty());
        let st = fold(&l.entries, "r1");
        assert_eq!(
            st.nodes.get("appr"),
            Some(&NodeStatus::Completed {
                output_ref: "approval-1".into()
            })
        );
        assert!(st.suspended().is_empty());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn denial_fails_the_node_with_the_reason_and_the_approver() {
        let d = dir();
        let j = Journal::open(&d);
        ask(&j);
        assert_eq!(
            decide(&j, "r1", 4, 20, "appr", false, "owner", "wrong LDO").unwrap(),
            Decision::Denied
        );
        let st = fold(&j.read().unwrap().entries, "r1");
        assert_eq!(
            st.nodes.get("appr"),
            Some(&NodeStatus::Failed {
                error: "denied by owner: wrong LDO".into()
            })
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn decisions_need_a_named_approver_a_pending_request_and_an_unchanged_design() {
        let d = dir();
        let j = Journal::open(&d);
        let none = decide(&j, "r1", 4, 1, "appr", true, "owner", "").unwrap_err();
        assert!(none.contains("no pending approval"), "{none}");
        ask(&j);
        let anon = decide(&j, "r1", 4, 1, "appr", true, "  ", "").unwrap_err();
        assert!(anon.contains("named approver"), "{anon}");
        let stale = decide(&j, "r1", 5, 1, "appr", true, "owner", "").unwrap_err();
        assert!(stale.contains("request approval again"), "{stale}");
        assert_eq!(pending(&j.read().unwrap().entries, "r1").len(), 1);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn invalidating_the_node_voids_an_open_request_and_allows_a_fresh_one() {
        let d = dir();
        let j = Journal::open(&d);
        ask(&j);
        let inv = Event::NodeInvalidated {
            node: "appr".into(),
            reason: "net changed".into(),
        };
        j.append("r1", 5, 30, inv).unwrap();
        assert!(pending(&j.read().unwrap().entries, "r1").is_empty());
        assert!(decide(&j, "r1", 5, 31, "appr", true, "owner", "").is_err());
        // A fresh question can be asked at the new revision.
        let ev: Vec<String> = vec![];
        assert!(request(&j, "r1", 5, 32, "appr", "Approve again?", &ev).unwrap());
        assert_eq!(pending(&j.read().unwrap().entries, "r1")[0].revision, 5);
        let _ = std::fs::remove_dir_all(d);
    }
}
