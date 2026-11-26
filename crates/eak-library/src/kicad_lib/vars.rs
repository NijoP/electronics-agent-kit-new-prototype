//! KiCad path variables (`${KIPRJMOD}`, `${KICAD8_3DMODEL_DIR}`, `$(VAR)` ...) and 3D model
//! references.
//!
//! A variable is expanded only from the map given; an unknown one stays unresolved and is named.
//! A model file is reported as present only when it was found on disk at the resolved path.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The result of expanding a path text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expansion {
    /// The text with every known variable replaced.
    pub text: String,
    /// Variables that were not in the map, in order of appearance (the text keeps them as
    /// written, so the path is not usable while this is non-empty).
    pub unresolved: Vec<String>,
}

/// Expand `${NAME}` and `$(NAME)` from `vars`. Text that is not a well-formed reference is kept.
pub fn expand(text: &str, vars: &BTreeMap<String, String>) -> Expansion {
    let mut out = String::with_capacity(text.len());
    let mut unresolved = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let close = match after.chars().next() {
            Some('{') => Some('}'),
            Some('(') => Some(')'),
            _ => None,
        };
        let end = close.and_then(|c| after.find(c));
        match end {
            Some(end) if end > 1 => {
                let name = &after[1..end];
                match vars.get(name) {
                    Some(v) => out.push_str(v.trim_end_matches(['/', '\\'])),
                    None => {
                        out.push_str(&rest[at..at + 2 + end]);
                        if !unresolved.iter().any(|u| u == name) {
                            unresolved.push(name.to_string());
                        }
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('$');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Expansion {
        text: out,
        unresolved,
    }
}

/// Every environment variable whose name starts with `KICAD` (for example
/// `KICAD8_3DMODEL_DIR`), for callers that want the user's KiCad setup to apply.
pub fn kicad_env_vars() -> BTreeMap<String, String> {
    std::env::vars()
        .filter(|(k, _)| k.starts_with("KICAD"))
        .collect()
}

/// 3D model file format, from the file extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "extension")]
pub enum ModelFormat {
    /// STEP (`.step` / `.stp`): exact solid geometry, used for mechanical export.
    Step,
    /// VRML (`.wrl`): mesh with colours, used for rendering only.
    Wrl,
    /// Any other extension, as written; EAK does not read it.
    Other(String),
}

impl ModelFormat {
    pub fn of(path: &str) -> ModelFormat {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "step" | "stp" => ModelFormat::Step,
            "wrl" => ModelFormat::Wrl,
            _ => ModelFormat::Other(ext),
        }
    }
}

/// A footprint's 3D model reference: what was written, where it points, and what was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRef {
    /// Path text as written in the footprint.
    pub written: String,
    /// The path after variable expansion; `None` while a variable is unresolved.
    pub resolved: Option<String>,
    pub unresolved_vars: Vec<String>,
    pub format: ModelFormat,
    /// True only when a file was found at `resolved`.
    pub found: bool,
    /// For a `.wrl` reference, the `.step` file next to it, when one was found (KiCad libraries
    /// usually ship both, and STEP is the one mechanical export needs).
    pub step_alternative: Option<String>,
    /// What the user should know, in plain words.
    pub notes: Vec<String>,
}

/// Resolve a model path written in a footprint. Relative paths resolve against `base` (the
/// footprint library folder). Only the file system decides `found`.
pub fn resolve_model(written: &str, vars: &BTreeMap<String, String>, base: &Path) -> ModelRef {
    let exp = expand(written, vars);
    let format = ModelFormat::of(written);
    let mut notes = Vec::new();
    let mut resolved = None;
    let mut found = false;
    let mut step_alternative = None;
    if exp.unresolved.is_empty() {
        let p = Path::new(&exp.text);
        let full = if p.is_absolute() {
            p.to_path_buf()
        } else {
            base.join(p)
        };
        found = full.is_file();
        if !found {
            notes.push("model file not found at the resolved path".into());
        }
        if format == ModelFormat::Wrl {
            for ext in ["step", "stp"] {
                let alt = full.with_extension(ext);
                if alt.is_file() {
                    step_alternative = Some(alt.to_string_lossy().into_owned());
                    break;
                }
            }
        }
        resolved = Some(full.to_string_lossy().into_owned());
    } else {
        notes.push(format!(
            "path variable(s) not set: {}",
            exp.unresolved.join(", ")
        ));
    }
    match &format {
        ModelFormat::Step => {}
        ModelFormat::Wrl if step_alternative.is_none() => notes
            .push("VRML model: usable for rendering only; no STEP file found next to it".into()),
        ModelFormat::Wrl => {}
        ModelFormat::Other(ext) => notes.push(format!(
            "model format {ext:?} is not read by EAK (STEP and VRML are)"
        )),
    }
    ModelRef {
        written: written.to_string(),
        resolved,
        unresolved_vars: exp.unresolved,
        format,
        found,
        step_alternative,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn expands_both_forms_and_names_unknown_variables() {
        let v = vars(&[("KIPRJMOD", "/p/"), ("KICAD8_3DMODEL_DIR", "/m")]);
        let e = expand("${KIPRJMOD}/a/$(KICAD8_3DMODEL_DIR)/b.step", &v);
        assert_eq!(e.text, "/p/a//m/b.step");
        assert!(e.unresolved.is_empty());
        let e = expand("${KICAD9_3DMODEL_DIR}/x.wrl and ${KICAD9_3DMODEL_DIR}", &v);
        assert_eq!(
            e.text,
            "${KICAD9_3DMODEL_DIR}/x.wrl and ${KICAD9_3DMODEL_DIR}"
        );
        assert_eq!(e.unresolved, vec!["KICAD9_3DMODEL_DIR".to_string()]);
        assert_eq!(expand("cost $5 ${", &v).text, "cost $5 ${");
    }

    #[test]
    fn format_comes_from_the_extension() {
        assert_eq!(ModelFormat::of("a/B.STEP"), ModelFormat::Step);
        assert_eq!(ModelFormat::of("a/b.stp"), ModelFormat::Step);
        assert_eq!(ModelFormat::of("b.wrl"), ModelFormat::Wrl);
        assert_eq!(ModelFormat::of("b.igs"), ModelFormat::Other("igs".into()));
    }

    #[test]
    fn unresolved_model_is_never_found() {
        let m = resolve_model("${NOPE}/x.step", &BTreeMap::new(), Path::new("/"));
        assert_eq!(m.resolved, None);
        assert!(!m.found);
        assert_eq!(m.unresolved_vars, vec!["NOPE".to_string()]);
        let m = resolve_model(
            "/definitely/not/here/x.wrl",
            &BTreeMap::new(),
            Path::new("/"),
        );
        assert!(!m.found);
        assert_eq!(m.format, ModelFormat::Wrl);
        assert_eq!(m.notes.len(), 2, "{:?}", m.notes);
    }
}
