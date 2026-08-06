//! `Serialize`/`Deserialize` for [`Cfi`], gated behind the `serde` feature.
//!
//! `Cfi` (de)serializes as its canonical 6-character string (e.g. `"ESVUFR"`), so it round-trips as
//! a plain identifier in JSON/config files. Deserializing always re-runs full validation; an
//! untrusted payload can never produce an invalid `Cfi`.

use ::serde::de::{self, Visitor};
use ::serde::{Deserialize, Deserializer, Serialize, Serializer};
use core::fmt;

use super::Cfi;

impl Serialize for Cfi {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct CfiVisitor;

impl<'de> Visitor<'de> for CfiVisitor {
    type Value = Cfi;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 6-character CFI (ISO 10962), e.g. ESVUFR")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Cfi::parse(v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Cfi {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(CfiVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Cfi;

    #[test]
    fn round_trips_through_json() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        let json = serde_json::to_string(&cfi).unwrap();
        assert_eq!(json, "\"ESVUFR\"");
        let back: Cfi = serde_json::from_str(&json).unwrap();
        assert_eq!(cfi, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<Cfi>("\"not-a-cfi\"").unwrap_err();
        assert!(err.to_string().contains("CFI"));
    }
}
