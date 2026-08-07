//! [`JsonSchema`] implementation for [`Cnpj`].
//!
//! Enabled by the `schemars` feature.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

use super::Cnpj;

impl JsonSchema for Cnpj {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Cnpj")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "cnpj",
            "minLength": 14,
            "maxLength": 14,
            "pattern": "^[A-Z0-9]{12}[0-9]{2}$",
            "description": "Brazilian CNPJ (Cadastro Nacional da Pessoa Jurídica), unformatted, Módulo 11 checksum-valid."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::Cnpj;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(Cnpj);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "cnpj");
        assert_eq!(json["minLength"], 14);
        assert_eq!(json["maxLength"], 14);
        assert_eq!(json["pattern"], "^[A-Z0-9]{12}[0-9]{2}$");
    }
}
