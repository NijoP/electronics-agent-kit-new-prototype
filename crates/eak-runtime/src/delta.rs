//! Reversible deltas computed by diffing the IR before and after a transaction.
//! Undo applies `before` values, redo applies `after` values — no per-command inverse
//! code that could drift out of sync.

use eak_core::Project;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Keyed collections inside EAK-IR.
const COLLECTIONS: &[&[&str]] = &[
    &["library"],
    &["schematic", "sheets"],
    &["schematic", "symbols"],
    &["schematic", "wires"],
    &["schematic", "labels"],
    &["schematic", "no_connects"],
    &["schematic", "graphics"],
    &["schematic", "buses"],
    &["schematic", "bus_entries"],
    &["pcb", "footprints"],
    &["pcb", "tracks"],
    &["pcb", "vias"],
    &["pcb", "zones"],
    &["pcb", "regions"],
    &["pcb", "texts"],
    &["pcb", "dimensions"],
    &["pcb", "cutouts"],
];
/// Whole-value sections.
const SECTIONS: &[&[&str]] = &[
    &["name"],
    &["requirements"],
    &["pcb", "board"],
    &["constraints"],
    &["manufacturing"],
    &["properties"],
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub path: Vec<String>,
    /// Entry key for collections; `None` for whole sections.
    pub key: Option<String>,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

fn get<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(v, |acc, k| acc.get(*k))
}

pub fn diff(before: &Project, after: &Project) -> Vec<Change> {
    let (b, a) = (
        serde_json::to_value(before).expect("ser"),
        serde_json::to_value(after).expect("ser"),
    );
    let mut out = vec![];
    let empty = Map::new();
    for path in COLLECTIONS {
        let bm = get(&b, path).and_then(Value::as_object).unwrap_or(&empty);
        let am = get(&a, path).and_then(Value::as_object).unwrap_or(&empty);
        for (k, bv) in bm {
            match am.get(k) {
                Some(av) if av == bv => {}
                av => out.push(Change {
                    path: path.iter().map(|s| s.to_string()).collect(),
                    key: Some(k.clone()),
                    before: Some(bv.clone()),
                    after: av.cloned(),
                }),
            }
        }
        for (k, av) in am {
            if !bm.contains_key(k) {
                out.push(Change {
                    path: path.iter().map(|s| s.to_string()).collect(),
                    key: Some(k.clone()),
                    before: None,
                    after: Some(av.clone()),
                });
            }
        }
    }
    for path in SECTIONS {
        let (bv, av) = (get(&b, path), get(&a, path));
        if bv != av {
            out.push(Change {
                path: path.iter().map(|s| s.to_string()).collect(),
                key: None,
                before: bv.cloned(),
                after: av.cloned(),
            });
        }
    }
    out
}

/// Apply the `before` (undo) or `after` (redo) side of `changes` to a project.
pub fn apply(p: &Project, changes: &[Change], use_before: bool) -> Result<Project, String> {
    let mut v = serde_json::to_value(p).map_err(|e| e.to_string())?;
    for c in changes {
        let val = if use_before { &c.before } else { &c.after };
        let mut node = &mut v;
        let (last, parents) = match &c.key {
            Some(_) => (None, &c.path[..]),
            None => (c.path.last(), &c.path[..c.path.len() - 1]),
        };
        for k in parents {
            node = node
                .get_mut(k.as_str())
                .ok_or_else(|| format!("delta path {:?} missing", c.path))?;
        }
        let obj = node
            .as_object_mut()
            .ok_or("delta parent is not an object")?;
        let key = c
            .key
            .as_deref()
            .or(last.map(String::as_str))
            .ok_or("empty delta path")?;
        match val {
            Some(x) => {
                obj.insert(key.to_string(), x.clone());
            }
            None => {
                obj.remove(key);
            }
        }
    }
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Ids touched by a set of changes (for event and evidence records).
pub fn affected_ids(changes: &[Change]) -> Vec<String> {
    changes
        .iter()
        .map(|c| c.key.clone().unwrap_or_else(|| c.path.join(".")))
        .collect()
}
