//! Reusable [`proptest`] strategies for [`CountryCode`].
//!
//! Available internally for this crate's own property tests, and to downstream crates under the
//! `proptest` feature, so consumers can property test code that takes a [`CountryCode`] without
//! hand rolling a generator that only yields assigned codes.

use super::CountryCode;
use super::table::ASSIGNED_CODES;
use proptest::prelude::Strategy;
use std::string::{String, ToString};

/// A strategy producing valid [`CountryCode`] values by picking from the officially assigned set.
pub fn valid_country_code() -> impl Strategy<Value = CountryCode> {
    (0..ASSIGNED_CODES.len()).prop_map(|index| {
        CountryCode::from_bytes(ASSIGNED_CODES[index])
            .expect("codes in the assigned set are valid by construction")
    })
}

/// A strategy producing a valid [`CountryCode`] rendered as its canonical two letter `String`,
/// useful for round trip through parsing property tests.
pub fn valid_country_code_string() -> impl Strategy<Value = String> {
    valid_country_code().prop_map(|code| code.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_country_code_always_round_trips_through_parse(code in valid_country_code()) {
            let reparsed = CountryCode::parse(code.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(code, reparsed.unwrap());
        }

        #[test]
        fn valid_country_code_string_always_parses(s in valid_country_code_string()) {
            prop_assert!(CountryCode::parse(&s).is_ok());
        }
    }
}
