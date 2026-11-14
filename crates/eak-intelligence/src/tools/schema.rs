//! Small JSON Schema builders for tool input schemas, mirroring the serde shape of the
//! EAK-IR types used by commands.

use serde_json::{json, Map, Value};

pub fn obj(props: &[(&str, Value)], required: &[&str]) -> Value {
    let mut p = Map::new();
    for (k, v) in props {
        p.insert((*k).to_string(), v.clone());
    }
    json!({
        "type": "object",
        "properties": p,
        "required": required,
        "additionalProperties": false,
    })
}

pub fn string(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}
pub fn boolean(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}
pub fn number(desc: &str) -> Value {
    json!({"type": "number", "description": desc})
}
pub fn integer(desc: &str) -> Value {
    json!({"type": "integer", "description": desc})
}
/// A length in integer nanometres (EAK-IR's canonical unit).
pub fn nm(desc: &str) -> Value {
    json!({"type": "integer", "description": format!("{desc} (integer nanometres; 1 mm = 1000000)")})
}
pub fn enumeration(values: &[&str], desc: &str) -> Value {
    json!({"type": "string", "enum": values, "description": desc})
}
pub fn array(items: Value, desc: &str) -> Value {
    json!({"type": "array", "items": items, "description": desc})
}
/// `Option<T>`: the field may be omitted or null.
pub fn nullable(v: Value) -> Value {
    json!({"anyOf": [v, {"type": "null"}]})
}

pub fn point(desc: &str) -> Value {
    let mut p = obj(&[("x", nm("X")), ("y", nm("Y"))], &["x", "y"]);
    p["description"] = Value::String(desc.into());
    p
}
pub fn points(desc: &str) -> Value {
    array(point("vertex"), desc)
}
pub fn rotation() -> Value {
    enumeration(&["R0", "R90", "R180", "R270"], "quarter-turn rotation")
}
pub fn label_kind() -> Value {
    enumeration(
        &["net", "power"],
        "net = ordinary label; power = power port that drives the net",
    )
}
pub fn copper_layer() -> Value {
    json!({
        "description": "copper layer: \"top\", \"bottom\", or {\"inner\": n} for the n-th inner layer from the top (1-based)",
        "anyOf": [
            {"type": "string", "enum": ["top", "bottom"]},
            obj(&[("inner", integer("inner layer number, 1 = just below the top layer"))], &["inner"]),
        ],
    })
}
pub fn side() -> Value {
    enumeration(&["top", "bottom"], "board side")
}

pub fn stackup() -> Value {
    let layer = obj(
        &[
            ("layer", copper_layer()),
            (
                "kind",
                enumeration(
                    &["signal", "plane"],
                    "signal routing layer or power/ground plane",
                ),
            ),
            ("copper_oz", number("copper weight, oz/ft²")),
            (
                "dielectric_after_um",
                integer("dielectric thickness below this layer, µm (0 for the bottom layer)"),
            ),
            ("er", number("relative permittivity of that dielectric")),
        ],
        &["layer", "copper_oz", "dielectric_after_um", "er"],
    );
    obj(
        &[
            (
                "layers",
                array(
                    layer,
                    "copper layers top to bottom: top, inner 1..n, bottom; an even count from 2 to 16",
                ),
            ),
            ("board_thickness", nm("finished board thickness")),
            ("dielectric", string("dielectric material, e.g. FR-4")),
        ],
        &["layers", "board_thickness", "dielectric"],
    )
}

pub fn net_class() -> Value {
    obj(
        &[
            ("name", string("class name")),
            ("track_width", nm("track width")),
            ("clearance", nm("clearance")),
            ("via_drill", nm("via drill")),
            ("via_diameter", nm("via pad diameter (> drill)")),
            ("nets", array(string("net name"), "member nets")),
        ],
        &[
            "name",
            "track_width",
            "clearance",
            "via_drill",
            "via_diameter",
        ],
    )
}

pub fn manufacturing() -> Value {
    obj(
        &[
            ("fabricator_profile", string("fabricator profile id")),
            ("surface_finish", string("e.g. HASL, ENIG")),
            ("solder_mask_color", string("solder mask colour")),
            (
                "mask_expansion",
                nm("solder mask expansion around pads (≥ 0)"),
            ),
        ],
        &[
            "fabricator_profile",
            "surface_finish",
            "solder_mask_color",
            "mask_expansion",
        ],
    )
}

/// Schema of a `FootprintDef`.
pub fn footprint_def() -> Value {
    let pad = obj(
        &[
            ("number", string("pad number")),
            ("kind", enumeration(&["smd", "tht"], "pad kind")),
            (
                "shape",
                enumeration(&["rect", "circle", "oval"], "pad shape"),
            ),
            ("position", point("pad centre relative to footprint origin")),
            ("size_x", nm("pad width")),
            ("size_y", nm("pad height")),
            ("drill", nullable(nm("finished hole diameter for THT pads"))),
        ],
        &["number", "kind", "shape", "position", "size_x", "size_y"],
    );
    obj(
        &[
            ("name", string("footprint name")),
            ("pads", array(pad, "pads")),
            ("courtyard_half_w", nm("courtyard half width")),
            ("courtyard_half_h", nm("courtyard half height")),
            (
                "body_height",
                nullable(nm("body height above board; null = unknown")),
            ),
        ],
        &["name", "pads", "courtyard_half_w", "courtyard_half_h"],
    )
}

/// Schema of a symbol-pin -> footprint-pad map.
pub fn pin_map() -> Value {
    json!({
        "type": "object",
        "additionalProperties": array(string("pad number"), "pads"),
        "description": "symbol pin number -> footprint pad numbers",
    })
}

