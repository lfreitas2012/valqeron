//! [`JsonSchema`] implementation for [`Lei`].
//!
//! Enabled by the `schemars` feature.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

use super::Lei;

impl JsonSchema for Lei {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Lei")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "lei",
            "minLength": 20,
            "maxLength": 20,
            "pattern": "^[A-Z0-9]{18}[0-9]{2}$",
            "description": "LEI (Legal Entity Identifier, ISO 17442), ISO/IEC 7064 MOD 97-10 checksum-valid."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::Lei;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(Lei);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "lei");
        assert_eq!(json["minLength"], 20);
        assert_eq!(json["maxLength"], 20);
        assert_eq!(json["pattern"], "^[A-Z0-9]{18}[0-9]{2}$");
    }
}
