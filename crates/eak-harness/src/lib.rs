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
//! * `cli`      – the `eak` command line.

pub mod cli;
pub mod context;
pub mod design;
pub mod endpoint;
pub mod engineering;
pub mod knowledge;
pub mod mcp;
pub mod rework;
pub mod route;
pub mod tools;

pub use context::{CallContext, ProjectRef};
pub use tools::{envelope_error, handle, tool_specs, ErrorCode, ToolSpec};

/// Agent-facing guide, embedded so `eak guide` and the MCP `guide` tool work from any folder.
pub const GUIDE: &str = include_str!("../../../docs/AGENT_HARNESS.md");
