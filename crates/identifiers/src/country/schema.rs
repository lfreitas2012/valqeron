//! [`JsonSchema`] implementation for [`CountryCode`].
//!
//! Enabled by the `schemars` feature. The schema is a structural, pattern constrained string. It
//! captures the two uppercase letter shape, but a regex cannot express which two letter codes ISO
//! 3166-1 actually assigns. Deserialization (via the `serde` feature) still enforces membership.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

use super::CountryCode;

impl JsonSchema for CountryCode {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("CountryCode")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "iso3166-1-alpha2",
            "minLength": 2,
            "maxLength": 2,
            "pattern": "^[A-Z]{2}$",
            "description": "ISO 3166-1 alpha-2 country code. \
            The pattern is structural; membership in the assigned set is enforced on deserialization."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::CountryCode;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(CountryCode);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "iso3166-1-alpha2");
        assert_eq!(json["minLength"], 2);
        assert_eq!(json["maxLength"], 2);
        assert_eq!(json["pattern"], "^[A-Z]{2}$");
    }
}
