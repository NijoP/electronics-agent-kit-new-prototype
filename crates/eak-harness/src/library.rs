//! Component selection tools (M5.1, M5.6, M5.7), all in the Read category.
//!
//! The agent states a role's needs as an `eak_library::selection::FamilyRequirement` and EAK
//! searches, compares and checks alternatives itself; the agent never certifies a part. Sources:
//! the built-in starter library, an optional part list the user supplied (CSV or JSON, absolute
//! path), and the LCSC slot, which is reported as not connected and never called. Nothing here
//! changes the project; a chosen part enters it only through a design command.

use eak_library::selection::{self, EvalContext, FamilyRequirement};
use eak_library::sources::local::{ListFormat, LocalListSource, MAX_FILE_BYTES};
use eak_library::sources::BuiltinSource;
use eak_library::{LcscSlot, Part, PartKey, PartSource, SourceStatus};
use eak_runtime::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

fn s(text: &str) -> Value {
    json!({"type": "string", "description": text})
}

fn obj(p: Value, req: &[&str]) -> Value {
    json!({"type": "object", "properties": p, "required": req, "additionalProperties": false})
}

fn part_ref() -> Value {
    obj(
        json!({
            "manufacturer": s("manufacturer as written"),
            "mpn": s("manufacturer part number"),
            "component_id": s("optional component definition id (project library or built-in) whose pins and pad map stand for this part"),
        }),
        &["manufacturer", "mpn"],
    )
}

const REQUIREMENT: &str = "FamilyRequirement: {role, family, search_terms[], criteria[{id, kind: electrical|mechanical|thermal|supply|cost, subject: spec (with name)|category|package|lifecycle|stock|unit_price, rule: {op: at_least|at_most, value, unit} | {op: one_of, values[]}, basis?}], quantity?, offer_max_age_s?, requirement_ids?}";

/// (name, description, input schema) of every `library_*` tool.
pub fn tool_specs() -> Vec<(&'static str, &'static str, Value)> {
    let list = s("optional absolute path of a part list the user supplied (.csv or .json)");
    vec![
        (
            "library_find_candidates",
            "Role-driven candidate search (M5.1): search every ready part source with the requirement's terms, merge by manufacturer+MPN, and evaluate every typed criterion from stated values only. A value no source states, an AI-proposed value, a disagreement between sources or an unconvertible unit is UNKNOWN; stock and price need offer_max_age_s and a fresh offer. Qualified only when every criterion passes. Lists skipped sources (LCSC: not connected).",
            obj(
                json!({"requirement": {"type": "object", "description": REQUIREMENT}, "part_list_path": list}),
                &["requirement"],
            ),
        ),
        (
            "library_compare_candidates",
            "Compare chosen parts against one requirement (M5.6): per-criterion results with evidence, pass/fail/unknown tallies per electrical, mechanical, thermal, supply and cost kind, and a rank among qualified parts only (by lowest fresh unit price when every qualified part has one).",
            obj(
                json!({
                    "requirement": {"type": "object", "description": REQUIREMENT},
                    "parts": {"type": "array", "items": part_ref(), "minItems": 1, "maxItems": 20},
                    "part_list_path": list,
                }),
                &["requirement", "parts"],
            ),
        ),
        (
            "library_check_equivalence",
            "Whether an alternative can replace an original part (M5.7): package, pinout and pad map (from the named component definitions), every stated specification of the original, and optional criteria. A differing value with no criterion deciding it is UNKNOWN (needs engineering review); Equivalent only when every aspect is shown compatible.",
            obj(
                json!({
                    "original": part_ref(),
                    "alternative": part_ref(),
                    "criteria": {"type": "array", "description": "optional Criterion list (see library_find_candidates)"},
                    "quantity": {"type": "integer", "minimum": 1},
                    "offer_max_age_s": {"type": "integer", "minimum": 0},
                    "part_list_path": list,
                }),
                &["original", "alternative"],
            ),
        ),
    ]
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartRef {
    manufacturer: String,
    mpn: String,
    #[serde(default)]
    component_id: Option<String>,
}

fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Read a user-supplied part list (bounded, absolute path, .csv or .json).
pub fn open_part_list(path: &str, now: u64) -> Result<LocalListSource, String> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err(format!("part_list_path must be absolute, got {path:?}"));
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let format = match ext.as_deref() {
        Some("csv") => ListFormat::Csv,
        Some("json") => ListFormat::Json,
        _ => return Err(format!("part_list_path must end in .csv or .json: {path}")),
    };
    let meta = std::fs::metadata(p).map_err(|e| format!("{path}: {e}"))?;
    if meta.len() > MAX_FILE_BYTES as u64 {
        return Err(format!("{path} is larger than {MAX_FILE_BYTES} bytes"));
    }
    let bytes = std::fs::read(p).map_err(|e| format!("{path}: {e}"))?;
    Ok(LocalListSource::import(
        "local-list",
        path,
        format,
        &bytes,
        Some(now),
    ))
}

