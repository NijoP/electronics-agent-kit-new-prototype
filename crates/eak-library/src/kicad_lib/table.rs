//! `sym-lib-table` / `fp-lib-table` reader (clean-room, from the documented s-expression form):
//!
//! ```text
//! (fp_lib_table (version 7)
//!   (lib (name "Demo") (type "KiCad") (uri "${KIPRJMOD}/Demo.pretty") (options "") (descr "")))
//! ```
//!
//! Entries are kept as written; the URI is expanded later by [`super::vars::expand`]. An entry
//! this reader cannot use is reported with the reason, never dropped.

use eak_core::sexpr::{parse, Limits, Node};
use serde::{Deserialize, Serialize};

/// Which table a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableKind {
    Symbol,
    Footprint,
}

impl TableKind {
    /// The file name KiCad gives this table.
    pub fn file_name(self) -> &'static str {
        match self {
            TableKind::Symbol => "sym-lib-table",
            TableKind::Footprint => "fp-lib-table",
        }
    }

    fn root(self) -> &'static str {
        match self {
            TableKind::Symbol => "sym_lib_table",
            TableKind::Footprint => "fp_lib_table",
        }
    }
}

/// One `(lib ...)` entry, as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableEntry {
    /// Library nickname (the `lib` part of `lib:item` references).
    pub name: String,
    /// Plugin type as written, for example `KiCad` or `Legacy`.
    pub lib_type: String,
    /// URI as written (may contain `${VAR}`).
    pub uri: String,
    pub options: String,
    pub descr: String,
    pub disabled: bool,
}

/// A parsed table: usable entries, and every entry that was not usable with the reason.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LibTable {
    pub entries: Vec<TableEntry>,
    pub problems: Vec<String>,
}

fn field(lib: &Node, key: &str) -> Option<String> {
    lib.child(key)
        .and_then(|n| n.as_list())
        .and_then(|v| v.get(1))
        .and_then(Node::text)
        .map(str::to_string)
}

/// Parse a library table of the given kind. A wrong root or a parse failure is an error; a bad
/// entry is a problem line and the rest still load.
pub fn parse_table(src: &str, kind: TableKind) -> Result<LibTable, String> {
    let root = parse(src, Limits::default()).map_err(|e| e.to_string())?;
    if root.head() != Some(kind.root()) {
        return Err(format!(
            "{}: root is {:?}, expected ({} ...)",
            kind.file_name(),
            root.head().unwrap_or(""),
            kind.root()
        ));
    }
    let mut table = LibTable::default();
    for (i, lib) in root.children("lib").enumerate() {
        let name = field(lib, "name").unwrap_or_default();
        let uri = field(lib, "uri").unwrap_or_default();
        if name.trim().is_empty() || uri.trim().is_empty() {
            table.problems.push(format!(
                "{} entry {}: missing name or uri; skipped",
                kind.file_name(),
                i + 1
            ));
            continue;
        }
        if table.entries.iter().any(|e| e.name == name) {
            table.problems.push(format!(
                "{}: library {name:?} is listed twice; the first entry is used",
                kind.file_name()
            ));
            continue;
        }
        table.entries.push(TableEntry {
            name,
            lib_type: field(lib, "type").unwrap_or_default(),
            uri,
            options: field(lib, "options").unwrap_or_default(),
            descr: field(lib, "descr").unwrap_or_default(),
            disabled: lib.child("disabled").is_some(),
        });
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_entries_and_reports_bad_ones() {
        let src = r#"(fp_lib_table (version 7)
          (lib (name "Demo")(type "KiCad")(uri "${KIPRJMOD}/Demo.pretty")(options "")(descr "demo"))
          (lib (name "Off")(type KiCad)(uri "/x/Off.pretty")(options "")(descr "")(disabled))
          (lib (name "")(type "KiCad")(uri "/x/y.pretty"))
          (lib (name "Demo")(type "KiCad")(uri "/other.pretty")))"#;
        let t = parse_table(src, TableKind::Footprint).unwrap();
        assert_eq!(t.entries.len(), 2);
        assert_eq!(t.entries[0].name, "Demo");
        assert_eq!(t.entries[0].uri, "${KIPRJMOD}/Demo.pretty");
        assert_eq!(t.entries[0].descr, "demo");
        assert!(!t.entries[0].disabled);
        assert_eq!(t.entries[1].lib_type, "KiCad");
        assert!(t.entries[1].disabled);
        assert_eq!(t.problems.len(), 2, "{:?}", t.problems);
    }

    #[test]
    fn wrong_root_is_an_error() {
        assert!(parse_table("(fp_lib_table)", TableKind::Symbol).is_err());
        assert!(parse_table("(sym_lib_table", TableKind::Symbol).is_err());
        assert!(parse_table("(sym_lib_table (version 7))", TableKind::Symbol)
            .unwrap()
            .entries
            .is_empty());
    }
}
