//! `Serialize` and `Deserialize` for [`Mic`], gated behind the `serde` feature.
//!
//! `Mic` (de)serializes as its canonical four-character string, for example, `"XNYS"`, so it round
//! trips as a plain identifier in JSON and config files. Deserializing always re-runs full
//! validation. An untrusted payload can never produce an invalid `Mic`.

use ::serde::de::{self, Visitor};
use ::serde::{Deserialize, Deserializer, Serialize, Serializer};
use core::fmt;

use super::Mic;

impl Serialize for Mic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct MicVisitor;

impl<'de> Visitor<'de> for MicVisitor {
    type Value = Mic;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 4-character ISO 10383 market identifier code, e.g. XNYS")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Mic::parse(v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Mic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(MicVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Mic;

    #[test]
    fn round_trips_through_json() {
        let mic = Mic::parse("XNYS").unwrap();
        let json = serde_json::to_string(&mic).unwrap();
        assert_eq!(json, "\"XNYS\"");
        let back: Mic = serde_json::from_str(&json).unwrap();
        assert_eq!(mic, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<Mic>("\"ZZZZ\"").unwrap_err();
        assert!(err.to_string().contains("ISO 10383"));
    }
}
