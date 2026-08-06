//! `Serialize`/`Deserialize` for [`Isin`], gated behind the `serde` feature.
//!
//! `Isin` (de)serializes as its canonical 12-character string (e.g. `"US0378331005"`), so it
//! round-trips as a plain identifier in JSON/config files. Deserializing always re-runs full
//! validation; an untrusted payload can never produce an invalid `Isin`.

use ::serde::de::{self, Visitor};
use ::serde::{Deserialize, Deserializer, Serialize, Serializer};
use core::fmt;

use super::Isin;

impl Serialize for Isin {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct IsinVisitor;

impl<'de> Visitor<'de> for IsinVisitor {
    type Value = Isin;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 12-character ISIN (ISO 6166), e.g. US0378331005")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Isin::parse(v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Isin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(IsinVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Isin;

    #[test]
    fn round_trips_through_json() {
        let isin = Isin::parse("US0378331005").unwrap();
        let json = serde_json::to_string(&isin).unwrap();
        assert_eq!(json, "\"US0378331005\"");
        let back: Isin = serde_json::from_str(&json).unwrap();
        assert_eq!(isin, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<Isin>("\"not-an-isin\"").unwrap_err();
        assert!(err.to_string().contains("ISIN"));
    }
}
