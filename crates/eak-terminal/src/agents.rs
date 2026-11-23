// Copyright the original upstream authors. Licensed under the Apache License, Version 2.0.
// Derived from third-party source; see third_party/licenses/THIRD_PARTY_NOTICES.md for the
// licence, notices and the pinned upstream commit.
// Modified by EAK: reduced to the name/alias table and lookup normalisation, returning EAK's
// canonical label strings instead of an enum.

//! Coding-agent CLI identification by executable name, including common aliases and
//! platform launcher suffixes (`.exe`, `.cmd`, `.bat`, `.ps1`, `.js`).

// Foreground command lines are read only on Linux and Windows (proc_info::agent), so elsewhere
// this table is exercised by its tests alone; it stays compiled everywhere for one behaviour.
#![cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]

/// The canonical agent label for a program name or path, if it is a known agent CLI.
pub fn canonical(name: &str) -> Option<&'static str> {
    let name = lookup_name(basename(name));
    Some(match name.as_str() {
        "pi" => "pi",
        "claude" | "claude-code" => "claude",
        "codex" => "codex",
        "gemini" => "gemini",
        "cursor" | "cursor-agent" => "cursor-agent",
        "devin" | "devin-cli" => "devin",
        "agy" | "antigravity" | "antigravity-cli" => "agy",
        "cline" | ".cline" => "cline",
        "omp" => "omp",
        "mastracode" | "mastra-code" => "mastracode",
        "opencode" | "opencode2" | "open-code" => "opencode",
        "copilot" | "github-copilot" | "ghcs" => "copilot",
        "kimi" | "kimi-code" => "kimi",
        "kiro" | "kiro-cli" => "kiro",
        "droid" => "droid",
        "amp" | "amp-local" => "amp",
        "grok" | "grok-build" => "grok",
        "hermes" | "hermes-agent" => "hermes",
        "kilo" | "kilo-code" => "kilo",
        "qodercli" | "qoderclicn" | "qoder" | "qodercn" => "qodercli",
        "qwen" | "qwen-code" => "qwen",
        "letta" | "letta-code" => "letta",
        "maki" => "maki",
        "muse" | "muse-code" | "muse-cli" => "muse",
        // A versioned launcher target such as `muse-bin-0.1.0`; a digit must follow.
        n if n
            .strip_prefix("muse-bin-")
            .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit())) =>
        {
            "muse"
        }
        _ => return None,
    })
}

fn lookup_name(name: &str) -> String {
    let mut name = name.trim().to_lowercase();
    for suffix in [".exe", ".cmd", ".bat", ".ps1", ".js"] {
        if name.ends_with(suffix) {
            name.truncate(name.len() - suffix.len());
            break;
        }
    }
    name
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .find(|c| !c.is_empty())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::canonical;

    #[test]
    fn aliases_paths_and_launcher_suffixes_map_to_one_label() {
        assert_eq!(canonical("/usr/bin/claude-code"), Some("claude"));
        assert_eq!(canonical(r"C:\tools\opencode.EXE"), Some("opencode"));
        assert_eq!(canonical("cursor-agent.cmd"), Some("cursor-agent"));
        assert_eq!(canonical("muse-bin-0.1.0-R7"), Some("muse"));
        assert_eq!(canonical("muse-binary"), None);
        assert_eq!(canonical("bash"), None);
    }
}
