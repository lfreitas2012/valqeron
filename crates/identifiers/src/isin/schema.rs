//! [`JsonSchema`] implementation for [`Isin`].
//!
//! Enabled by the `schemars` feature.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

use super::Isin;

impl JsonSchema for Isin {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Isin")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "isin",
            "minLength": 12,
            "maxLength": 12,
            "pattern": "^[A-Z]{2}[A-Z0-9]{9}[0-9]$",
            "description": "ISIN (International Securities Identification Number, ISO 6166), Luhn checksum-valid."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::Isin;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(Isin);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "isin");
        assert_eq!(json["minLength"], 12);
        assert_eq!(json["maxLength"], 12);
        assert_eq!(json["pattern"], "^[A-Z]{2}[A-Z0-9]{9}[0-9]$");
    }
}
