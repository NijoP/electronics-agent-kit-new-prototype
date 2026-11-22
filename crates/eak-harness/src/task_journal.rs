//! Append-only, revision-bound task journal (ADR-0037).
//!
//! One JSON object per line in `<project dir>/agent-journal.jsonl`. Every entry records the run,
//! the project revision it was written against, a monotonically increasing sequence number and a
//! SHA-256 chain over the previous entry, so an edit, deletion or reordering is detected on read.
//! Nothing is ever rewritten; the only repair is dropping an incomplete final line left by a crash
//! (an uncommitted record). Run state is rebuilt by folding the entries, so it survives a restart.
//!
//! External side effects use an idempotency key: `EffectPlanned` is written before the effect and
//! `EffectCommitted` after it. After a crash a planned-but-uncommitted key is *never* replayed
//! blindly; the gate reports `NeedsReconcile` so the caller must check the outside world first.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const JOURNAL_FILE: &str = "agent-journal.jsonl";
const GENESIS: &str = "genesis";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    RunStarted {
        graph_hash: String,
        base_revision: u64,
    },
    NodeKicked {
        node: String,
    },
    NodeStarted {
        node: String,
    },
    NodeCompleted {
        node: String,
        output_ref: String,
    },
    NodeFailed {
        node: String,
        error: String,
    },
    NodeSkipped {
        node: String,
        reason: String,
    },
    NodeSuspended {
        node: String,
        reason: String,
    },
    NodeResumed {
        node: String,
    },
    NodeCancelled {
        node: String,
    },
    /// The node's earlier result was discarded because something it depends on changed.
    NodeInvalidated {
        node: String,
        reason: String,
    },
    /// A person was asked a question; the node waits (see `task_approval`).
    ApprovalRequested {
        node: String,
        question: String,
        evidence: Vec<String>,
    },
    /// A named person answered; `approve` is their decision.
    ApprovalDecided {
        node: String,
        approve: bool,
        by: String,
        note: String,
    },
    EffectPlanned {
        node: String,
        key: String,
        action: String,
    },
    EffectCommitted {
        key: String,
        result: String,
    },
    RunCompleted,
    RunFailed {
        error: String,
    },
    RunCancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub at_ms: u64,
    pub run: String,
    /// Project revision the writer saw when it wrote this entry.
    pub revision: u64,
    pub event: Event,
    pub prev: String,
    pub hash: String,
}

#[derive(Serialize)]
struct Body<'a> {
    seq: u64,
    at_ms: u64,
    run: &'a str,
    revision: u64,
    event: &'a Event,
    prev: &'a str,
}

fn digest(b: &Body) -> String {
    let bytes = serde_json::to_vec(b).unwrap_or_default();
    Sha256::digest(bytes)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// What was read from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub entries: Vec<Entry>,
    /// The file ended in an incomplete record (crash during a write). It is not in `entries`.
    pub torn_tail: bool,
    /// Byte length of the valid prefix.
    valid_len: usize,
}

/// State of an idempotency key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectState {
    Unplanned,
    /// Planned but no commit recorded: the effect may or may not have happened.
    Planned {
        action: String,
    },
    Committed {
        result: String,
    },
}

/// What a caller must do before performing an effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    /// Not seen before; the plan was just recorded, perform the effect then commit it.
    Run,
    /// Already done with this result; do not repeat it.
    Skip(String),
    /// Planned earlier with no commit (crash or timeout): reconcile with the outside world first.
    NeedsReconcile(String),
}

pub fn effect_state(entries: &[Entry], key: &str) -> EffectState {
    let mut state = EffectState::Unplanned;
    for e in entries {
        match &e.event {
            Event::EffectPlanned { key: k, action, .. } if k == key => {
                if !matches!(state, EffectState::Committed { .. }) {
                    state = EffectState::Planned {
                        action: action.clone(),
                    };
                }
            }
            Event::EffectCommitted { key: k, result } if k == key => {
                state = EffectState::Committed {
                    result: result.clone(),
                };
            }
            _ => {}
        }
    }
    state
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeStatus {
    Kicked,
    Running,
    Completed {
        output_ref: String,
    },
    Failed {
        error: String,
    },
    Skipped {
        reason: String,
    },
    Suspended {
        reason: String,
    },
    Cancelled,
    /// Result discarded; the node runs again.
    Invalidated {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    Completed,
    Failed,
    Cancelled,
}

/// A run rebuilt from its entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunState {
    pub run: String,
    pub graph_hash: Option<String>,
    pub base_revision: u64,
    /// Latest status of each node that appeared.
    pub nodes: BTreeMap<String, NodeStatus>,
    pub ended: Option<RunEnd>,
}

impl RunState {
    /// Nodes that were running when the journal stopped. After a restart these are neither done
    /// nor failed: the caller re-evaluates them (their effects are gated by idempotency keys).
    pub fn interrupted(&self) -> Vec<String> {
        if self.ended.is_some() {
            return Vec::new();
        }
        self.nodes
            .iter()
            .filter(|(_, s)| matches!(s, NodeStatus::Running))
            .map(|(n, _)| n.clone())
            .collect()
    }

    /// Nodes waiting for a human or external answer.
    pub fn suspended(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|(_, s)| matches!(s, NodeStatus::Suspended { .. }))
            .map(|(n, _)| n.clone())
            .collect()
    }

    /// Nodes that completed, for `Graph::ready`.
    pub fn completed(&self) -> std::collections::BTreeSet<String> {
        self.nodes
            .iter()
            .filter(|(_, s)| matches!(s, NodeStatus::Completed { .. }))
            .map(|(n, _)| n.clone())
            .collect()
    }
}

