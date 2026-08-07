//! `Serialize` and `Deserialize` for [`CountryCode`], gated behind the `serde` feature.
//!
//! `CountryCode` (de)serializes as its canonical two letter string, for example `"US"`, so it round
//! trips as a plain identifier in JSON and config files. Deserializing always re-runs full
//! validation. An untrusted payload can never produce an invalid `CountryCode`.

use ::serde::de::{self, Visitor};
use ::serde::{Deserialize, Deserializer, Serialize, Serializer};
use core::fmt;

use super::CountryCode;

impl Serialize for CountryCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct CountryCodeVisitor;

impl<'de> Visitor<'de> for CountryCodeVisitor {
    type Value = CountryCode;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 2-character ISO 3166-1 alpha-2 country code, e.g. US")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        CountryCode::parse(v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for CountryCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(CountryCodeVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::super::CountryCode;

    #[test]
    fn round_trips_through_json() {
        let code = CountryCode::parse("US").unwrap();
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"US\"");
        let back: CountryCode = serde_json::from_str(&json).unwrap();
        assert_eq!(code, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<CountryCode>("\"ZZ\"").unwrap_err();
        assert!(err.to_string().contains("ISO 3166-1"));
    }
}
