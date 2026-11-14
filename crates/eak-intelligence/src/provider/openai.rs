//! OpenAI-compatible `/chat/completions` adapter (OpenAI, vLLM, LM Studio, llama.cpp server…).

use super::http::Endpoint;
use super::sse::SseParser;
use super::{auth_headers, parse_arguments, Provider, ProviderError, ProviderKind};
use crate::message::*;
use serde_json::{json, Value};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

pub struct OpenAiProvider {
    pub(crate) ep: Endpoint,
}

/// Per-endpoint differences between "OpenAI-compatible" servers, detected from the base URL
/// (the same approach as Pi's `detectCompat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compat {
    /// OpenAI's own API wants `max_completion_tokens` (reasoning models reject `max_tokens`);
    /// every other compatible server accepts `max_tokens`.
    pub max_completion_tokens: bool,
    /// Send `stream_options.include_usage` (Mistral rejects unknown fields).
    pub stream_usage: bool,
}

impl Compat {
    pub fn detect(base_url: &str) -> Compat {
        Compat {
            max_completion_tokens: base_url.contains("api.openai.com"),
            stream_usage: !base_url.contains("mistral.ai"),
        }
    }
}

/// Request body for `/chat/completions`, with the compat rules of `base_url`.
pub fn build_body_for(base_url: &str, model: &str, req: &ChatRequest, stream: bool) -> Value {
    let compat = Compat::detect(base_url);
    let mut body = build_body(model, req);
    if compat.max_completion_tokens {
        if let Some(m) = body.as_object_mut().and_then(|o| o.remove("max_tokens")) {
            body["max_completion_tokens"] = m;
        }
    }
    if stream {
        body["stream"] = json!(true);
        if compat.stream_usage {
            body["stream_options"] = json!({"include_usage": true});
        }
    }
    body
}

pub fn build_body(model: &str, req: &ChatRequest) -> Value {
    let mut messages = vec![json!({"role": "system", "content": req.system})];
    for m in &req.messages {
        match m {
            ChatMessage::User { text } => messages.push(json!({"role": "user", "content": text})),
            ChatMessage::Assistant {
                text, tool_calls, ..
            } => {
                let mut msg = json!({"role": "assistant", "content": text});
                if !tool_calls.is_empty() {
                    msg["tool_calls"] = tool_calls
                        .iter()
                        .map(|c| {
                            json!({
                                "id": c.id,
                                "type": "function",
                                "function": {"name": c.name, "arguments": c.arguments.to_string()}
                            })
                        })
                        .collect();
                }
                messages.push(msg);
            }
            ChatMessage::Tool { results } => {
                for r in results {
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": r.call_id,
                        "content": r.content,
                    }));
                }
            }
        }
    }
    let mut body = json!({
        "model": model,
        "messages": messages,
        // shortcut: `max_tokens` is the field every OpenAI-compatible server accepts; switch
        // to `max_completion_tokens` per endpoint once reasoning-model support is needed.
        "max_tokens": req.max_tokens,
    });
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect();
    }
    body
}

pub fn parse_response(v: &Value) -> Result<ChatResponse, ProviderError> {
    let choice = v
        .pointer("/choices/0")
        .ok_or_else(|| ProviderError::Malformed("no choices in response".into()))?;
    let msg = choice
        .get("message")
        .ok_or_else(|| ProviderError::Malformed("choice has no message".into()))?;
    let text = msg
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut tool_calls = vec![];
    for (i, c) in msg
        .get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let f = c
            .get("function")
            .ok_or_else(|| ProviderError::Malformed("tool call without function".into()))?;
        let name = f
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::Malformed("tool call without name".into()))?;
        let arguments = match f.get("arguments") {
            Some(Value::String(s)) => parse_arguments(s),
            Some(other) => other.clone(),
            None => json!({}),
        };
        tool_calls.push(ToolCall {
            id: c
                .get("id")
                .and_then(Value::as_str)
                .map(String::from)
                .unwrap_or_else(|| format!("call_{i}")),
            name: name.into(),
            arguments,
        });
    }
    let stop_reason = match choice.get("finish_reason").and_then(Value::as_str) {
        Some("stop") => StopReason::EndTurn,
        Some("tool_calls") | Some("function_call") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        Some("content_filter") => StopReason::Refusal,
        _ if !tool_calls.is_empty() => StopReason::ToolUse,
        _ => StopReason::Other,
    };
    let usage = Usage {
        input_tokens: v
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: v
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    };
    Ok(ChatResponse {
        text,
        tool_calls,
        stop_reason,
        usage,
        raw: None,
    })
}

