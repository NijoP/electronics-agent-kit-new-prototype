//! Built-in adapters. Every flag used here was read from the tool's own `--help` (versions
//! named in each file); a flag EAK has not confirmed is not used.

pub mod claude;
pub mod codex;
pub mod generic;

/// The value after `key` on the first line that starts with it (`Scope: Local config`).
pub(crate) fn line_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix(key))
        .map(str::trim)
}

/// A first message starting with `-` would be read as an option by the tool's argument parser.
pub(crate) fn check_prompt(prompt: &str) -> Result<(), String> {
    if prompt.trim_start().starts_with('-') {
        Err("the first message cannot start with '-'".into())
    } else {
        Ok(())
    }
}
