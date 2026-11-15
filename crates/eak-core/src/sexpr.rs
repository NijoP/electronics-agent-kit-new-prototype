//! Bounded S-expression reader for library interchange files (ADR-0035, Milestone 3 groundwork).
//!
//! Several EDA interchange formats (for example KiCad's `.kicad_sym` and `.kicad_mod`) are text
//! S-expressions: `(name atom "quoted string" (child ...))`. This reader is an independent
//! implementation of that generic syntax. It is safe for untrusted input: input size, nesting
//! depth and node count are bounded, nothing is executed, and errors carry a line number.
//! It performs no format-specific interpretation; importers sit on top and must report every
//! element they do not understand (no silent data loss).

/// Limits applied to every parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_bytes: 8 * 1024 * 1024, max_depth: 64, max_nodes: 2_000_000 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// Unquoted token (identifier, number, keyword).
    Atom(String),
    /// Quoted string with escapes resolved.
    Str(String),
    List(Vec<Node>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("input is {0} bytes, over the {1} byte limit")]
    TooLarge(usize, usize),
    #[error("line {0}: nesting deeper than {1}")]
    TooDeep(usize, usize),
    #[error("more than {0} nodes")]
    TooManyNodes(usize),
    #[error("line {0}: unexpected ')'")]
    UnbalancedClose(usize),
    #[error("line {0}: list opened here is never closed")]
    Unclosed(usize),
    #[error("line {0}: unterminated string")]
    UnterminatedString(usize),
    #[error("input contains no expression")]
    Empty,
    #[error("line {0}: text after the top-level expression")]
    Trailing(usize),
}

impl Node {
    pub fn as_list(&self) -> Option<&[Node]> {
        match self {
            Node::List(v) => Some(v),
            _ => None,
        }
    }

    /// Text of an atom or string.
    pub fn text(&self) -> Option<&str> {
        match self {
            Node::Atom(s) | Node::Str(s) => Some(s),
            Node::List(_) => None,
        }
    }

    /// For a list whose first element is an atom, that head name.
    pub fn head(&self) -> Option<&str> {
        match self {
            Node::List(v) => match v.first() {
                Some(Node::Atom(s)) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    /// Child lists whose head is `name`, in order.
    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.as_list()
            .unwrap_or(&[])
            .iter()
            .filter(move |c| c.head() == Some(name))
    }

    /// First child list with head `name`.
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children(name).next()
    }
}

/// Parse exactly one top-level expression.
pub fn parse(src: &str, limits: Limits) -> Result<Node, ParseError> {
    if src.len() > limits.max_bytes {
        return Err(ParseError::TooLarge(src.len(), limits.max_bytes));
    }
    let b = src.as_bytes();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut nodes = 0usize;
    // Stack of (open-line, items).
    let mut stack: Vec<(usize, Vec<Node>)> = Vec::new();
    let mut root: Option<Node> = None;

    macro_rules! push {
        ($n:expr) => {{
            nodes += 1;
            if nodes > limits.max_nodes {
                return Err(ParseError::TooManyNodes(limits.max_nodes));
            }
            let n: Node = $n;
            match stack.last_mut() {
                Some((_, items)) => items.push(n),
                None => {
                    if root.is_some() {
                        return Err(ParseError::Trailing(line));
                    }
                    root = Some(n);
                }
            }
        }};
    }

    while i < b.len() {
        let c = b[i];
        match c {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b' ' | b'\t' | b'\r' => i += 1,
            b'(' => {
                if stack.len() >= limits.max_depth {
                    return Err(ParseError::TooDeep(line, limits.max_depth));
                }
                if stack.is_empty() && root.is_some() {
                    return Err(ParseError::Trailing(line));
                }
                stack.push((line, Vec::new()));
                i += 1;
            }
            b')' => {
                let (_, items) = stack.pop().ok_or(ParseError::UnbalancedClose(line))?;
                push!(Node::List(items));
                i += 1;
            }
            b'"' => {
                let start_line = line;
                i += 1;
                let mut out: Vec<u8> = Vec::new();
                loop {
                    if i >= b.len() {
                        return Err(ParseError::UnterminatedString(start_line));
                    }
                    match b[i] {
                        b'"' => {
                            i += 1;
                            break;
                        }
                        b'\\' if i + 1 < b.len() => {
                            let e = b[i + 1];
                            out.push(match e {
                                b'n' => b'\n',
                                b't' => b'\t',
                                b'r' => b'\r',
                                other => other,
                            });
                            i += 2;
                        }
                        b'\n' => {
                            line += 1;
                            out.push(b'\n');
                            i += 1;
                        }
                        other => {
                            out.push(other);
                            i += 1;
                        }
                    }
                }
                push!(Node::Str(String::from_utf8_lossy(&out).into_owned()));
            }
            _ => {
                let start = i;
                while i < b.len()
                    && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'(' | b')' | b'"')
                {
                    i += 1;
                }
                push!(Node::Atom(src[start..i].to_string()));
            }
        }
    }
    if let Some((l, _)) = stack.last() {
        return Err(ParseError::Unclosed(*l));
    }
    root.ok_or(ParseError::Empty)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "(kicad_symbol_lib (version 20231120)\n  (symbol \"R\" (property \"Reference\" \"R\")\n    (pin passive line (at 0 0 0) (number \"1\"))))";

    #[test]
    fn parses_nested_lists_atoms_and_strings() {
        let n = parse(SAMPLE, Limits::default()).unwrap();
        assert_eq!(n.head(), Some("kicad_symbol_lib"));
        let sym = n.child("symbol").unwrap();
        assert_eq!(sym.as_list().unwrap()[1].text(), Some("R"));
        let pin = sym.child("pin").unwrap();
        assert_eq!(pin.child("number").unwrap().as_list().unwrap()[1].text(), Some("1"));
        assert_eq!(n.children("symbol").count(), 1);
    }

    #[test]
    fn string_escapes_resolve() {
        let n = parse(r#"(a "x\"y\\z\nw")"#, Limits::default()).unwrap();
        assert_eq!(n.as_list().unwrap()[1].text(), Some("x\"y\\z\nw"));
    }

    #[test]
    fn rejects_malformed_input_with_line_numbers() {
        let l = Limits::default();
        assert_eq!(parse("", l), Err(ParseError::Empty));
        assert_eq!(parse("(a (b)", l), Err(ParseError::Unclosed(1)));
        assert_eq!(parse("(a))", l), Err(ParseError::UnbalancedClose(1)));
        assert_eq!(parse("(a)\n(b)", l), Err(ParseError::Trailing(2)));
        assert_eq!(parse("(a\n\"open", l), Err(ParseError::UnterminatedString(2)));
    }

    #[test]
    fn limits_bound_size_depth_and_node_count() {
        let small = Limits { max_bytes: 4, ..Limits::default() };
        assert!(matches!(parse("(abcdef)", small), Err(ParseError::TooLarge(8, 4))));
        let shallow = Limits { max_depth: 2, ..Limits::default() };
        assert!(matches!(parse("(((a)))", shallow), Err(ParseError::TooDeep(1, 2))));
        let few = Limits { max_nodes: 3, ..Limits::default() };
        assert!(matches!(parse("(a b c d)", few), Err(ParseError::TooManyNodes(3))));
        // Hostile deep nesting is cut off long before the stack could overflow.
        let deep = "(".repeat(100_000);
        assert!(matches!(parse(&deep, Limits::default()), Err(ParseError::TooDeep(..))));
    }
}
