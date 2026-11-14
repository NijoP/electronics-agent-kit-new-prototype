//! Minimal Server-Sent Events line parser (the streaming format of the OpenAI, Anthropic and
//! Gemini APIs). Fed one line at a time by the transport; emits one event per blank line.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    event: Option<String>,
    data: Vec<String>,
}

impl SseParser {
    pub fn new() -> SseParser {
        SseParser::default()
    }

    /// Feed one line (without its terminator). Returns an event when `line` ends one.
    pub fn push_line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.flush();
        }
        if line.starts_with(':') {
            return None; // comment / keep-alive
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data.push(value.to_string()),
            _ => {} // id / retry are not used by model APIs
        }
        None
    }

    /// Emit a final event that was not followed by a blank line.
    pub fn flush(&mut self) -> Option<SseEvent> {
        if self.data.is_empty() && self.event.is_none() {
            return None;
        }
        let ev = SseEvent {
            event: self.event.take(),
            data: std::mem::take(&mut self.data).join("\n"),
        };
        Some(ev)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_events_comments_and_multiline_data() {
        let mut p = SseParser::new();
        let mut out = vec![];
        for l in [
            ": keep-alive",
            "event: message_start",
            "data: {\"a\":1}",
            "",
            "data:x",
            "data: y",
            "",
            "data: tail",
        ] {
            out.extend(p.push_line(l));
        }
        out.extend(p.flush());
        assert_eq!(
            out,
            vec![
                SseEvent {
                    event: Some("message_start".into()),
                    data: "{\"a\":1}".into()
                },
                SseEvent {
                    event: None,
                    data: "x\ny".into()
                },
                SseEvent {
                    event: None,
                    data: "tail".into()
                },
            ]
        );
        assert_eq!(p.flush(), None);
    }
}