/// Rebuild one run's state from entries.
pub fn fold(entries: &[Entry], run: &str) -> RunState {
    let mut st = RunState {
        run: run.to_string(),
        graph_hash: None,
        base_revision: 0,
        nodes: BTreeMap::new(),
        ended: None,
    };
    for e in entries.iter().filter(|e| e.run == run) {
        match &e.event {
            Event::RunStarted {
                graph_hash,
                base_revision,
            } => {
                st.graph_hash = Some(graph_hash.clone());
                st.base_revision = *base_revision;
            }
            Event::NodeKicked { node } => {
                st.nodes.insert(node.clone(), NodeStatus::Kicked);
            }
            Event::NodeStarted { node } | Event::NodeResumed { node } => {
                st.nodes.insert(node.clone(), NodeStatus::Running);
            }
            Event::NodeCompleted { node, output_ref } => {
                st.nodes.insert(
                    node.clone(),
                    NodeStatus::Completed {
                        output_ref: output_ref.clone(),
                    },
                );
            }
            Event::NodeFailed { node, error } => {
                st.nodes.insert(
                    node.clone(),
                    NodeStatus::Failed {
                        error: error.clone(),
                    },
                );
            }
            Event::NodeSkipped { node, reason } => {
                st.nodes.insert(
                    node.clone(),
                    NodeStatus::Skipped {
                        reason: reason.clone(),
                    },
                );
            }
            Event::NodeSuspended { node, reason } => {
                st.nodes.insert(
                    node.clone(),
                    NodeStatus::Suspended {
                        reason: reason.clone(),
                    },
                );
            }
            Event::NodeCancelled { node } => {
                st.nodes.insert(node.clone(), NodeStatus::Cancelled);
            }
            Event::NodeInvalidated { node, reason } => {
                st.nodes.insert(
                    node.clone(),
                    NodeStatus::Invalidated {
                        reason: reason.clone(),
                    },
                );
            }
            Event::EffectPlanned { .. }
            | Event::EffectCommitted { .. }
            | Event::ApprovalRequested { .. }
            | Event::ApprovalDecided { .. } => {}
            Event::RunCompleted => st.ended = Some(RunEnd::Completed),
            Event::RunFailed { .. } => st.ended = Some(RunEnd::Failed),
            Event::RunCancelled => st.ended = Some(RunEnd::Cancelled),
        }
    }
    st
}

pub struct Journal {
    path: PathBuf,
}

impl Journal {
    /// The journal beside a project (`dir` is the project folder).
    pub fn open(dir: &Path) -> Journal {
        Journal {
            path: dir.join(JOURNAL_FILE),
        }
    }

