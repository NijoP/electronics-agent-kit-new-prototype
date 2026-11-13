//! # eak-intelligence — the EAK Intelligence Engine
//!
//! One lead agent that plans and proposes, and applies changes only through the shared
//! typed command engine under the user's approval (Execution Instructions §5, §10, §11;
//! Spec §5.1, §6, AG-01..AG-03). It cannot bypass the command API or certify design
//! correctness: verification is `eak-validation`, numbers are `eak-engineering`.
//!
//! * `message`  – provider-neutral chat types and the cancellation token.
//! * `provider` – provider trait; OpenAI-compatible, Anthropic Messages and Ollama adapters;
//!   API keys from user configuration or the environment only.
//! * `tools`    – typed tools generated from `eak_runtime::Command`, plus read-only tools
//!   (design summary, netlist, ERC/DRC reports, calculators).
//! * `agent`    – bounded, cancellable agent loop with Manual/Assisted/Autonomous modes
//!   and approvals.
//! * `mock`     – scripted provider for tests (no network).

pub mod agent;
pub mod message;
pub mod mock;
pub mod provider;
pub mod tools;

pub use agent::{
    ActionRecord, Agent, AgentConfig, AgentEvent, AgentMode, AgentOutcome, AgentRun,
    ApprovalRequest, Approver, DenyAll, PlannedStep,
};
pub use message::{CancelToken, ChatMessage, ChatRequest, ChatResponse, ToolCall, ToolSpec};
pub use provider::{build_provider, Provider, ProviderConfig, ProviderError, ProviderKind};
