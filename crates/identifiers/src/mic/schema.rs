//! [`JsonSchema`] implementation for [`Mic`].
//!
//! Enabled by the `schemars` feature. The schema is a structural, pattern-constrained string. It
//! captures the four uppercase letter or digit shape, but a regex cannot express which codes ISO
//! 10383 actually registers. Deserialization (via the `serde` feature) still enforces membership.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

use super::Mic;

impl JsonSchema for Mic {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Mic")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "iso10383-mic",
            "minLength": 4,
            "maxLength": 4,
            "pattern": "^[A-Z0-9]{4}$",
            "description": "ISO 10383 market identifier code. \
            The pattern is structural; membership in the registry is enforced on deserialization."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::Mic;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(Mic);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "iso10383-mic");
        assert_eq!(json["minLength"], 4);
        assert_eq!(json["maxLength"], 4);
        assert_eq!(json["pattern"], "^[A-Z0-9]{4}$");
    }
}