struct Sources {
    builtin: BuiltinSource,
    list: Option<LocalListSource>,
    lcsc: LcscSlot,
}

impl Sources {
    fn open(args: &Value, now: u64) -> Result<Sources, String> {
        let list = match args.get("part_list_path").and_then(Value::as_str) {
            Some(p) => Some(open_part_list(p, now)?),
            None => None,
        };
        Ok(Sources {
            builtin: BuiltinSource::new(),
            list,
            lcsc: LcscSlot,
        })
    }

    fn all(&self) -> Vec<&dyn PartSource> {
        let mut v: Vec<&dyn PartSource> = vec![&self.builtin];
        if let Some(l) = &self.list {
            v.push(l);
        }
        v.push(&self.lcsc);
        v
    }

    /// Every source with its status and terms, plus the part list's import report.
    fn report(&self) -> Value {
        let sources: Vec<Value> = self
            .all()
            .iter()
            .map(|s| json!({"id": s.id(), "status": s.status_text(), "terms": s.terms()}))
            .collect();
        json!({
            "sources": sources,
            "part_list_import": self.list.as_ref().map(|l| l.report()),
        })
    }

    /// The merged part for one reference, from every ready source that has it.
    fn fetch(&self, r: &PartRef) -> Result<Part, String> {
        let key = PartKey::new(&r.manufacturer, &r.mpn)
            .map_err(|e| format!("{} {}: {e}", r.manufacturer, r.mpn))?;
        let mut records = Vec::new();
        let mut errors = Vec::new();
        for s in self.all() {
            if s.status() != SourceStatus::Ready {
                continue;
            }
            match s.fetch(&key) {
                Ok(Some(rec)) => records.push(rec),
                Ok(None) => {}
                Err(e) => errors.push(format!("{}: {e}", s.id())),
            }
        }
        let mut parts = Part::merge(records);
        if parts.is_empty() {
            let mut why = format!(
                "{} {} is not in any ready part source",
                r.manufacturer, r.mpn
            );
            if !errors.is_empty() {
                why.push_str(&format!(" ({})", errors.join("; ")));
            }
            return Err(why);
        }
        Ok(parts.remove(0))
    }
}

fn requirement(args: &Value) -> Result<FamilyRequirement, String> {
    let v = args
        .get("requirement")
        .cloned()
        .ok_or("requirement is required")?;
    serde_json::from_value(v).map_err(|e| format!("requirement: {e}"))
}

