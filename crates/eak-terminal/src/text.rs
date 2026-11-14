//! Plain-text helpers over terminal output.

/// Remove terminal escape sequences (CSI, OSC, other ESC codes) and carriage returns that
/// only rewind the cursor, leaving the text a human would read.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\x1b' => match it.next() {
                // CSI: parameters, then one final byte in @..~
                Some('[') => {
                    for d in it.by_ref() {
                        if ('@'..='~').contains(&d) {
                            break;
                        }
                    }
                }
                // OSC / DCS / APC / PM / SOS: up to BEL or ST (ESC \)
                Some(']' | 'P' | '_' | '^' | 'X') => {
                    while let Some(d) = it.next() {
                        if d == '\x07' {
                            break;
                        }
                        if d == '\x1b' && it.peek() == Some(&'\\') {
                            it.next();
                            break;
                        }
                    }
                }
                // Charset selection and similar two-byte forms take one more char.
                Some('(' | ')' | '*' | '+' | '#' | '%') => {
                    it.next();
                }
                _ => {}
            },
            '\r' => {
                if it.peek() != Some(&'\n') {
                    // A lone CR rewinds the line; keep the line break semantics simple.
                    out.push('\n');
                }
            }
            '\x07' | '\x08' | '\x0e' | '\x0f' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Supplementary heuristic: does this last output line look like a question waiting for an
/// answer? Only used after the output has gone quiet, and always labelled as a heuristic.
pub fn looks_like_prompt(line: &str) -> bool {
    let l = line.trim_end().to_ascii_lowercase();
    const ENDINGS: [&str; 8] = [
        "(y/n)",
        "[y/n]",
        "(yes/no)",
        "[yes/no]",
        "?",
        "password:",
        "passphrase:",
        "(y/n)?",
    ];
    ENDINGS.iter().any(|e| l.ends_with(e))
        || l.contains("[y/n]")
        || l.contains("(y/n)")
        || l.contains("press enter")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_colour_osc_and_cursor_codes() {
        let raw = "\x1b]0;title\x07\x1b[1;32mok\x1b[0m done\r\n\x1b[?2004hnext";
        assert_eq!(strip_ansi(raw), "ok done\nnext");
    }

    #[test]
    fn prompt_heuristic_matches_questions_only() {
        assert!(looks_like_prompt("Overwrite file? [y/N]"));
        assert!(looks_like_prompt("Continue (y/n)"));
        assert!(looks_like_prompt("[sudo] password:"));
        assert!(!looks_like_prompt("Compiling eak-core v0.1.4"));
        assert!(!looks_like_prompt("$ "));
    }
}
