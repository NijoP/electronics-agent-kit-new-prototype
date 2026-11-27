//! # eak-harness — the EAK engineering harness (ADR-0018)
//!
//! Lets external coding agents (Pi, OpenCode, Claude Code, any CLI or MCP client) work on an
//! EAK project through the same typed command engine the desktop uses. There is no second
//! design model and no direct project-file write: every mutation is an
//! `eak_runtime::Engine::execute` call with an explicit expected revision, an agent actor
//! (agent name + session id) and a standing approval bounded by the session policy.
//!
//! * `context`  – explicit project/session context (flags, `EAK_*` environment), never a global
//!   "active project".
//! * `tools`    – the typed tool registry and its dispatcher over an `Engine`; every response is
//!   one structured JSON envelope.
//! * `engineering` – unit-aware copper/power calculations and the fail-closed gates as tools.
//! * `endpoint` – the authenticated local socket the desktop app serves while it has a project
//!   open (single writer), and its client.
//! * `route`    – picks the running app's engine (socket) or a direct engine under the project
//!   lock, and invokes a tool.
//! * `mcp`      – Model Context Protocol server over stdio exposing the same tools.
//! * `proposal` – propose mode (ADR-0051): design edits collected on a copy, reviewed with their
//!   ERC/DRC delta and applied by a person as one checkpointed `Batch`.
//! * `weft`     – WEFT orchestration (ADR-0053): agent registry, delegated tasks, typed
//!   messages and review verdicts in one journal; proposal conflict detection.
//! * `cli`      – the `eak` command line.

pub mod activity;
pub mod agent_task;
pub mod cli;
pub mod context;
pub mod design;
pub mod endpoint;
pub mod engineering;
pub mod intel;
pub mod knowledge;
pub mod library;
pub mod mcp;
pub mod proposal;
pub mod rework;
pub mod route;
pub mod task_approval;
pub mod task_context;
pub mod task_graph;
pub mod task_invalidate;
pub mod task_journal;
pub mod tools;
pub mod weft;

pub use context::{CallContext, ProjectRef};
pub use tools::{envelope_error, handle, tool_specs, ErrorCode, ToolSpec};

/// Agent-facing guide, embedded so `eak guide` and the MCP `guide` tool work from any folder.
pub const GUIDE: &str = include_str!("../../../docs/AGENT_HARNESS.md");