#[derive(Debug, Default)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
}

/// Accumulates `chat.completion.chunk` events into one [`ChatResponse`].
#[derive(Debug, Default)]
pub struct StreamState {
    text: String,
    calls: Vec<PartialCall>,
    finish: Option<String>,
    usage: Usage,
    chunks: usize,
}

impl StreamState {
    /// Apply one SSE `data:` payload.
    pub fn on_data(
        &mut self,
        data: &str,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<(), ProviderError> {
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            return Ok(());
        }
        let v: Value = serde_json::from_str(data)
            .map_err(|e| ProviderError::Malformed(format!("stream chunk: {e}")))?;
        if let Some(err) = v.get("error") {
            return Err(stream_error(err));
        }
        self.chunks += 1;
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            self.usage.input_tokens = u["prompt_tokens"].as_u64().unwrap_or(0);
            self.usage.output_tokens = u["completion_tokens"].as_u64().unwrap_or(0);
        }
        let Some(choice) = v.pointer("/choices/0") else {
            return Ok(());
        };
        let delta = &choice["delta"];
        if let Some(t) = delta["content"].as_str().filter(|t| !t.is_empty()) {
            self.text.push_str(t);
            sink(StreamEvent::TextDelta { text: t.into() });
        }
        for key in ["reasoning_content", "reasoning"] {
            if let Some(t) = delta[key].as_str().filter(|t| !t.is_empty()) {
                sink(StreamEvent::ThinkingDelta { text: t.into() });
            }
        }
        for c in delta["tool_calls"].as_array().into_iter().flatten() {
            let i = c["index"].as_u64().unwrap_or(self.calls.len() as u64) as usize;
            while self.calls.len() <= i {
                self.calls.push(PartialCall::default());
            }
            let slot = &mut self.calls[i];
            if let Some(id) = c["id"].as_str() {
                slot.id = id.into();
            }
            if let Some(n) = c.pointer("/function/name").and_then(Value::as_str) {
                let first = slot.name.is_empty();
                slot.name.push_str(n);
                if first {
                    sink(StreamEvent::ToolCallStarted {
                        id: slot.id.clone(),
                        name: slot.name.clone(),
                    });
                }
            }
            if let Some(a) = c.pointer("/function/arguments").and_then(Value::as_str) {
                slot.arguments.push_str(a);
            }
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            self.finish = Some(f.into());
        }
        Ok(())
    }

    pub fn finish(self) -> Result<ChatResponse, ProviderError> {
        if self.chunks == 0 {
            return Err(ProviderError::Malformed(
                "the stream ended without any completion chunk".into(),
            ));
        }
        let calls: Vec<Value> = self
            .calls
            .iter()
            .enumerate()
            .map(|(i, c)| {
                json!({
                    "id": if c.id.is_empty() { format!("call_{i}") } else { c.id.clone() },
                    "function": {"name": c.name, "arguments": c.arguments}
                })
            })
            .collect();
        let mut message = json!({"content": self.text});
        if !calls.is_empty() {
            message["tool_calls"] = Value::Array(calls);
        }
        parse_response(&json!({
            "choices": [{"message": message, "finish_reason": self.finish}],
            "usage": {"prompt_tokens": self.usage.input_tokens, "completion_tokens": self.usage.output_tokens},
        }))
    }
}

