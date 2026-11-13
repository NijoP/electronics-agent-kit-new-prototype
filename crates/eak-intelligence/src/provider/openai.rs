//! OpenAI-compatible `/chat/completions` adapter (OpenAI, vLLM, LM Studio, llama.cpp server…).

use super::http::Endpoint;
use super::{parse_arguments, Provider, ProviderError};
use crate::message::*;
use serde_json::{json, Value};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

pub struct OpenAiProvider {
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

impl Provider for OpenAiProvider {
    fn label(&self) -> String {
        format!("openai-compatible:{}", self.ep.model)
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
            "/chat/completions",
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