pub fn component_def() -> Value {
    let pin = obj(
        &[
            ("number", string("pin number")),
            ("name", string("pin name")),
            (
                "electrical_type",
                enumeration(
                    &[
                        "input",
                        "output",
                        "bidirectional",
                        "passive",
                        "power_in",
                        "power_out",
                        "open_collector",
                        "no_connect",
                    ],
                    "ERC pin type",
                ),
            ),
            (
                "position",
                point("connection point relative to symbol origin"),
            ),
        ],
        &["number", "name", "electrical_type", "position"],
    );
    let symbol = obj(
        &[
            ("pins", array(pin, "symbol pins")),
            ("body_half_w", nm("body half width")),
            ("body_half_h", nm("body half height")),
        ],
        &["pins", "body_half_w", "body_half_h"],
    );
    let footprint = footprint_def();
    let rating = obj(
        &[
            ("name", string("rating name")),
            ("value", number("value")),
            ("unit", string("unit")),
            ("source", string("datasheet URL, standard, or 'unverified'")),
            (
                "verified",
                boolean("true only when checked against the source"),
            ),
        ],
        &["name", "value", "unit", "source", "verified"],
    );
    let model3d = obj(
        &[
            ("kind", string("parametric body kind")),
            ("color", string("body colour")),
            ("external_ref", nullable(string("external model reference"))),
        ],
        &["kind", "color"],
    );
    obj(
        &[
            ("id", string("component id")),
            ("name", string("display name")),
            ("category", string("category")),
            (
                "refdes_prefix",
                string("reference designator prefix, e.g. R"),
            ),
            ("default_value", string("default value")),
            ("manufacturer", nullable(string("manufacturer"))),
            ("mpn", nullable(string("manufacturer part number"))),
            ("lcsc", nullable(string("LCSC part number"))),
            ("datasheet", nullable(string("datasheet URL"))),
            ("symbol", symbol),
            ("footprint", footprint),
            (
                "pin_map",
                json!({
                    "type": "object",
                    "additionalProperties": array(string("pad number"), "pads"),
                    "description": "symbol pin number -> footprint pad numbers",
                }),
            ),
            (
                "ratings",
                array(rating, "electrical ratings with provenance"),
            ),
            ("model3d", model3d),
            ("provenance", string("where this definition came from")),
        ],
        &[
            "id",
            "name",
            "category",
            "refdes_prefix",
            "symbol",
            "footprint",
            "pin_map",
            "model3d",
            "provenance",
        ],
    )
}

#[cfg(test)]
pub(crate) mod check {
    //! Minimal validator for the schema subset produced above (tests only): proves the
    //! hand-written schemas match the serde shape of the real types.
    use serde_json::{json, Value};

    pub fn validate(schema: &Value, v: &Value, path: &str) -> Result<(), String> {
        if let Some(any) = schema.get("anyOf").and_then(Value::as_array) {
            return if any.iter().any(|s| validate(s, v, path).is_ok()) {
                Ok(())
            } else {
                Err(format!("{path}: matches no anyOf branch: {v}"))
            };
        }
        if let Some(e) = schema.get("enum").and_then(Value::as_array) {
            if !e.contains(v) {
                return Err(format!("{path}: {v} not in enum {e:?}"));
            }
        }
        let ty = schema.get("type").and_then(Value::as_str).unwrap_or("any");
        let ok = match ty {
            "object" => v.is_object(),
            "array" => v.is_array(),
            "string" => v.is_string(),
            "integer" => v.is_i64() || v.is_u64(),
            "number" => v.is_number(),
            "boolean" => v.is_boolean(),
            "null" => v.is_null(),
            _ => true,
        };
        if !ok {
            return Err(format!("{path}: expected {ty}, got {v}"));
        }
        if let (Some(o), "object") = (v.as_object(), ty) {
            let props = schema.get("properties").and_then(Value::as_object);
            for r in schema
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let r = r.as_str().unwrap();
                if !o.contains_key(r) {
                    return Err(format!("{path}: missing required {r}"));
                }
            }
            for (k, x) in o {
                match props.and_then(|p| p.get(k)) {
                    Some(s) => validate(s, x, &format!("{path}.{k}"))?,
                    None => match schema.get("additionalProperties") {
                        Some(Value::Bool(false)) => {
                            return Err(format!("{path}: unexpected property {k}"))
                        }
                        Some(s @ Value::Object(_)) => validate(s, x, &format!("{path}.{k}"))?,
                        _ => {}
                    },
                }
            }
        }
        if let (Some(a), Some(items)) = (v.as_array(), schema.get("items")) {
            for (i, x) in a.iter().enumerate() {
                validate(items, x, &format!("{path}[{i}]"))?;
            }
        }
        Ok(())
    }

    /// Smallest instance satisfying the schema: required properties only.
    pub fn minimal(schema: &Value) -> Value {
        if let Some(any) = schema.get("anyOf").and_then(Value::as_array) {
            return minimal(&any[0]);
        }
        if let Some(e) = schema.get("enum").and_then(Value::as_array) {
            return e[0].clone();
        }
        match schema.get("type").and_then(Value::as_str) {
            Some("object") => {
                let mut m = serde_json::Map::new();
                let props = schema.get("properties").and_then(Value::as_object);
                for r in schema
                    .get("required")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let r = r.as_str().unwrap();
                    m.insert(r.into(), minimal(&props.unwrap()[r]));
                }
                Value::Object(m)
            }
            Some("array") => json!([]),
            Some("string") => json!("x"),
            Some("integer") => json!(1),
            Some("number") => json!(1.0),
            Some("boolean") => json!(false),
            _ => Value::Null,
        }
    }
}
