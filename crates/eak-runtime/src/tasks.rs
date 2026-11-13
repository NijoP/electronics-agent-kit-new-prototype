//! Small durable engineering task model (concepts from Tracer/Weft; EAK-native code).
//! Tasks track work; they never imply that electrical verification passed.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    NeedsApproval,
    Verifying,
    Done,
    Failed,
    Cancelled,
    UnknownOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub state: TaskState,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub input_revision: Option<u64>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskBoard {
    pub tasks: BTreeMap<String, Task>,
}

impl TaskBoard {
    pub fn load(dir: &Path) -> TaskBoard {
        std::fs::read_to_string(dir.join("tasks.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        crate::store::atomic_write(
            &dir.join("tasks.json"),
            serde_json::to_string_pretty(self).expect("ser").as_bytes(),
        )
    }

    pub fn add(&mut self, title: &str, depends_on: Vec<String>) -> Result<String, String> {
        for d in &depends_on {
            if !self.tasks.contains_key(d) {
                return Err(format!("unknown dependency {d}"));
            }
        }
        let id = eak_core::new_id("tsk");
        self.tasks.insert(
            id.clone(),
            Task {
                id: id.clone(),
                title: title.into(),
                state: TaskState::Ready,
                depends_on,
                owner: None,
                input_revision: None,
                evidence: vec![],
                note: String::new(),
            },
        );
        Ok(id)
    }

    /// Tasks whose dependencies are all Done and that are themselves Ready.
    pub fn ready(&self) -> Vec<&Task> {
        self.tasks
            .values()
            .filter(|t| {
                t.state == TaskState::Ready
                    && t.depends_on.iter().all(|d| {
                        self.tasks
                            .get(d)
                            .is_some_and(|x| x.state == TaskState::Done)
                    })
            })
            .collect()
    }

    /// Claim a ready task for an owner (atomic within this board).
    pub fn claim(&mut self, id: &str, owner: &str, revision: u64) -> Result<(), String> {
        if !self.ready().iter().any(|t| t.id == id) {
            return Err(format!("task {id} is not ready"));
        }
        let t = self.tasks.get_mut(id).expect("ready implies exists");
        t.state = TaskState::Running;
        t.owner = Some(owner.into());
        t.input_revision = Some(revision);
        Ok(())
    }

    pub fn transition(
        &mut self,
        id: &str,
        to: TaskState,
        evidence: Option<String>,
    ) -> Result<(), String> {
        let t = self
            .tasks
            .get_mut(id)
            .ok_or_else(|| format!("unknown task {id}"))?;
        use TaskState::*;
        let ok = matches!(
            (t.state, to),
            (Ready, Running | Blocked | Cancelled)
                | (
                    Running,
                    NeedsApproval
                        | Verifying
                        | Done
                        | Failed
                        | Cancelled
                        | Blocked
                        | UnknownOutcome
                )
                | (NeedsApproval, Running | Cancelled | Failed)
                | (Verifying, Done | Failed | Running)
                | (Blocked, Ready | Cancelled)
                | (UnknownOutcome, Running | Failed | Done | Cancelled)
                | (Failed, Ready)
        );
        if !ok {
            return Err(format!("invalid transition {:?} → {:?}", t.state, to));
        }
        t.state = to;
        if let Some(e) = evidence {
            t.evidence.push(e);
        }
        Ok(())
    }

    /// After a crash, anything left Running has an unknown outcome until reconciled.
    pub fn mark_interrupted(&mut self) -> usize {
        let mut n = 0;
        for t in self
            .tasks
            .values_mut()
            .filter(|t| t.state == TaskState::Running || t.state == TaskState::Verifying)
        {
            t.state = TaskState::UnknownOutcome;
            n += 1;
        }
        n
    }
}