/// An `{"error": …}` object sent inside a stream.
pub(crate) fn stream_error(err: &Value) -> ProviderError {
    let message = err["message"]
        .as_str()
        .or_else(|| err.as_str())
        .unwrap_or("provider error")
        .to_string();
    let status = err["code"].as_u64().unwrap_or(0) as u16;
    let kind = err["type"].as_str().unwrap_or_default();
    if status == 429 || kind.contains("rate_limit") {
        ProviderError::RateLimited(message)
    } else if status >= 500 || kind.contains("overloaded") || kind.contains("server") {
        ProviderError::Server {
            status: status.max(500),
            message,
        }
    } else {
        ProviderError::Http { status, message }
    }
}

impl Provider for OpenAiProvider {
    fn label(&self) -> String {
        format!("openai-compatible:{}", self.ep.model)
    }
    fn complete(
        &self,
        request: &ChatRequest,
        cancel: &CancelToken,
    ) -> Result<ChatResponse, ProviderError> {
        let headers = auth_headers(ProviderKind::OpenAiCompatible, self.ep.api_key.as_ref());
        let body = build_body_for(&self.ep.base_url, &self.ep.model, request, false);
        let v = self
            .ep
            .post_json("/chat/completions", headers, &body, cancel)?;
        parse_response(&v)
    }
    fn stream(
        &self,
        request: &ChatRequest,
        cancel: &CancelToken,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<ChatResponse, ProviderError> {
        let headers = auth_headers(ProviderKind::OpenAiCompatible, self.ep.api_key.as_ref());
        let body = build_body_for(&self.ep.base_url, &self.ep.model, request, true);
        let mut state = StreamState::default();
        let mut sse = SseParser::new();
        self.ep.post_stream(
            "/chat/completions",
            headers,
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
    use crate::provider::ApiKey;
    use crate::provider::{build_provider_with, ProviderConfig, ProviderKind};

    fn request() -> ChatRequest {
        ChatRequest {
            system: "sys".into(),
            messages: vec![
                ChatMessage::User { text: "hi".into() },
                ChatMessage::Assistant {
                    text: "".into(),
                    tool_calls: vec![ToolCall {
                        id: "c1".into(),
                        name: "get_netlist".into(),
                        arguments: json!({}),
                    }],
                    raw: None,
                },
                ChatMessage::Tool {
                    results: vec![ToolResult {
                        call_id: "c1".into(),
                        name: "get_netlist".into(),
                        content: "{}".into(),
                        is_error: false,
                    }],
                },
            ],
            tools: vec![ToolSpec {
                name: "get_netlist".into(),
                description: "d".into(),
                input_schema: json!({"type": "object"}),
            }],
            max_tokens: 100,
        }
    }

    #[test]
    fn request_and_response_round_trip_over_recording_transport() {
        let t = RecordingTransport::with(vec![(
            200,
            r#"{"choices":[{"finish_reason":"tool_calls","message":{"content":null,
               "tool_calls":[{"id":"x","type":"function","function":{"name":"run_erc","arguments":"{\"a\":1}"}}]}}],
               "usage":{"prompt_tokens":5,"completion_tokens":7}}"#,
        )]);
        let mut cfg = ProviderConfig::new(ProviderKind::OpenAiCompatible);
        cfg.model = Some("gpt-test".into());
        cfg.api_key = Some(ApiKey::new("sk-test"));
        let p = build_provider_with(&cfg, t.clone()).unwrap();
        let r = p.complete(&request(), &CancelToken::new()).unwrap();
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!(r.tool_calls[0].name, "run_erc");
        assert_eq!(r.tool_calls[0].arguments, json!({"a": 1}));
        assert_eq!(r.usage.output_tokens, 7);

