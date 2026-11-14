//! Local engineering-knowledge retrieval: a small, pure-Rust BM25 index with chunk-level
//! provenance. No embeddings, no network, no external RAG platform (see
//! `docs/engineering/KNOWLEDGE_RETRIEVAL.md`).
//!
//! **Retrieved knowledge is advisory.** A hit is text a human or agent may cite; it is never a
//! validation result, never evidence for a gate, and never changes the design. Verification stays
//! in `eak-validation`, numbers in `eak-engineering`.
//!
//! Sources: built-in DESK-derived rules (`mental`) and EAK engineering docs, plus text the user
//! imports (datasheet text, notes, project documents). PDFs are not parsed here: import text
//! extracted from them; a form feed (`\x0c`) starts a new page, and markdown headings start
//! sections, so citations can carry a page or section locator.
//!
//! The index is small by design (thousands of chunks). Scoring recomputes document frequencies
//! per query; there is no on-disk inverted index to corrupt or migrate. The persisted form is a
//! JSON list of chunks.

use crate::mental;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// File (inside `<project>/knowledge/`) holding user-imported chunks.
pub const INDEX_DIR: &str = "knowledge";
pub const INDEX_FILE: &str = "index.json";
const SCHEMA_VERSION: u32 = 1;
/// Target chunk size in characters; paragraphs are packed up to this.
const CHUNK_CHARS: usize = 900;
const MAX_QUERY_LIMIT: usize = 20;
/// Label attached to every hit and citation.
pub const ADVISORY: &str = "advisory_knowledge_not_a_validation_result";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Rule or procedure recovered from DESK, re-expressed in `mental`.
    DeskRule,
    /// An EAK engineering document shipped with the build.
    EakDoc,
    /// Text the user imported (datasheet text, notes, project documents).
    UserDocument,
}

/// One retrievable passage and where it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    /// Stable id: `<source hash prefix>:<ordinal>`.
    pub id: String,
    pub kind: SourceKind,
    /// Source file name or built-in label (never an absolute path of another machine).
    pub source: String,
    /// `page N, section <heading>`, `section <heading>`, or `p<ordinal>`.
    pub locator: String,
    /// Source revision label: project revision at import, or the EAK/DESK version.
    pub revision: String,
    /// SHA-256 (hex) of the whole source text this chunk came from.
    pub source_sha256: String,
    /// SHA-256 (hex) of this chunk's text.
    pub chunk_sha256: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hit {
    pub chunk: Chunk,
    pub score: f64,
    pub advisory: &'static str,
}

