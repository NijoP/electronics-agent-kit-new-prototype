//! Google Gemini API adapter (`models/{model}:generateContent` and
//! `:streamGenerateContent?alt=sse`, key in the `x-goog-api-key` header).
//!
//! Model turns are replayed from the provider's own parts (`raw`), because Gemini attaches
//! `thoughtSignature`s to parts that must come back unchanged on the next request.

use super::http::Endpoint;
use super::sse::SseParser;
use super::{auth_headers, Provider, ProviderError, ProviderKind};
use crate::message::*;
use serde_json::{json, Value};

pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

pub struct GeminiProvider {
    pub(crate) ep: Endpoint,
}

/// `models/gemini-x` and `gemini-x` both name the same model.
pub fn model_path(model: &str) -> String {
    format!("/models/{}", model.trim().trim_start_matches("models/"))
}

/// Append parts as a user turn, merging with a preceding user turn so roles alternate.
fn push_user(contents: &mut Vec<Value>, parts: Vec<Value>) {
    if let Some(last) = contents.last_mut() {
        if last["role"] == "user" {
            if let Some(arr) = last["parts"].as_array_mut() {
                arr.extend(parts);
                return;
            }
        }
    }
    contents.push(json!({"role": "user", "parts": parts}));
}

pub fn build_body(req: &ChatRequest) -> Value {
    let mut contents: Vec<Value> = vec![];
    for m in &req.messages {
        match m {
            ChatMessage::User { text } => push_user(&mut contents, vec![json!({"text": text})]),
            ChatMessage::Assistant {
                text,
                tool_calls,
                raw,
            } => {
                let parts = match raw {
                    Some(Value::Array(parts)) if !parts.is_empty() => parts.clone(),
                    _ => {
                        let mut parts = vec![];
                        if !text.is_empty() {
                            parts.push(json!({"text": text}));
                        }
                        for c in tool_calls {
                            parts.push(
                                json!({"functionCall": {"name": c.name, "args": c.arguments}}),
                            );
                        }
                        parts
                    }
                };
                contents.push(json!({"role": "model", "parts": parts}));
            }
            ChatMessage::Tool { results } => push_user(
                &mut contents,
                results
                    .iter()
                    .map(|r| {
                        let key = if r.is_error { "error" } else { "output" };
                        json!({"functionResponse": {"name": r.name, "response": {key: r.content}}})
                    })
                    .collect(),
            ),
        }
    }
    let mut body = json!({
        "systemInstruction": {"parts": [{"text": req.system}]},
        "contents": contents,
        "generationConfig": {"maxOutputTokens": req.max_tokens},
    });
    if !req.tools.is_empty() {
        body["tools"] = json!([{
            "functionDeclarations": req.tools.iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                // Full JSON Schema (the `parameters` field only takes an OpenAPI subset).
                "parametersJsonSchema": t.input_schema,
            })).collect::<Vec<_>>()
        }]);
    }
    body
}

