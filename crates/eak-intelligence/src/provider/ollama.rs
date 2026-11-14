//! Ollama native chat adapter (`POST /api/chat`, non-streaming) for local models.

use super::http::Endpoint;
use super::{parse_arguments, Provider, ProviderError};
use crate::message::*;
use serde_json::{json, Value};

pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";

pub struct OllamaProvider {
    pub(crate) ep: Endpoint,
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
                        .map(|c| json!({"function": {"name": c.name, "arguments": c.arguments}}))
                        .collect();
                }
                messages.push(msg);
            }
            ChatMessage::Tool { results } => {
                for r in results {
                    messages
                        .push(json!({"role": "tool", "content": r.content, "tool_name": r.name}));
                }
            }
        }
    }
    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": false,
        "options": {"num_predict": req.max_tokens},
    });
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| {
                json!({"type": "function", "function": {
                    "name": t.name, "description": t.description, "parameters": t.input_schema
                }})
            })
            .collect();
    }
    body
}

pub fn parse_response(v: &Value) -> Result<ChatResponse, ProviderError> {
    let msg = v
        .get("message")
        .ok_or_else(|| ProviderError::Malformed("response has no message".into()))?;
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
        tool_calls.push(ToolCall {
            // Ollama has no call ids: synthesise stable per-response ids.
            id: c
                .get("id")
                .and_then(Value::as_str)
                .map(String::from)
                .unwrap_or_else(|| format!("ollama_call_{i}")),
            name: f
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ProviderError::Malformed("tool call without name".into()))?
                .into(),
            arguments: match f.get("arguments") {
                Some(Value::String(s)) => parse_arguments(s),
                Some(other) => other.clone(),
                None => json!({}),
            },
        });
    }
    let stop_reason = if !tool_calls.is_empty() {
        StopReason::ToolUse
    } else {
        match v.get("done_reason").and_then(Value::as_str) {
            Some("stop") | None => StopReason::EndTurn,
            Some("length") => StopReason::MaxTokens,
            _ => StopReason::Other,
        }
    };
    Ok(ChatResponse {
        text,
        tool_calls,
        stop_reason,
        usage: Usage {
            input_tokens: v
                .get("prompt_eval_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_tokens: v.get("eval_count").and_then(Value::as_u64).unwrap_or(0),
        },
        raw: None,
    })
}

impl Provider for OllamaProvider {
    fn label(&self) -> String {
        format!("ollama:{}", self.ep.model)
    }
    fn complete(
        &self,
        request: &ChatRequest,
        cancel: &CancelToken,
    ) -> Result<ChatResponse, ProviderError> {
        let headers = self
            .ep
            .api_key
            .iter()
            .map(|k| {
                (
                    "authorization".to_string(),
                    format!("Bearer {}", k.expose()),
                )
            })
            .collect();
        let v = self.ep.post_json(
            "/api/chat",
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
    use crate::provider::{build_provider_with, ProviderConfig, ProviderKind};

    #[test]
    fn local_chat_with_tools_without_key() {
        let t = RecordingTransport::with(vec![(
            200,
            r#"{"message":{"role":"assistant","content":"",
                 "tool_calls":[{"function":{"name":"get_netlist","arguments":{"x":1}}}]},
               "done":true,"done_reason":"stop","prompt_eval_count":9,"eval_count":2}"#,
        )]);
        let mut cfg = ProviderConfig::new(ProviderKind::Ollama);
        cfg.model = Some("qwen3".into());
        let p = build_provider_with(&cfg, t.clone()).unwrap();
        let req = ChatRequest {
            system: "s".into(),
            messages: vec![
                ChatMessage::User { text: "u".into() },
                ChatMessage::Tool {
                    results: vec![ToolResult {
                        call_id: "ollama_call_0".into(),
                        name: "get_netlist".into(),
                        content: "{}".into(),
                        is_error: false,
                    }],
                },
            ],
            tools: vec![],
            max_tokens: 50,
        };
        let r = p.complete(&req, &CancelToken::new()).unwrap();
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!(r.tool_calls[0].id, "ollama_call_0");
        assert_eq!(r.tool_calls[0].arguments, json!({"x": 1}));
        let sent = t.requests.lock().unwrap()[0].clone();
        assert_eq!(sent.url, "http://localhost:11434/api/chat");
        assert!(!sent.headers.iter().any(|(k, _)| k == "authorization"));
        let body = &t.bodies()[0];
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][2]["role"], "tool");
        assert_eq!(body["messages"][2]["tool_name"], "get_netlist");
        assert!(body.get("tools").is_none());
    }
}