/// What a caller may quote: the passage plus its provenance.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Citation {
    pub chunk_id: String,
    pub kind: SourceKind,
    pub source: String,
    pub locator: String,
    pub revision: String,
    pub source_sha256: String,
    pub chunk_sha256: String,
    pub quote: String,
    pub advisory: &'static str,
    /// Always states the limits of the citation.
    pub note: &'static str,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnowledgeIndex {
    #[serde(default)]
    pub version: u32,
    pub chunks: Vec<Chunk>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportSummary {
    pub source: String,
    pub source_sha256: String,
    pub chunks: usize,
    /// Chunks of an earlier import of the same source name that were replaced.
    pub replaced: usize,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Lowercase alphanumeric tokens. `3.3V` becomes `3`, `3`, `v`; numeric queries still match
/// because the same split is applied to the text.
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

fn locator(multi_page: bool, page_no: usize, section: &Option<String>, ord: usize) -> String {
    let base = match section {
        Some(s) => format!("section {s}"),
        None => format!("p{}", ord + 1),
    };
    if multi_page {
        format!("page {page_no}, {base}")
    } else {
        base
    }
}

/// Split text into `(locator, passage)` pairs. Pages (form feed) and markdown headings set the
/// locator; paragraphs are packed up to `CHUNK_CHARS`.
pub fn chunk_text(text: &str) -> Vec<(String, String)> {
    let multi_page = text.contains('\u{c}');
    let mut out: Vec<(String, String)> = Vec::new();
    for (pi, page) in text.split('\u{c}').enumerate() {
        let page_no = pi + 1;
        let mut section: Option<String> = None;
        let mut buf = String::new();
        let mut ord = 0usize;
        for para in page.split("\n\n") {
            let p = para.trim();
            if p.is_empty() {
                continue;
            }
            let heading = p
                .lines()
                .next()
                .filter(|l| l.starts_with('#'))
                .map(|h| h.trim_start_matches('#').trim().to_string());
            // A heading, or a paragraph that would overflow the chunk, closes the open chunk.
            if !buf.is_empty() && (heading.is_some() || buf.len() + p.len() > CHUNK_CHARS) {
                out.push((locator(multi_page, page_no, &section, ord), buf.clone()));
                ord += 1;
                buf.clear();
            }
            if heading.is_some() {
                section = heading;
            }
            if !buf.is_empty() {
                buf.push_str("\n\n");
            }
            buf.push_str(p);
        }
        if !buf.is_empty() {
            out.push((locator(multi_page, page_no, &section, ord), buf));
        }
    }
    out
}

impl KnowledgeIndex {
    pub fn new() -> Self {
        KnowledgeIndex {
            version: SCHEMA_VERSION,
            chunks: Vec::new(),
        }
    }

    /// The built-in corpus: DESK-derived rules and the EAK engineering docs shipped with the
    /// build. User imports are added with `add_source` / `load_user`.
    pub fn builtin() -> Self {
        let mut idx = Self::new();
        for m in mental::models() {
            let mut t = format!("# {} ({})\n\n", m.id, m.domain);
            for c in &m.constraints {
                let by = c.evaluated_by.unwrap_or("not evaluated by EAK");
                t.push_str(&format!("{}: {} [checked by: {by}]\n\n", c.id, c.statement));
            }
            for s in &m.procedure {
                t.push_str(&format!("{}. {}: {}\n\n", s.id, s.title, s.detail));
            }
            for g in &m.guidance {
                t.push_str(&format!("Guidance: {g}\n\n"));
            }
            idx.add_source(
                SourceKind::DeskRule,
                &format!("desk:{}", m.id),
                &format!("{} ({})", m.version, m.source),
                &t,
            );
        }
        let mut d = String::from("# Derating facts\n\n");
        for f in mental::DERATING {
            d.push_str(&format!(
                "{} {}: use at most {:.0}% of the rating ({})\n\n",
                f.refdes_prefix,
                f.quantity,
                f.max_fraction * 100.0,
                f.source
            ));
        }
        idx.add_source(SourceKind::DeskRule, "desk:derating", "mental.rs", &d);
        for (name, text) in [
            (
                "docs/engineering/ELECTRICAL_RULES.md",
                include_str!("../../../docs/engineering/ELECTRICAL_RULES.md"),
            ),
            (
                "docs/engineering/IPC_RULE_VALUES.md",
                include_str!("../../../docs/engineering/IPC_RULE_VALUES.md"),
            ),
        ] {
            idx.add_source(SourceKind::EakDoc, name, "build", text);
        }
        idx
    }

    /// Index `text` under `source`, replacing any earlier chunks of the same source name.
    pub fn add_source(
        &mut self,
        kind: SourceKind,
        source: &str,
        revision: &str,
        text: &str,
    ) -> ImportSummary {
        let before = self.chunks.len();
        self.chunks
            .retain(|c| !(c.kind == kind && c.source == source));
        let replaced = before - self.chunks.len();
        let source_sha = sha256_hex(text.as_bytes());
        // The id prefix covers the source name too, so identical text under two names stays unique.
        let id_prefix = sha256_hex(format!("{source}\n{source_sha}").as_bytes());
        let mut n = 0;
        for (i, (loc, passage)) in chunk_text(text).into_iter().enumerate() {
            self.chunks.push(Chunk {
                id: format!("{}:{i}", &id_prefix[..12]),
                kind,
                source: source.to_string(),
                locator: loc,
                revision: revision.to_string(),
                source_sha256: source_sha.clone(),
                chunk_sha256: sha256_hex(passage.as_bytes()),
                text: passage,
            });
            n += 1;
        }
        ImportSummary {
            source: source.to_string(),
            source_sha256: source_sha,
            chunks: n,
            replaced,
        }
    }

    pub fn merge(&mut self, other: KnowledgeIndex) {
        self.chunks.extend(other.chunks);
    }

    /// BM25 (k1 = 1.2, b = 0.75) over the chunk text and its source name. Deterministic: ties
    /// break on chunk id. `kind` restricts the corpus.
    pub fn search(&self, query: &str, limit: usize, kind: Option<SourceKind>) -> Vec<Hit> {
        let mut q: Vec<String> = Vec::new();
        for t in tokenize(query) {
            if !q.contains(&t) {
                q.push(t);
            }
        }
        if q.is_empty() {
            return vec![];
        }
        let docs: Vec<(&Chunk, Vec<String>)> = self
            .chunks
            .iter()
            .filter(|c| match kind {
                Some(k) => c.kind == k,
                None => true,
            })
            .map(|c| (c, tokenize(&format!("{} {}", c.source, c.text))))
            .collect();
        if docs.is_empty() {
            return vec![];
        }
        let n = docs.len() as f64;
        let avg = docs.iter().map(|(_, t)| t.len()).sum::<usize>() as f64 / n;
        let mut df: HashMap<&str, usize> = HashMap::new();
        for (_, toks) in &docs {
            for term in &q {
                if toks.iter().any(|t| t == term) {
                    *df.entry(term.as_str()).or_insert(0) += 1;
                }
            }
        }
        let (k1, b) = (1.2, 0.75);
        let mut hits: Vec<Hit> = docs
            .iter()
            .filter_map(|(c, toks)| {
                let len = toks.len() as f64;
                let mut score = 0.0;
                for term in &q {
                    let tf = toks.iter().filter(|t| *t == term).count() as f64;
                    if tf == 0.0 {
                        continue;
                    }
                    let d = *df.get(term.as_str()).unwrap_or(&0) as f64;
                    let idf = ((n - d + 0.5) / (d + 0.5) + 1.0).ln();
                    score +=
                        idf * tf * (k1 + 1.0) / (tf + k1 * (1.0 - b + b * len / avg.max(1.0)));
                }
                (score > 0.0).then(|| Hit {
                    chunk: (*c).clone(),
                    score,
                    advisory: ADVISORY,
                })
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.chunk.id.cmp(&b.chunk.id))
        });
        hits.truncate(limit.clamp(1, MAX_QUERY_LIMIT));
        hits
    }

    /// Citation for one chunk id (`None` when the id is unknown).
    pub fn cite(&self, chunk_id: &str) -> Option<Citation> {
        let c = self.chunks.iter().find(|c| c.id == chunk_id)?;
        Some(Citation {
            chunk_id: c.id.clone(),
            kind: c.kind,
            source: c.source.clone(),
            locator: c.locator.clone(),
            revision: c.revision.clone(),
            source_sha256: c.source_sha256.clone(),
            chunk_sha256: c.chunk_sha256.clone(),
            quote: c.text.clone(),
            advisory: ADVISORY,
            note: "Retrieved text only: it is not a verification result, a gate input or evidence that the design satisfies it. Check numbers with eak-engineering and rules with eak-validation.",
        })
    }

    /// Chunk counts per source, for status displays.
    pub fn sources(&self) -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for c in &self.chunks {
            *m.entry(c.source.clone()).or_insert(0) += 1;
        }
        m
    }

    /// Load the user-imported index of a project folder; a missing file is an empty index.
    pub fn load_user(project_dir: &Path) -> Result<Self, String> {
        let path = project_dir.join(INDEX_DIR).join(INDEX_FILE);
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s)
                .map_err(|e| format!("{} is not a valid knowledge index: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::new()),
            Err(e) => Err(format!("cannot read {}: {e}", path.display())),
        }
    }

    /// Write the index (temp file + rename).
    pub fn save_user(&self, project_dir: &Path) -> Result<(), String> {
        let dir = project_dir.join(INDEX_DIR);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let tmp = dir.join("index.json.tmp");
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, json).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, dir.join(INDEX_FILE))
            .map_err(|e| format!("cannot replace index: {e}"))
    }

    /// Built-in corpus plus the project's imports.
    pub fn for_project(project_dir: Option<&Path>) -> Result<Self, String> {
        let mut idx = Self::builtin();
        if let Some(d) = project_dir {
            idx.merge(Self::load_user(d)?);
        }
        Ok(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> KnowledgeIndex {
        let mut i = KnowledgeIndex::new();
        i.add_source(
            SourceKind::UserDocument,
            "ldo.txt",
            "rev 3",
            "# Dropout\n\nThe LDO dropout voltage is 300 mV at 1 A.\n\u{c}# Thermal\n\nJunction to ambient thermal resistance is 60 C/W.",
        );
        i.add_source(
            SourceKind::UserDocument,
            "buck.txt",
            "rev 3",
            "Buck converter inductor ripple current is 30 percent of load.",
        );
        i
    }

    #[test]
    fn finds_the_relevant_chunk_with_provenance() {
        let i = sample();
        let h = i.search("dropout voltage", 3, None);
        assert_eq!(h[0].chunk.source, "ldo.txt");
        assert!(h[0].chunk.locator.contains("page 1"));
        assert_eq!(h[0].advisory, ADVISORY);
        assert_eq!(
            h[0].chunk.chunk_sha256,
            sha256_hex(h[0].chunk.text.as_bytes())
        );
    }

    #[test]
    fn pages_and_sections_are_locators() {
        let i = sample();
        let h = i.search("thermal resistance", 1, None);
        assert_eq!(h[0].chunk.locator, "page 2, section Thermal");
    }

    #[test]
    fn reimport_replaces_and_unknown_terms_return_nothing() {
        let mut i = sample();
        let s = i.add_source(
            SourceKind::UserDocument,
            "ldo.txt",
            "rev 4",
            "Only new text.",
        );
        assert_eq!(s.replaced, 2);
        assert!(i.search("dropout", 3, None).is_empty());
        assert!(i.search("zzzz", 3, None).is_empty());
        assert!(i.search("  ", 3, None).is_empty());
    }

    #[test]
    fn cite_roundtrip_and_unknown_id() {
        let i = sample();
        let id = i.search("ripple", 1, None)[0].chunk.id.clone();
        let c = i.cite(&id).expect("citation");
        assert_eq!(c.source, "buck.txt");
        assert!(c.note.contains("not a verification result"));
        assert!(i.cite("nope").is_none());
    }

    #[test]
    fn builtin_corpus_is_searchable_and_labelled() {
        let b = KnowledgeIndex::builtin();
        assert!(!b.search("derating", 5, Some(SourceKind::DeskRule)).is_empty());
        assert!(
            b.search("IPC-2221 ampacity", 3, Some(SourceKind::EakDoc))[0]
                .chunk
                .source
                .starts_with("docs/")
        );
    }

    #[test]
    fn search_is_deterministic() {
        let b = KnowledgeIndex::builtin();
        assert_eq!(
            b.search("clearance", 8, None),
            b.search("clearance", 8, None)
        );
    }
}
