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

/// Accumulates Messages-API stream events (`message_start`, `content_block_*`,
/// `message_delta`, `error`) into the same content blocks a non-streaming reply has, so the
/// result (including signed thinking blocks) replays verbatim on the next turn.
#[derive(Debug, Default)]
pub struct StreamState {
    blocks: Vec<Value>,
    /// Partial `input_json_delta` text per tool_use block index.
    json: Vec<String>,
    stop_reason: Option<String>,
    usage: Usage,
    started: bool,
}

impl StreamState {
    pub fn on_data(
        &mut self,
        data: &str,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<(), ProviderError> {
        let v: Value = serde_json::from_str(data.trim())
            .map_err(|e| ProviderError::Malformed(format!("stream event: {e}")))?;
        match v["type"].as_str().unwrap_or_default() {
            "message_start" => {
                self.started = true;
                self.usage.input_tokens = v
                    .pointer("/message/usage/input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
            }
            "content_block_start" => {
                let i = v["index"].as_u64().unwrap_or(self.blocks.len() as u64) as usize;
                let block = v["content_block"].clone();
                while self.blocks.len() <= i {
                    self.blocks.push(Value::Null);
                    self.json.push(String::new());
                }
                if block["type"] == "tool_use" {
                    sink(StreamEvent::ToolCallStarted {
                        id: block["id"].as_str().unwrap_or_default().into(),
                        name: block["name"].as_str().unwrap_or_default().into(),
                    });
                }
                self.blocks[i] = block;
            }
            "content_block_delta" => {
                let i = v["index"].as_u64().unwrap_or(0) as usize;
                let d = &v["delta"];
                let Some(block) = self.blocks.get_mut(i) else {
                    return Err(ProviderError::Malformed(
                        "delta for a block that was never started".into(),
                    ));
                };
                let append = |b: &mut Value, field: &str, s: &str| {
                    let cur = b[field].as_str().unwrap_or_default().to_string();
                    b[field] = Value::String(cur + s);
                };
                match d["type"].as_str().unwrap_or_default() {
                    "text_delta" => {
                        let t = d["text"].as_str().unwrap_or_default();
                        append(block, "text", t);
                        sink(StreamEvent::TextDelta { text: t.into() });
                    }
                    "thinking_delta" => {
                        let t = d["thinking"].as_str().unwrap_or_default();
                        append(block, "thinking", t);
                        sink(StreamEvent::ThinkingDelta { text: t.into() });
                    }
                    "signature_delta" => {
                        block["signature"] = d["signature"].clone();
                    }
                    "input_json_delta" => {
                        self.json[i].push_str(d["partial_json"].as_str().unwrap_or_default());
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let i = v["index"].as_u64().unwrap_or(0) as usize;
                if let Some(block) = self.blocks.get_mut(i) {
                    if block["type"] == "tool_use" {
                        let raw = self.json[i].trim();
                        block["input"] = if raw.is_empty() {
                            json!({})
                        } else {
                            super::parse_arguments(raw)
                        };
                    }
                }
            }
            "message_delta" => {
                if let Some(r) = v.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop_reason = Some(r.into());
                }
                if let Some(o) = v.pointer("/usage/output_tokens").and_then(Value::as_u64) {
                    self.usage.output_tokens = o;
                }
            }
            "error" => return Err(super::openai::stream_error(&v["error"])),
            _ => {} // ping, message_stop
        }
        Ok(())
    }

    pub fn finish(self) -> Result<ChatResponse, ProviderError> {
        if !self.started {
            return Err(ProviderError::Malformed(
                "the stream ended before message_start".into(),
            ));
        }
        let blocks: Vec<Value> = self.blocks.into_iter().filter(|b| !b.is_null()).collect();
        parse_response(&json!({
            "content": blocks,
            "stop_reason": self.stop_reason,
            "usage": {"input_tokens": self.usage.input_tokens, "output_tokens": self.usage.output_tokens},
        }))
    }
}

impl AnthropicProvider {
    fn headers(&self) -> Result<Vec<(String, String)>, ProviderError> {
        let key =
            self.ep.api_key.as_ref().ok_or_else(|| {
                ProviderError::MissingApiKey("Anthropic requires an API key".into())
            })?;
        Ok(super::auth_headers(
            super::ProviderKind::Anthropic,
            Some(key),
        ))
    }
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
        let v = self.ep.post_json(
            "/v1/messages",
            self.headers()?,
            &build_body(&self.ep.model, request),
            cancel,
        )?;
        parse_response(&v)
    }
    fn stream(
        &self,
        request: &ChatRequest,
        cancel: &CancelToken,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<ChatResponse, ProviderError> {
        let mut body = build_body(&self.ep.model, request);
        body["stream"] = json!(true);
        let mut state = StreamState::default();
        let mut sse = super::sse::SseParser::new();
        self.ep.post_stream(
            "/v1/messages",
            self.headers()?,
            &body,
            cancel,
            &mut |line| match sse.push_line(line) {
                Some(ev) => state.on_data(&ev.data, sink),
                None => Ok(()),
            },
        )?;
        if let Some(ev) = sse.flush() {
            state.on_data(&ev.data, sink)?;
        }
        state.finish()
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

#[cfg(test)]
mod stream_tests {
    use super::*;
    use crate::provider::http::testing::RecordingTransport;
    use crate::provider::{build_provider_with, ApiKey, ProviderConfig, ProviderKind};

    /// Recorded shape of a Messages API stream: signed thinking, text, a tool_use whose
    /// input arrives as partial JSON, and the final stop reason / usage.
    const FIXTURE: &str = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[],"usage":{"input_tokens":21,"output_tokens":1}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Need ERC."}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig-abc"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: ping
data: {"type":"ping"}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Checking"}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":" now."}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}

event: content_block_start
data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"run_erc","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"sheet\":"}}

event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":" 2}"}}

event: content_block_stop
data: {"type":"content_block_stop","index":2}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":40}}

event: message_stop
data: {"type":"message_stop"}

"#;

    #[test]
    fn streams_thinking_text_and_tool_use_and_keeps_signed_blocks() {
        let t = RecordingTransport::with(vec![(200, FIXTURE)]);
        let mut cfg = ProviderConfig::new(ProviderKind::Anthropic);
        cfg.api_key = Some(ApiKey::new("ak"));
        let p = build_provider_with(&cfg, t.clone()).unwrap();
        let mut events = vec![];
        let r = p
            .stream(
                &ChatRequest {
                    system: "s".into(),
                    messages: vec![ChatMessage::User { text: "u".into() }],
                    tools: vec![],
                    max_tokens: 100,
                },
                &CancelToken::new(),
                &mut |e| events.push(e),
            )
            .unwrap();
        assert_eq!(r.text, "Checking now.");
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!(r.tool_calls[0].arguments, json!({"sheet": 2}));
        assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (21, 40));
        let raw = r.raw.unwrap();
        assert_eq!(raw[0]["type"], "thinking");
        assert_eq!(raw[0]["thinking"], "Need ERC.");
        assert_eq!(raw[0]["signature"], "sig-abc");
        assert_eq!(raw[2]["input"], json!({"sheet": 2}));
        assert_eq!(events.len(), 4, "{events:?}");
        assert_eq!(t.bodies()[0]["stream"], true);
    }

    #[test]
    fn overloaded_error_event_is_provider_unavailable() {
        let mut s = StreamState::default();
        let e = s
            .on_data(
                r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
                &mut |_| {},
            )
            .unwrap_err();
        assert_eq!(e.class(), crate::provider::ErrorClass::ProviderUnavailable);
        assert!(StreamState::default().finish().is_err());
    }
}
