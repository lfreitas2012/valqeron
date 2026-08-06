//! Reusable [`proptest`] strategies for [`Isin`].
//!
//! Available internally for this crate's own property tests, and to downstream crates under the
//! `proptest` feature, so consumers can property-test code that takes an `Isin` without
//! hand-rolling a checksum-valid generator.

use super::Isin;
use super::validation::{ALPHANUMERIC, BASE_LEN, LETTERS, build_valid_isin_bytes};
use proptest::prelude::{Strategy, prop};
use std::string::{String, ToString};

/// A strategy producing structurally valid, checksum-correct [`Isin`] values: a two-letter country
/// code, a nine-character alphanumeric NSIN, and a matching Luhn check digit.
pub fn valid_isin() -> impl Strategy<Value = Isin> {
    (
        prop::collection::vec(0..LETTERS.len(), 2),
        prop::collection::vec(0..ALPHANUMERIC.len(), BASE_LEN - 2),
    )
        .prop_map(|(country, nsin)| {
            let bytes = build_valid_isin_bytes([country[0], country[1]], &nsin);
            Isin::from_bytes(bytes).expect("generated candidate is checksum-valid by construction")
        })
}

/// A strategy producing a valid [`Isin`] rendered as its canonical 12-character `String`, useful
/// for round-trip-through-parsing property tests.
pub fn valid_isin_string() -> impl Strategy<Value = String> {
    valid_isin().prop_map(|isin| isin.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_isin_always_round_trips_through_parse(isin in valid_isin()) {
            let reparsed = Isin::parse(isin.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(isin, reparsed.unwrap());
        }

        #[test]
        fn valid_isin_string_always_parses(s in valid_isin_string()) {
            prop_assert!(Isin::parse(&s).is_ok());
        }
    }
}
