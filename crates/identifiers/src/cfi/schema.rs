//! [`JsonSchema`] implementation for [`Cfi`].
//!
//! Enabled by the `schemars` feature. The schema is a structural, pattern-constrained string: it
//! captures the six-uppercase-letter shape, but a regex cannot express which category/group/
//! attribute *combinations* ISO 10962 actually defines. Deserialization (via the `serde` feature)
//! still enforces the full taxonomy.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

use super::Cfi;

impl JsonSchema for Cfi {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Cfi")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "cfi",
            "minLength": 6,
            "maxLength": 6,
            "pattern": "^[A-Z]{6}$",
            "description": "CFI (Classification of Financial Instruments, ISO 10962). \
            The pattern is structural; taxonomic validity is enforced on deserialization."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::Cfi;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(Cfi);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "cfi");
        assert_eq!(json["minLength"], 6);
        assert_eq!(json["maxLength"], 6);
        assert_eq!(json["pattern"], "^[A-Z]{6}$");
    }
}