    /// Read and verify the whole journal. A missing file is an empty journal. A final line that
    /// does not parse is reported as `torn_tail`; any other bad line, a sequence gap or a broken
    /// hash chain is an error naming the sequence number.
    pub fn read(&self) -> Result<Loaded, String> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Loaded {
                    entries: Vec::new(),
                    torn_tail: false,
                    valid_len: 0,
                });
            }
            Err(e) => return Err(format!("cannot read {}: {e}", self.path.display())),
        };
        let mut entries: Vec<Entry> = Vec::new();
        let mut prev = GENESIS.to_string();
        let mut offset = 0usize;
        let mut valid_len = 0usize;
        let mut torn = false;
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        for (i, line) in lines.iter().enumerate() {
            let last = i + 1 == lines.len();
            let body = line.trim_end_matches('\n');
            offset += line.len();
            if body.trim().is_empty() {
                valid_len = offset;
                continue;
            }
            let entry: Entry = match serde_json::from_str(body) {
                Ok(e) => e,
                Err(_) if last => {
                    torn = true;
                    break;
                }
                Err(e) => return Err(format!("journal line {} is not valid: {e}", i + 1)),
            };
            let want_seq = entries.len() as u64 + 1;
            if entry.seq != want_seq {
                return Err(format!(
                    "journal sequence break: expected {want_seq}, found {}",
                    entry.seq
                ));
            }
            if entry.prev != prev {
                return Err(format!("journal chain broken at seq {}", entry.seq));
            }
            let h = digest(&Body {
                seq: entry.seq,
                at_ms: entry.at_ms,
                run: &entry.run,
                revision: entry.revision,
                event: &entry.event,
                prev: &entry.prev,
            });
            if h != entry.hash {
                return Err(format!(
                    "journal entry {} was altered (hash mismatch)",
                    entry.seq
                ));
            }
            prev = entry.hash.clone();
            entries.push(entry);
            valid_len = offset;
        }
        Ok(Loaded {
            entries,
            torn_tail: torn,
            valid_len,
        })
    }

    /// Append one entry and flush it to disk. An incomplete final record left by a crash is
    /// dropped first (it was never committed); nothing else is ever modified.
    pub fn append(
        &self,
        run: &str,
        revision: u64,
        at_ms: u64,
        event: Event,
    ) -> Result<Entry, String> {
        let loaded = self.read()?;
        if loaded.torn_tail {
            let f = std::fs::OpenOptions::new()
                .write(true)
                .open(&self.path)
                .map_err(|e| format!("cannot open journal: {e}"))?;
            f.set_len(loaded.valid_len as u64)
                .map_err(|e| format!("cannot drop the torn final record: {e}"))?;
        }
        let prev = loaded
            .entries
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_else(|| GENESIS.to_string());
        let seq = loaded.entries.len() as u64 + 1;
        let hash = digest(&Body {
            seq,
            at_ms,
            run,
            revision,
            event: &event,
            prev: &prev,
        });
        let entry = Entry {
            seq,
            at_ms,
            run: run.to_string(),
            revision,
            event,
            prev,
            hash,
        };
        let mut line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
        line.push('\n');
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create journal folder: {e}"))?;
        }
        // A valid last line without a newline (hand-edited) must not be glued to the new record.
        let needs_nl = std::fs::read(&self.path)
            .map(|b| !b.is_empty() && b.last() != Some(&b'\n'))
            .unwrap_or(false);
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| format!("cannot open journal: {e}"))?;
        if needs_nl {
            f.write_all(b"\n")
                .map_err(|e| format!("cannot write journal: {e}"))?;
        }
        f.write_all(line.as_bytes())
            .map_err(|e| format!("cannot write journal: {e}"))?;
        f.sync_all()
            .map_err(|e| format!("cannot flush journal: {e}"))?;
        Ok(entry)
    }

    /// Record the plan for an external effect unless it was already planned or done. Perform the
    /// effect only when this returns `Gate::Run`, then call [`Journal::commit_effect`].
    pub fn begin_effect(
        &self,
        run: &str,
        revision: u64,
        at_ms: u64,
        node: &str,
        key: &str,
        action: &str,
    ) -> Result<Gate, String> {
        let loaded = self.read()?;
        match effect_state(&loaded.entries, key) {
            EffectState::Committed { result } => Ok(Gate::Skip(result)),
            EffectState::Planned { action: planned } => Ok(Gate::NeedsReconcile(planned)),
            EffectState::Unplanned => {
                self.append(
                    run,
                    revision,
                    at_ms,
                    Event::EffectPlanned {
                        node: node.to_string(),
                        key: key.to_string(),
                        action: action.to_string(),
                    },
                )?;
                Ok(Gate::Run)
            }
        }
    }

    pub fn commit_effect(
        &self,
        run: &str,
        revision: u64,
        at_ms: u64,
        key: &str,
        result: &str,
    ) -> Result<Entry, String> {
        self.append(
            run,
            revision,
            at_ms,
            Event::EffectCommitted {
                key: key.to_string(),
                result: result.to_string(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "eak-journal-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn started(node: &str) -> Event {
        Event::NodeStarted { node: node.into() }
    }

    #[test]
    fn entries_round_trip_with_a_verified_chain() {
        let d = dir();
        let j = Journal::open(&d);
        assert!(j.read().unwrap().entries.is_empty());
        let a = j
            .append(
                "r1",
                3,
                100,
                Event::RunStarted {
                    graph_hash: "g".into(),
                    base_revision: 3,
                },
            )
            .unwrap();
        let b = j.append("r1", 3, 101, started("sch")).unwrap();
        assert_eq!((a.seq, b.seq), (1, 2));
        assert_eq!(a.prev, "genesis");
        assert_eq!(b.prev, a.hash);
        let l = j.read().unwrap();
        assert_eq!(l.entries, vec![a, b]);
        assert!(!l.torn_tail);
        assert_eq!(l.entries[1].revision, 3);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn tampering_is_detected() {
        let d = dir();
        let j = Journal::open(&d);
        j.append("r1", 1, 1, started("a")).unwrap();
        j.append("r1", 1, 2, started("b")).unwrap();
        let p = d.join(JOURNAL_FILE);
        let text = std::fs::read_to_string(&p).unwrap();
        std::fs::write(&p, text.replacen("\"a\"", "\"z\"", 1)).unwrap();
        let e = j.read().unwrap_err();
        assert!(e.contains("altered"), "{e}");
        // Deleting the first record breaks the sequence.
        let mut lines: Vec<&str> = text.lines().collect();
        lines.remove(0);
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        assert!(j.read().unwrap_err().contains("sequence break"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_torn_final_record_is_reported_then_dropped_on_the_next_append() {
        let d = dir();
        let j = Journal::open(&d);
        j.append("r1", 1, 1, started("a")).unwrap();
        let p = d.join(JOURNAL_FILE);
        let mut text = std::fs::read_to_string(&p).unwrap();
        text.push_str("{\"seq\":2,\"at_ms\":2,\"run\":\"r1\"");
        std::fs::write(&p, text).unwrap();
        let l = j.read().unwrap();
        assert!(l.torn_tail);
        assert_eq!(l.entries.len(), 1);
        let e = j.append("r1", 1, 3, started("b")).unwrap();
        assert_eq!(e.seq, 2);
        let l = j.read().unwrap();
        assert!(!l.torn_tail);
        assert_eq!(l.entries.len(), 2);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_planned_effect_is_never_replayed_blindly() {
        let d = dir();
        let j = Journal::open(&d);
        assert_eq!(
            j.begin_effect("r1", 1, 1, "sch", "k1", "place U1").unwrap(),
            Gate::Run
        );
        // Crash here: planned, never committed. A restart must not just run it again.
        assert_eq!(
            j.begin_effect("r1", 1, 2, "sch", "k1", "place U1").unwrap(),
            Gate::NeedsReconcile("place U1".into())
        );
        j.commit_effect("r1", 2, 3, "k1", "ok").unwrap();
        assert_eq!(
            j.begin_effect("r1", 2, 4, "sch", "k1", "place U1").unwrap(),
            Gate::Skip("ok".into())
        );
        assert_eq!(
            j.begin_effect("r1", 2, 5, "sch", "k2", "place U2").unwrap(),
            Gate::Run
        );
        let l = j.read().unwrap();
        assert_eq!(
            effect_state(&l.entries, "k2"),
            EffectState::Planned {
                action: "place U2".into()
            }
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn run_state_is_rebuilt_from_entries_after_a_restart() {
        let d = dir();
        let j = Journal::open(&d);
        let start = Event::RunStarted {
            graph_hash: "gh".into(),
            base_revision: 5,
        };
        j.append("r1", 5, 1, start).unwrap();
        j.append("r1", 5, 2, started("req")).unwrap();
        let done = Event::NodeCompleted {
            node: "req".into(),
            output_ref: "o1".into(),
        };
        j.append("r1", 5, 3, done).unwrap();
        j.append("r1", 5, 4, started("sch")).unwrap();
        let wait = Event::NodeSuspended {
            node: "appr".into(),
            reason: "human".into(),
        };
        j.append("r1", 5, 5, wait).unwrap();
        j.append("r2", 9, 6, started("other")).unwrap();
        // A fresh Journal value models a restarted process.
        let st = fold(&Journal::open(&d).read().unwrap().entries, "r1");
        assert_eq!(st.graph_hash.as_deref(), Some("gh"));
        assert_eq!(st.base_revision, 5);
        assert_eq!(st.interrupted(), vec!["sch"]);
        assert_eq!(st.suspended(), vec!["appr"]);
        assert_eq!(st.completed().into_iter().collect::<Vec<_>>(), vec!["req"]);
        assert!(!st.nodes.contains_key("other"));
        j.append("r1", 5, 7, Event::RunCancelled).unwrap();
        let st = fold(&j.read().unwrap().entries, "r1");
        assert_eq!(st.ended, Some(RunEnd::Cancelled));
        assert!(st.interrupted().is_empty());
        let _ = std::fs::remove_dir_all(d);
    }
}