        let sent = t.requests.lock().unwrap()[0].clone();
        assert_eq!(sent.url, "https://api.openai.com/v1/chat/completions");
        assert!(sent
            .headers
            .contains(&("authorization".into(), "Bearer sk-test".into())));
        let body = &t.bodies()[0];
        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["arguments"],
            "{}"
        );
        assert_eq!(body["messages"][3]["role"], "tool");
        assert_eq!(body["messages"][3]["tool_call_id"], "c1");
        assert_eq!(body["tools"][0]["function"]["name"], "get_netlist");
    }

    #[test]
    fn bad_argument_json_is_preserved_as_string() {
        let r = parse_response(&json!({"choices":[{"message":{"tool_calls":[
            {"id":"a","function":{"name":"t","arguments":"{oops"}}]}}]}))
        .unwrap();
        assert_eq!(r.tool_calls[0].arguments, json!("{oops"));
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert!(parse_response(&json!({})).is_err());
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    use crate::provider::http::testing::RecordingTransport;
    use crate::provider::{build_provider_with, ApiKey, ProviderConfig, ProviderKind};

    /// Recorded shape of an OpenAI chat-completions stream with text and a split tool call.
    const FIXTURE: &str = "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"}}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Run\"}}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ning ERC.\"}}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_9\",\"type\":\"function\",\"function\":{\"name\":\"run_erc\",\"arguments\":\"\"}}]}}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"sheet\\\":\"}}]}}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"1}\"}}]}}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: {\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":6}}\n\n\
data: [DONE]\n\n";

    fn req() -> ChatRequest {
        ChatRequest {
            system: "s".into(),
            messages: vec![ChatMessage::User { text: "u".into() }],
            tools: vec![],
            max_tokens: 64,
        }
    }

    #[test]
    fn streams_text_and_assembles_split_tool_call_arguments() {
        let t = RecordingTransport::with(vec![(200, FIXTURE)]);
        let mut cfg = ProviderConfig::new(ProviderKind::OpenAiCompatible);
        cfg.model = Some("gpt-x".into());
        cfg.api_key = Some(ApiKey::new("sk-s"));
        cfg.extra_headers = vec![("OpenAI-Organization".into(), "org-1".into())];
        let p = build_provider_with(&cfg, t.clone()).unwrap();
        let mut events = vec![];
        let r = p
            .stream(&req(), &CancelToken::new(), &mut |e| events.push(e))
            .unwrap();
        assert_eq!(r.text, "Running ERC.");
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!(r.tool_calls[0].id, "call_9");
        assert_eq!(r.tool_calls[0].arguments, json!({"sheet": 1}));
        assert_eq!(r.usage.input_tokens, 11);
        assert_eq!(
            events,
            vec![
                StreamEvent::TextDelta { text: "Run".into() },
                StreamEvent::TextDelta {
                    text: "ning ERC.".into()
                },
                StreamEvent::ToolCallStarted {
                    id: "call_9".into(),
                    name: "run_erc".into()
                },
            ]
        );
        let sent = t.requests.lock().unwrap()[0].clone();
        assert!(sent
            .headers
            .contains(&("OpenAI-Organization".into(), "org-1".into())));
        let body = &t.bodies()[0];
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        // OpenAI's own endpoint takes max_completion_tokens.
        assert_eq!(body["max_completion_tokens"], 64);
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn compat_rules_follow_the_endpoint() {
        let b = build_body_for("https://api.mistral.ai/v1", "m", &req(), true);
        assert_eq!(b["max_tokens"], 64);
        assert!(b.get("stream_options").is_none());
        let b = build_body_for("https://api.groq.com/openai/v1", "m", &req(), false);
        assert_eq!(b["max_tokens"], 64);
        assert!(b.get("stream").is_none());
    }

    #[test]
    fn mid_stream_errors_and_empty_streams_are_failures() {
        let mut s = StreamState::default();
        let e = s
            .on_data(
                r#"{"error":{"message":"slow down","type":"rate_limit_error"}}"#,
                &mut |_| {},
            )
            .unwrap_err();
        assert!(matches!(e, ProviderError::RateLimited(_)));
        assert!(matches!(
            StreamState::default().finish(),
            Err(ProviderError::Malformed(_))
        ));
    }
}
