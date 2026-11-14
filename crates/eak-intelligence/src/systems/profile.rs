//! Product architecture: product -> subsystem -> board -> functional block -> component ->
//! PCB region. Every node carries its own domain tags; a node's *effective* domains add every
//! ancestor's tags, marked inherited, so a sensor block on a product with a battery still sees
//! the battery domain's inherited constraints but not its local rules.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Product,
    Subsystem,
    Board,
    FunctionalBlock,
    Component,
    PcbRegion,
}

/// One domain classification of one node, with the signal that produced it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DomainTag {
    pub domain: String,
    /// Why: the classifier reason, the spec field, or the interface kind.
    pub reasons: Vec<String>,
    /// Set when the tag was copied from an ancestor by `effective_domains`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherited_from: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileNode {
    pub id: String,
    pub level: Level,
    pub name: String,
    pub parent: Option<String>,
    pub domains: Vec<DomainTag>,
    /// Design objects this node stands for (net names, refdes), for traceability.
    pub objects: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProductModel {
    pub nodes: Vec<ProfileNode>,
}

impl ProductModel {
    pub fn node(&self, id: &str) -> Option<&ProfileNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Insert a node (a repeated id is ignored: the first definition wins).
    pub fn add(&mut self, n: ProfileNode) {
        if self.node(&n.id).is_none() {
            self.nodes.push(n);
        }
    }

    /// Add a domain tag to a node, merging reasons when the domain is already present.
    pub fn tag(&mut self, id: &str, domain: &str, reason: String) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
            match n.domains.iter_mut().find(|t| t.domain == domain) {
                Some(t) => {
                    if !t.reasons.contains(&reason) {
                        t.reasons.push(reason)
                    }
                }
                None => n.domains.push(DomainTag {
                    domain: domain.into(),
                    reasons: vec![reason],
                    inherited_from: None,
                }),
            }
        }
    }

    /// Ancestors of `id`, nearest first. Stops on a cycle or a dangling parent.
    pub fn ancestors(&self, id: &str) -> Vec<&ProfileNode> {
        let mut out: Vec<&ProfileNode> = vec![];
        let mut cur = self.node(id).and_then(|n| n.parent.as_deref());
        while let Some(p) = cur {
            match self.node(p) {
                Some(n) if n.id != id && !out.iter().any(|x| x.id == n.id) => {
                    out.push(n);
                    cur = n.parent.as_deref();
                }
                _ => break,
            }
        }
        out
    }

    /// True when `anc` is `id` or one of its ancestors (constraint scope inheritance).
    pub fn is_within(&self, id: &str, anc: &str) -> bool {
        id == anc || self.ancestors(id).iter().any(|n| n.id == anc)
    }

    /// Descendants of `id` (any depth).
    pub fn descendants(&self, id: &str) -> Vec<&ProfileNode> {
        self.nodes
            .iter()
            .filter(|n| n.id != id && self.is_within(&n.id, id))
            .collect()
    }

    /// Own tags plus every ancestor's tags (marked inherited; an own tag wins on a repeat).
    pub fn effective_domains(&self, id: &str) -> Vec<DomainTag> {
        let Some(n) = self.node(id) else {
            return vec![];
        };
        let mut out = n.domains.clone();
        for a in self.ancestors(id) {
            for t in &a.domains {
                if !out.iter().any(|x| x.domain == t.domain) {
                    out.push(DomainTag {
                        domain: t.domain.clone(),
                        reasons: t.reasons.clone(),
                        inherited_from: Some(a.id.clone()),
                    });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, level: Level, parent: Option<&str>) -> ProfileNode {
        ProfileNode {
            id: id.into(),
            level,
            name: id.into(),
            parent: parent.map(Into::into),
            domains: vec![],
            objects: vec![],
        }
    }

    // Written, not run (code-reviewed only).
    #[test]
    fn inheritance_marks_source_and_stops_on_cycle() {
        let mut m = ProductModel::default();
        m.add(node("product", Level::Product, None));
        m.add(node("board", Level::Board, Some("product")));
        m.add(node("blk", Level::FunctionalBlock, Some("board")));
        m.tag("product", "battery", "spec".into());
        m.tag("blk", "analog", "interface adc".into());
        let eff = m.effective_domains("blk");
        assert_eq!(eff.len(), 2);
        assert_eq!(eff[1].inherited_from.as_deref(), Some("product"));
        assert!(m.is_within("blk", "product"));
        assert_eq!(m.descendants("product").len(), 2);
        m.add(node("a", Level::Board, Some("b")));
        m.add(node("b", Level::Board, Some("a")));
        assert_eq!(m.ancestors("a").len(), 1);
    }
}
