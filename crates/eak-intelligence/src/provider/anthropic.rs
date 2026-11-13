//! Anthropic Messages API adapter (`POST /v1/messages`, raw HTTP: there is no official Rust SDK).

use super::http::Endpoint;
use super::{Provider, ProviderError};
use crate::message::*;
use serde_json::{json, Value};

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
pub const API_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    pub(crate) ep: Endpoint,
}

/// Append content blocks as a user turn, merging with a preceding user turn so roles
/// always alternate (tool results followed by a new prompt form one user message).
fn push_user(messages: &mut Vec<Value>, blocks: Vec<Value>) {
    if let Some(last) = messages.last_mut() {
        if last["role"] == "user" {
            if let Some(arr) = last["content"].as_array_mut() {
                arr.extend(blocks);
                return;
            }
        }
    }
    messages.push(json!({"role": "user", "content": blocks}));
}

pub fn build_body(model: &str, req: &ChatRequest) -> Value {
    let mut messages: Vec<Value> = vec![];
    for m in &req.messages {
        match m {
            ChatMessage::User { text } => {
                push_user(&mut messages, vec![json!({"type": "text", "text": text})])
            }
            ChatMessage::Assistant {
                text,
                tool_calls,
                raw,
            } => {
                // Replay provider-native content unchanged (keeps thinking blocks valid).
                let content = match raw {
                    Some(Value::Array(blocks)) => Value::Array(blocks.clone()),
                    _ => {
                        let mut blocks = vec![];
                        if !text.is_empty() {
                            blocks.push(json!({"type": "text", "text": text}));
                        }
                        for c in tool_calls {
                            blocks.push(json!({
                                "type": "tool_use", "id": c.id, "name": c.name, "input": c.arguments
                            }));
                        }
                        Value::Array(blocks)
                    }
                };
                messages.push(json!({"role": "assistant", "content": content}));
            }
            ChatMessage::Tool { results } => push_user(
                &mut messages,
                results
                    .iter()
                    .map(|r| {
                        json!({
                            "type": "tool_result",
                            "tool_use_id": r.call_id,
                            "content": r.content,
                            "is_error": r.is_error,
                        })
                    })
                    .collect(),
            ),
        }
    }
    let mut body = json!({
        "model": model,
        "max_tokens": req.max_tokens,
        "system": req.system,
        "messages": messages,
    });
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema}))
            .collect();
    }
    body
}

pub fn parse_response(v: &Value) -> Result<ChatResponse, ProviderError> {
    let blocks = v
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::Malformed("response has no content array".into()))?;
    let mut text = String::new();
    let mut tool_calls = vec![];
    for b in blocks {
        match b.get("type").and_then(Value::as_str) {
            Some("text") => text.push_str(b.get("text").and_then(Value::as_str).unwrap_or("")),
            Some("tool_use") => tool_calls.push(ToolCall {
                id: b
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ProviderError::Malformed("tool_use without id".into()))?
                    .into(),
                name: b
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ProviderError::Malformed("tool_use without name".into()))?
                    .into(),
                arguments: b.get("input").cloned().unwrap_or_else(|| json!({})),
            }),
            // Thinking and other blocks are kept only through `raw` for verbatim replay.
            _ => {}
        }
    }
    let stop_reason = match v.get("stop_reason").and_then(Value::as_str) {
        Some("end_turn") | Some("stop_sequence") => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens") => StopReason::MaxTokens,
        Some("refusal") => StopReason::Refusal,
        _ => StopReason::Other,
    };
    Ok(ChatResponse {
        text,
        tool_calls,
        stop_reason,
        usage: Usage {
            input_tokens: v
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_tokens: v
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        },
        raw: Some(Value::Array(blocks.clone())),
    })
}

