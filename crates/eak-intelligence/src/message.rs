//! Provider-neutral chat model. Every provider adapter translates to and from these types,
//! so the agent loop never depends on one vendor's wire format.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// One tool the model may call. `input_schema` is a JSON Schema object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// A tool invocation requested by the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider call id (synthesised when the provider has none, e.g. Ollama).
    pub id: String,
    pub name: String,
    /// Parsed arguments. A provider that returned unparseable JSON yields a string here,
    /// which the tool layer rejects as invalid arguments.
    pub arguments: Value,
}

/// Result of executing one tool call, returned to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ChatMessage {
    User {
        text: String,
    },
    Assistant {
        text: String,
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
        /// Provider-native content to replay verbatim (Anthropic content blocks, including
        /// thinking blocks, which must be passed back unchanged).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw: Option<Value>,
    },
    /// All tool results for one assistant turn, kept together (providers that support
    /// parallel tool calls expect them in a single message).
    Tool {
        results: Vec<ToolResult>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub system: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    /// The provider's safety system declined the request.
    Refusal,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: StopReason,
    #[serde(default)]
    pub usage: Usage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
}

impl ChatResponse {
    /// Convenience constructor (mock scripts, tests).
    pub fn text(text: &str) -> ChatResponse {
        ChatResponse {
            text: text.into(),
            tool_calls: vec![],
            stop_reason: StopReason::EndTurn,
            usage: Usage::default(),
            raw: None,
        }
    }
    pub fn tool_calls(text: &str, calls: Vec<ToolCall>) -> ChatResponse {
        ChatResponse {
            text: text.into(),
            tool_calls: calls,
            stop_reason: StopReason::ToolUse,
            usage: Usage::default(),
            raw: None,
        }
    }
}

/// Incremental output while a provider streams its reply. The final [`ChatResponse`] is
/// still returned at the end; these events only let a UI show progress as it arrives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Visible answer text.
    TextDelta { text: String },
    /// Reasoning/thinking text (shown only as progress, never as the answer).
    ThinkingDelta { text: String },
    /// The model started a tool call.
    ToolCallStarted { id: String, name: String },
}

/// Cooperative cancellation shared between the caller (UI "Cancel" button) and the agent
/// loop, provider transport and tool dispatch.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> CancelToken {
        CancelToken::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
