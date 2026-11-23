//! # eak-runtime — application runtime
//!
//! * `command` – the typed command set and its preconditions (single mutation path).
//! * `delta`   – reversible change sets (undo/redo).
//! * `engine`  – per-project command engine: revisions, approvals, transactions, events.
//! * `store`   – project folder persistence, journal, checkpoints, crash reconciliation.
//! * `process` – project-scoped process supervisor.
//! * `tasks`   – small durable engineering task model.
//! * `index`   – SQLite metadata index (recent projects, event log); rebuildable cache.

pub mod command;
pub mod delta;
pub mod engine;
pub mod fixtures;
pub mod index;
pub mod process;
pub mod status;
pub mod store;
pub mod tasks;

pub use command::{
    sync_changes, AnnotateOrder, AnnotateScope, Command, CommandError, OperationClass, SyncChange,
};
pub use engine::{Actor, ActorKind, Approval, CommandRequest, CommandResponse, Engine, Event};
