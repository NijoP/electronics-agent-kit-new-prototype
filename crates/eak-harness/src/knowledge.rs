//! Knowledge tools: `knowledge_search`, `knowledge_cite`, `knowledge_import`.
//!
//! Retrieval is advisory (ADR-0024): every result carries `advisory`, a note that it is not a
//! validation result, and chunk-level provenance. Nothing here touches the design (no
//! `Engine::execute`, no revision change, no evidence record); `knowledge_import` writes only
//! `<project>/knowledge/index.json`.

use crate::tools::{ErrorCode, Fail};
use eak_intelligence::knowledge::{KnowledgeIndex, SourceKind, ADVISORY};
use eak_runtime::Engine;
use serde_json::{json, Value};
use std::path::Path;

/// Largest text file `knowledge_import` accepts.
const MAX_IMPORT_BYTES: u64 = 5 * 1024 * 1024;

fn fail(code: ErrorCode, msg: impl Into<String>) -> Fail {
    Fail(code, msg.into(), Value::Null)
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, Fail> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| fail(ErrorCode::InvalidArguments, format!("{key} is required")))
}

fn index(engine: &Engine) -> Result<KnowledgeIndex, Fail> {
    KnowledgeIndex::for_project(engine.dir()).map_err(|e| fail(ErrorCode::Storage, e))
}

fn kind_filter(args: &Value) -> Result<Option<SourceKind>, Fail> {
    match args.get("kind").and_then(Value::as_str) {
        None => Ok(None),
        Some("desk_rule") => Ok(Some(SourceKind::DeskRule)),
        Some("eak_doc") => Ok(Some(SourceKind::EakDoc)),
        Some("user_document") => Ok(Some(SourceKind::UserDocument)),
        Some(o) => Err(fail(
            ErrorCode::InvalidArguments,
            format!("kind {o} is not one of desk_rule, eak_doc, user_document"),
        )),
    }
}

pub(crate) fn dispatch(engine: &Engine, name: &str, args: &Value) -> Result<Value, Fail> {
    match name {
        "knowledge_search" => search(engine, args),
        "knowledge_cite" => cite(engine, args),
        "knowledge_lookup" => lookup(engine, args),
        "knowledge_import" => import(engine, args),
        other => Err(fail(
            ErrorCode::UnknownTool,
            format!("no knowledge tool named {other}"),
        )),
    }
}

fn search(engine: &Engine, args: &Value) -> Result<Value, Fail> {
    let query = arg_str(args, "query")?;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map_or(5, |n| n as usize);
    let idx = index(engine)?;
    let part = args
        .get("part")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty());
    let hits = idx.search_filtered(query, limit, kind_filter(args)?, part);
    Ok(json!({
        "advisory": ADVISORY,
        "note": "Retrieved text is advisory. It is not a validation result and does not change what eak-validation reports; cite it with knowledge_cite and verify numbers with the calc_* tools.",
        "query": query,
        "searched_chunks": idx.chunks.len(),
        "hits": hits,
        "limitations": [
            "lexical BM25 only: no synonyms or embeddings, so rephrase if nothing matches",
            "PDFs are not parsed; import extracted text (form feed = page break)",
            "an empty result means no lexical match, not that the topic is unaddressed",
        ],
    }))
}

/// Exact MPN / Distributor-id lookup: every chunk about the part, no lexical match needed.
fn lookup(engine: &Engine, args: &Value) -> Result<Value, Fail> {
    let part = arg_str(args, "part")?;
    let idx = index(engine)?;
    let chunks = idx.by_part(part);
    Ok(json!({
        "advisory": ADVISORY,
        "note": "Chunks tagged with this part number at import or mentioning this Distributor id. Advisory only; an empty list means nothing was imported for it, not that the part is unsuitable or unknown.",
        "part": part,
        "chunks": chunks,
    }))
}

fn cite(engine: &Engine, args: &Value) -> Result<Value, Fail> {
    let id = arg_str(args, "chunk_id")?;
    let c = index(engine)?.cite(id).ok_or_else(|| {
        fail(
            ErrorCode::NotFound,
            format!("no knowledge chunk {id} (ids come from knowledge_search)"),
        )
    })?;
    serde_json::to_value(&c).map_err(|e| fail(ErrorCode::Internal, e.to_string()))
}

fn import(engine: &Engine, args: &Value) -> Result<Value, Fail> {
    let dir = engine.dir().ok_or_else(|| {
        fail(
            ErrorCode::Precondition,
            "the project has no folder to hold a knowledge index",
        )
    })?;
    let rel = arg_str(args, "path")?;
    let requested = Path::new(rel);
    let full = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        dir.join(requested)
    };
    let canon = full.canonicalize().map_err(|e| {
        fail(
            ErrorCode::NotFound,
            format!("cannot open {}: {e}", full.display()),
        )
    })?;
    let root = dir
        .canonicalize()
        .map_err(|e| fail(ErrorCode::Storage, e.to_string()))?;
    if !canon.starts_with(&root) {
        return Err(fail(
            ErrorCode::InvalidArguments,
            "path must be inside the project folder; copy the document there first",
        ));
    }
    let meta = std::fs::metadata(&canon).map_err(|e| fail(ErrorCode::Storage, e.to_string()))?;
    if !meta.is_file() || meta.len() > MAX_IMPORT_BYTES {
        return Err(fail(
            ErrorCode::InvalidArguments,
            format!("{} is not a text file of at most 5 MiB", canon.display()),
        ));
    }
    let text = std::fs::read_to_string(&canon).map_err(|_| {
        fail(
            ErrorCode::InvalidArguments,
            "the file is not UTF-8 text; extract text from PDFs and datasheets before importing",
        )
    })?;
    let source = args
        .get("source")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(String::from)
        .unwrap_or_else(|| {
            canon
                .strip_prefix(&root)
                .unwrap_or(&canon)
                .to_string_lossy()
                .replace('\\', "/")
        });
    let revision = format!("project revision {}", engine.project().revision);
    let mut user = KnowledgeIndex::load_user(dir).map_err(|e| fail(ErrorCode::Storage, e))?;
    let parts: Vec<String> = args
        .get("mpn")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(|s| vec![s.to_string()])
        .unwrap_or_default();
    let summary =
        user.add_source_for_part(SourceKind::UserDocument, &source, &revision, &text, &parts);
    user.save_user(dir)
        .map_err(|e| fail(ErrorCode::Storage, e))?;
    Ok(json!({
        "advisory": ADVISORY,
        "imported": summary,
        "revision_label": revision,
        "note": "Indexed for retrieval only; the design and its revision are unchanged.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_filter_rejects_unknown_kinds() {
        assert!(kind_filter(&json!({"kind": "datasheet"})).is_err());
        assert!(matches!(
            kind_filter(&json!({"kind": "desk_rule"})),
            Ok(Some(SourceKind::DeskRule))
        ));
        assert!(matches!(kind_filter(&json!({})), Ok(None)));
    }

    #[test]
    fn query_is_required() {
        assert!(arg_str(&json!({"query": "  "}), "query").is_err());
        assert_eq!(arg_str(&json!({"query": "x"}), "query").ok(), Some("x"));
    }
}
