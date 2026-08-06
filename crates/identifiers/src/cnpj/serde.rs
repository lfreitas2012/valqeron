//! `Serialize`/`Deserialize` for [`Cnpj`], gated behind the `serde` feature.
//!
//! `Cnpj` (de)serializes as its unformatted 14-character string (e.g. `"12ABC34501DE35"`), not the
//! punctuated display form, so that it round-trips as a plain identifier in JSON/config files.
//! Deserializing always re-runs full validation; an untrusted payload can never produce an invalid `Cnpj`.

use ::serde::de::{self, Visitor};
use ::serde::{Deserialize, Deserializer, Serialize, Serializer};
use core::fmt;

use super::Cnpj;

impl Serialize for Cnpj {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct CnpjVisitor;

impl<'de> Visitor<'de> for CnpjVisitor {
    type Value = Cnpj;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 14-character CNPJ, optionally punctuated as AA.AAA.AAA/AAAA-DD")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Cnpj::parse(v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Cnpj {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(CnpjVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Cnpj;

    #[test]
    fn round_trips_through_json() {
        let cnpj = Cnpj::parse("00.000.000/0001-91").unwrap();
        let json = serde_json::to_string(&cnpj).unwrap();
        assert_eq!(json, "\"00000000000191\"");
        let back: Cnpj = serde_json::from_str(&json).unwrap();
        assert_eq!(cnpj, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<Cnpj>("\"not-a-cnpj\"").unwrap_err();
        assert!(err.to_string().contains("CNPJ"));
    }
}