pub fn parse_response(v: &Value) -> Result<ChatResponse, ProviderError> {
    if let Some(block) = v
        .pointer("/promptFeedback/blockReason")
        .and_then(Value::as_str)
    {
        if v.pointer("/candidates/0").is_none() {
            return Ok(ChatResponse {
                text: format!("(prompt blocked by the provider: {block})"),
                tool_calls: vec![],
                stop_reason: StopReason::Refusal,
                usage: Usage::default(),
                raw: None,
            });
        }
    }
    let cand = v
        .pointer("/candidates/0")
        .ok_or_else(|| ProviderError::Malformed("no candidates in response".into()))?;
    let parts: Vec<Value> = cand
        .pointer("/content/parts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut text = String::new();
    let mut tool_calls = vec![];
    for (i, p) in parts.iter().enumerate() {
        if let Some(fc) = p.get("functionCall") {
            tool_calls.push(ToolCall {
                id: fc["id"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| format!("gemini_call_{i}")),
                name: fc["name"]
                    .as_str()
                    .ok_or_else(|| ProviderError::Malformed("functionCall without name".into()))?
                    .into(),
                arguments: fc.get("args").cloned().unwrap_or_else(|| json!({})),
            });
        } else if p["thought"] != json!(true) {
            text.push_str(p["text"].as_str().unwrap_or_default());
        }
    }
    let stop_reason = match cand["finishReason"].as_str() {
        _ if !tool_calls.is_empty() => StopReason::ToolUse,
        Some("STOP") => StopReason::EndTurn,
        Some("MAX_TOKENS") => StopReason::MaxTokens,
        Some("SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII") => {
            StopReason::Refusal
        }
        _ => StopReason::Other,
    };
    Ok(ChatResponse {
        text,
        tool_calls,
        stop_reason,
        usage: Usage {
            input_tokens: v
                .pointer("/usageMetadata/promptTokenCount")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_tokens: v
                .pointer("/usageMetadata/candidatesTokenCount")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        },
        raw: (!parts.is_empty()).then_some(Value::Array(parts)),
    })
}

/// Accumulates `streamGenerateContent` chunks. Plain text parts are concatenated; parts that
/// carry a function call, a thought or a `thoughtSignature` are kept as sent.
#[derive(Debug, Default)]
pub struct StreamState {
    parts: Vec<Value>,
    finish: Option<String>,
    usage: Option<Value>,
    block: Option<String>,
    chunks: usize,
}

fn is_plain_text(p: &Value) -> bool {
    p.get("text").is_some() && p.as_object().is_some_and(|o| o.len() == 1)
}

impl StreamState {
    pub fn on_data(
        &mut self,
        data: &str,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<(), ProviderError> {
        let v: Value = serde_json::from_str(data.trim())
            .map_err(|e| ProviderError::Malformed(format!("stream chunk: {e}")))?;
        if let Some(err) = v.get("error") {
            let status = err["code"].as_u64().unwrap_or(0) as u16;
            let msg = err["message"].as_str().unwrap_or("provider error");
            return Err(super::http::status_error(
                status,
                &json!({"error": {"message": msg}}).to_string(),
            ));
        }
        self.chunks += 1;
        if let Some(u) = v.get("usageMetadata") {
            self.usage = Some(u.clone());
        }
        if let Some(b) = v
            .pointer("/promptFeedback/blockReason")
            .and_then(Value::as_str)
        {
            self.block = Some(b.into());
        }
        let Some(cand) = v.pointer("/candidates/0") else {
            return Ok(());
        };
        if let Some(f) = cand["finishReason"].as_str() {
            self.finish = Some(f.into());
        }
        for p in cand
            .pointer("/content/parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(fc) = p.get("functionCall") {
                sink(StreamEvent::ToolCallStarted {
                    id: fc["id"].as_str().unwrap_or_default().into(),
                    name: fc["name"].as_str().unwrap_or_default().into(),
                });
            } else if let Some(t) = p["text"].as_str().filter(|t| !t.is_empty()) {
                sink(if p["thought"] == json!(true) {
                    StreamEvent::ThinkingDelta { text: t.into() }
                } else {
                    StreamEvent::TextDelta { text: t.into() }
                });
            }
            match self.parts.last_mut() {
                Some(last) if is_plain_text(p) && is_plain_text(last) => {
                    let cur = last["text"].as_str().unwrap_or_default().to_string();
                    last["text"] = Value::String(cur + p["text"].as_str().unwrap_or_default());
                }
                _ => self.parts.push(p.clone()),
            }
        }
        Ok(())
    }

    pub fn finish(self) -> Result<ChatResponse, ProviderError> {
        if self.chunks == 0 {
            return Err(ProviderError::Malformed(
                "the stream ended without any response chunk".into(),
            ));
        }
        if self.parts.is_empty() {
            if let Some(b) = self.block {
                return parse_response(&json!({"promptFeedback": {"blockReason": b}}));
            }
        }
        parse_response(&json!({
            "candidates": [{"content": {"role": "model", "parts": self.parts}, "finishReason": self.finish}],
            "usageMetadata": self.usage.unwrap_or(Value::Null),
        }))
    }
}

impl Provider for GeminiProvider {
    fn label(&self) -> String {
        format!("gemini:{}", self.ep.model)
    }
    fn complete(
        &self,
        request: &ChatRequest,
        cancel: &CancelToken,
    ) -> Result<ChatResponse, ProviderError> {
        let v = self.ep.post_json(
            &format!("{}:generateContent", model_path(&self.ep.model)),
            auth_headers(ProviderKind::Gemini, self.ep.api_key.as_ref()),
            &build_body(request),
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
        let mut state = StreamState::default();
        let mut sse = SseParser::new();
        self.ep.post_stream(
            &format!(
                "{}:streamGenerateContent?alt=sse",
                model_path(&self.ep.model)
            ),
            auth_headers(ProviderKind::Gemini, self.ep.api_key.as_ref()),
            &build_body(request),
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
    use crate::provider::{build_provider_with, ApiKey, ErrorClass, ProviderConfig};

    fn cfg() -> ProviderConfig {
        let mut c = ProviderConfig::new(ProviderKind::Gemini);
        c.model = Some("models/gemini-test".into());
        c.api_key = Some(ApiKey::new("g-key"));
        c
    }

    fn request() -> ChatRequest {
        ChatRequest {
            system: "sys".into(),
            messages: vec![ChatMessage::User { text: "go".into() }],
            tools: vec![ToolSpec {
                name: "run_erc".into(),
                description: "d".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            }],
            max_tokens: 50,
        }
    }

    /// Recorded shape of a non-streaming generateContent reply with a signed function call.
    const REPLY: &str = r#"{"candidates":[{"content":{"role":"model","parts":[
        {"text":"Running ERC."},
        {"functionCall":{"name":"run_erc","args":{"sheet":1}},"thoughtSignature":"c2ln"}]},
        "finishReason":"STOP"}],
        "usageMetadata":{"promptTokenCount":12,"candidatesTokenCount":5}}"#;

    #[test]
    fn request_wire_format_and_signed_part_replay() {
        let t = RecordingTransport::with(vec![(200, REPLY)]);
        let p = build_provider_with(&cfg(), t.clone()).unwrap();
        let r = p.complete(&request(), &CancelToken::new()).unwrap();
        assert_eq!(r.text, "Running ERC.");
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!(r.tool_calls[0].name, "run_erc");
        assert_eq!(r.tool_calls[0].arguments, json!({"sheet": 1}));
        assert_eq!(r.usage.input_tokens, 12);

        let sent = t.requests.lock().unwrap()[0].clone();
        assert_eq!(
            sent.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-test:generateContent"
        );
        assert!(sent
            .headers
            .contains(&("x-goog-api-key".into(), "g-key".into())));
        assert!(!sent.url.contains("g-key"), "key must not be in the URL");
        let body = &t.bodies()[0];
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "sys");
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 50);
        assert_eq!(
            body["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"]["type"],
            "object"
        );

        // Next turn: the model parts (with the signature) come back unchanged; the tool
        // result and a new prompt form one user turn.
        let next = build_body(&ChatRequest {
            messages: vec![
                ChatMessage::User { text: "go".into() },
                ChatMessage::Assistant {
                    text: r.text.clone(),
                    tool_calls: r.tool_calls.clone(),
                    raw: r.raw.clone(),
                },
                ChatMessage::Tool {
                    results: vec![ToolResult {
                        call_id: r.tool_calls[0].id.clone(),
                        name: "run_erc".into(),
                        content: "{\"errors\":0}".into(),
                        is_error: false,
                    }],
                },
                ChatMessage::User {
                    text: "next".into(),
                },
            ],
            ..request()
        });
        let c = next["contents"].as_array().unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c[1]["role"], "model");
        assert_eq!(c[1]["parts"][1]["thoughtSignature"], "c2ln");
        assert_eq!(c[2]["parts"][0]["functionResponse"]["name"], "run_erc");
        assert_eq!(
            c[2]["parts"][0]["functionResponse"]["response"]["output"],
            "{\"errors\":0}"
        );
        assert_eq!(c[2]["parts"][1]["text"], "next");
    }

    /// Recorded shape of a `streamGenerateContent?alt=sse` reply.
    const STREAM: &str = "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Hel\"}]}}]}\r\n\r\n\
data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"lo\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":3,\"candidatesTokenCount\":2}}\r\n\r\n";

    #[test]
    fn streaming_concatenates_text_and_reports_deltas() {
        let t = RecordingTransport::with(vec![(200, STREAM)]);
        let p = build_provider_with(&cfg(), t.clone()).unwrap();
        let mut deltas = vec![];
        let r = p
            .stream(&request(), &CancelToken::new(), &mut |e| deltas.push(e))
            .unwrap();
        assert_eq!(r.text, "Hello");
        assert_eq!(r.stop_reason, StopReason::EndTurn);
        assert_eq!(r.usage.output_tokens, 2);
        assert_eq!(r.raw.unwrap(), json!([{"text": "Hello"}]));
        assert_eq!(deltas.len(), 2);
        assert!(t.requests.lock().unwrap()[0]
            .url
            .ends_with("/models/gemini-test:streamGenerateContent?alt=sse"));
    }

    #[test]
    fn bad_key_and_blocked_prompt() {
        let t = RecordingTransport::with(vec![(
            400,
            r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT"}}"#,
        )]);
        let p = build_provider_with(&cfg(), t).unwrap();
        let e = p.complete(&request(), &CancelToken::new()).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidApiKey, "{e}");
        let r = parse_response(&json!({"promptFeedback": {"blockReason": "SAFETY"}})).unwrap();
        assert_eq!(r.stop_reason, StopReason::Refusal);
    }
}