fn component(engine: &Engine, id: Option<&str>) -> Result<Option<eak_core::ComponentDef>, String> {
    let Some(id) = id else {
        return Ok(None);
    };
    engine
        .project()
        .library
        .get(id)
        .cloned()
        .or_else(|| {
            eak_core::builtin::components()
                .into_iter()
                .find(|c| c.id == id)
        })
        .map(Some)
        .ok_or_else(|| {
            format!("component {id} is in neither the project library nor the built-in library")
        })
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

fn part_refs(args: &Value) -> Result<Vec<PartRef>, String> {
    let v = args.get("parts").cloned().ok_or("parts is required")?;
    let refs: Vec<PartRef> = serde_json::from_value(v).map_err(|e| format!("parts: {e}"))?;
    if refs.is_empty() || refs.len() > 20 {
        return Err("parts must name 1 to 20 parts".into());
    }
    Ok(refs)
}

fn one_ref(args: &Value, k: &str) -> Result<PartRef, String> {
    let v = args
        .get(k)
        .cloned()
        .ok_or_else(|| format!("{k} is required"))?;
    serde_json::from_value(v).map_err(|e| format!("{k}: {e}"))
}

pub fn read(engine: &Engine, name: &str, args: &Value) -> Result<Value, String> {
    let now = now_s();
    let sources = Sources::open(args, now)?;
    let result = match name {
        "library_find_candidates" => {
            let req = requirement(args)?;
            to_json(&selection::search_candidates(&sources.all(), &req, now))?
        }
        "library_compare_candidates" => {
            let req = requirement(args)?;
            let mut parts = Vec::new();
            let mut not_found = Vec::new();
            for r in &part_refs(args)? {
                match sources.fetch(r) {
                    Ok(p) => parts.push(p),
                    Err(why) => not_found.push(why),
                }
            }
            let mut v = to_json(&selection::compare(&parts, &req, now))?;
            v["not_found"] = json!(not_found);
            v
        }
        "library_check_equivalence" => {
            let (o, a) = (one_ref(args, "original")?, one_ref(args, "alternative")?);
            let criteria: Vec<selection::Criterion> = match args.get("criteria") {
                Some(c) => {
                    serde_json::from_value(c.clone()).map_err(|e| format!("criteria: {e}"))?
                }
                None => Vec::new(),
            };
            let ctx = EvalContext {
                quantity: args
                    .get("quantity")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .max(1),
                offer_max_age_s: args.get("offer_max_age_s").and_then(Value::as_u64),
                now_unix_s: now,
            };
            let (op, ap) = (sources.fetch(&o)?, sources.fetch(&a)?);
            let od = component(engine, o.component_id.as_deref())?;
            let ad = component(engine, a.component_id.as_deref())?;
            to_json(&selection::check_equivalence(
                &op,
                &ap,
                od.as_ref(),
                ad.as_ref(),
                &criteria,
                &ctx,
            ))?
        }
        other => return Err(format!("no library tool named {other}")),
    };
    let mut out = json!({"result": result, "now_unix_s": now});
    if let (Some(m), Value::Object(r)) = (out.as_object_mut(), sources.report()) {
        m.extend(r);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_library_tool_is_named_and_has_an_object_schema() {
        let specs = tool_specs();
        assert_eq!(specs.len(), 3);
        for (name, _, schema) in specs {
            assert!(name.starts_with("library_"));
            assert_eq!(schema["type"], "object");
        }
    }

    #[test]
    fn relative_and_unknown_part_lists_are_refused() {
        assert!(open_part_list("parts.csv", 0).is_err());
        let abs = std::env::temp_dir().join("eak-parts.txt");
        assert!(open_part_list(&abs.to_string_lossy(), 0).is_err());
    }

    #[test]
    fn a_part_list_feeds_the_candidate_search_and_lcsc_is_reported_not_connected() {
        let dir = std::env::temp_dir().join(format!("eak-harness-lib-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("parts.csv");
        std::fs::write(
            &path,
            "manufacturer,mpn,description,package,spec:Vin max [V]\nMaker,LDO-1,LDO regulator,SOT-23-5,6\nMaker,LDO-2,LDO regulator,SOT-23-5,4\n",
        )
        .unwrap();
        let engine = Engine::in_memory(eak_core::Project::new("lib-tools"));
        let args = json!({
            "part_list_path": path.to_string_lossy(),
            "requirement": {
                "role": "U1", "family": "LDO", "search_terms": ["LDO regulator"],
                "criteria": [
                    {"id": "vin", "kind": "electrical", "subject": "spec", "name": "Vin max",
                     "rule": {"op": "at_least", "value": 5.5, "unit": "V"}}
                ]
            }
        });
        let v = read(&engine, "library_find_candidates", &args).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        let c = v["result"]["candidates"].as_array().unwrap();
        let verdict = |mpn: &str| {
            c.iter()
                .find(|c| c["display_mpn"] == mpn)
                .map(|c| c["verdict"].as_str().unwrap_or("").to_string())
        };
        assert_eq!(verdict("LDO-1").as_deref(), Some("qualified"));
        assert_eq!(verdict("LDO-2").as_deref(), Some("rejected"));
        // A built-in part states no Vin max, so it can only be undetermined, never qualified.
        for other in c
            .iter()
            .filter(|c| c["display_mpn"] != "LDO-1" && c["display_mpn"] != "LDO-2")
        {
            assert_eq!(other["verdict"], "undetermined", "{other}");
        }
        let lcsc = v["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == LcscSlot::ID)
            .unwrap();
        assert_eq!(lcsc["status"], "not connected");
    }
}