impl Provider for AnthropicProvider {
    fn label(&self) -> String {
        format!("anthropic:{}", self.ep.model)
    }
    fn complete(
        &self,
        request: &ChatRequest,
        cancel: &CancelToken,
    ) -> Result<ChatResponse, ProviderError> {
        let key =
            self.ep.api_key.as_ref().ok_or_else(|| {
                ProviderError::MissingApiKey("Anthropic requires an API key".into())
            })?;
        let headers = vec![
            ("x-api-key".to_string(), key.expose().to_string()),
            ("anthropic-version".to_string(), API_VERSION.to_string()),
        ];
        let v = self.ep.post_json(
            "/v1/messages",
            headers,
            &build_body(&self.ep.model, request),
            cancel,
        )?;
        parse_response(&v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::http::testing::RecordingTransport;
    use crate::provider::{build_provider_with, ApiKey, ProviderConfig, ProviderKind};

    #[test]
    fn tool_loop_wire_format_and_thinking_replay() {
        let t = RecordingTransport::with(vec![(
            200,
            r#"{"content":[{"type":"thinking","thinking":"","signature":"sig"},
                {"type":"text","text":"Checking."},
                {"type":"tool_use","id":"tu_1","name":"run_erc","input":{}}],
               "stop_reason":"tool_use","usage":{"input_tokens":3,"output_tokens":4}}"#,
        )]);
        let mut cfg = ProviderConfig::new(ProviderKind::Anthropic);
        cfg.api_key = Some(ApiKey::new("ak-test"));
        let p = build_provider_with(&cfg, t.clone()).unwrap();
        let req = ChatRequest {
            system: "sys".into(),
            messages: vec![ChatMessage::User { text: "go".into() }],
            tools: vec![ToolSpec {
                name: "run_erc".into(),
                description: "d".into(),
                input_schema: json!({"type":"object"}),
            }],
            max_tokens: 1000,
        };
        let r = p.complete(&req, &CancelToken::new()).unwrap();
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!(r.text, "Checking.");
        assert_eq!(r.tool_calls[0].id, "tu_1");

        let sent = t.requests.lock().unwrap()[0].clone();
        assert_eq!(sent.url, "https://api.anthropic.com/v1/messages");
        assert!(sent
            .headers
            .contains(&("x-api-key".into(), "ak-test".into())));
        assert!(sent
            .headers
            .contains(&("anthropic-version".into(), API_VERSION.into())));
        let body = &t.bodies()[0];
        assert_eq!(body["model"], DEFAULT_MODEL);
        assert_eq!(body["system"], "sys");
        assert_eq!(body["tools"][0]["input_schema"]["type"], "object");

        // Next turn: assistant content replayed verbatim; tool result and a new prompt are
        // merged into a single user turn.
        let next = build_body(
            "m",
            &ChatRequest {
                messages: vec![
                    ChatMessage::User { text: "go".into() },
                    ChatMessage::Assistant {
                        text: r.text.clone(),
                        tool_calls: r.tool_calls.clone(),
                        raw: r.raw.clone(),
                    },
                    ChatMessage::Tool {
                        results: vec![ToolResult {
                            call_id: "tu_1".into(),
                            name: "run_erc".into(),
                            content: "{}".into(),
                            is_error: false,
                        }],
                    },
                    ChatMessage::User {
                        text: "and now?".into(),
                    },
                ],
                ..req
            },
        );
        let msgs = next["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[1]["content"][0]["type"], "thinking");
        assert_eq!(msgs[1]["content"][0]["signature"], "sig");
        assert_eq!(msgs[2]["role"], "user");
        assert_eq!(msgs[2]["content"][0]["type"], "tool_result");
        assert_eq!(msgs[2]["content"][0]["tool_use_id"], "tu_1");
        assert_eq!(msgs[2]["content"][1]["text"], "and now?");
    }

    #[test]
    fn refusal_and_missing_key() {
        let r = parse_response(&json!({"content":[], "stop_reason":"refusal"})).unwrap();
        assert_eq!(r.stop_reason, StopReason::Refusal);
        let cfg = ProviderConfig {
            api_key_env: Some("EAK_TEST_UNSET_KEY_VAR_7F3A".into()),
            ..ProviderConfig::new(ProviderKind::Anthropic)
        };
        assert!(matches!(
            build_provider_with(&cfg, RecordingTransport::with(vec![])),
            Err(ProviderError::MissingApiKey(_))
        ));
    }
}
