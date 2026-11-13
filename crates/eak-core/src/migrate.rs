//! Schema migration. Version 1 is the first released schema, so the only migration is
//! the identity; newer or missing versions are rejected rather than guessed.

use crate::ir::SCHEMA_VERSION;
use crate::CoreError;
use serde_json::Value;

pub fn migrate(raw: Value) -> Result<Value, CoreError> {
    let found = raw
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| CoreError::Parse("missing schema_version".into()))? as u32;
    if found == 0 || found > SCHEMA_VERSION {
        return Err(CoreError::UnsupportedSchema {
            found,
            supported: SCHEMA_VERSION,
        });
    }
    // Future: match found { 1 => v1_to_v2(raw), ... }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{from_json, to_json, Project};

    #[test]
    fn round_trip_is_byte_stable() {
        let p = Project::new("demo");
        let a = to_json(&p);
        let q = from_json(&a).unwrap();
        assert_eq!(p, q);
        assert_eq!(a, to_json(&q));
    }

    #[test]
    fn newer_schema_is_rejected() {
        let mut v: Value = serde_json::from_str(&to_json(&Project::new("x"))).unwrap();
        v["schema_version"] = Value::from(99);
        let err = from_json(&v.to_string()).unwrap_err();
        assert!(matches!(
            err,
            CoreError::UnsupportedSchema { found: 99, .. }
        ));
    }

    #[test]
    fn corrupt_file_fails_safely() {
        assert!(matches!(from_json("{not json"), Err(CoreError::Parse(_))));
        assert!(matches!(from_json("{}"), Err(CoreError::Parse(_))));
    }
}
