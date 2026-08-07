//! `Serialize`/`Deserialize` for [`Lei`], gated behind the `serde` feature.
//!
//! `Lei` (de)serializes as its canonical 20-character string (e.g. `"5493000IBP32UQZ0KL24"`), so it
//! round-trips as a plain identifier in JSON/config files. Deserializing always re-runs full
//! validation; an untrusted payload can never produce an invalid `Lei`.

use ::serde::de::{self, Visitor};
use ::serde::{Deserialize, Deserializer, Serialize, Serializer};
use core::fmt;

use super::Lei;

impl Serialize for Lei {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct LeiVisitor;

impl<'de> Visitor<'de> for LeiVisitor {
    type Value = Lei;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 20-character LEI (ISO 17442), e.g. 5493000IBP32UQZ0KL24")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Lei::parse(v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Lei {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(LeiVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Lei;

    #[test]
    fn round_trips_through_json() {
        let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
        let json = serde_json::to_string(&lei).unwrap();
        assert_eq!(json, "\"5493000IBP32UQZ0KL24\"");
        let back: Lei = serde_json::from_str(&json).unwrap();
        assert_eq!(lei, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<Lei>("\"not-an-lei\"").unwrap_err();
        assert!(err.to_string().contains("LEI"));
    }
}
