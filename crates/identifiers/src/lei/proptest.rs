//! Reusable [`proptest`] strategies for [`Lei`].
//!
//! Available internally for this crate's own property tests, and to downstream crates under the
//! `proptest` feature, so consumers can property-test code that takes a `Lei` without hand-rolling a
//! checksum-valid generator.

use super::Lei;
use super::validation::{ALPHANUMERIC, BASE_LEN, build_valid_lei_bytes};
use alloc::string::{String, ToString};
use proptest::prelude::{Strategy, prop};

/// A strategy producing structurally valid, checksum-correct [`Lei`] values: eighteen alphanumeric
/// base characters and two matching ISO/IEC 7064 MOD 97-10 check digits.
pub fn valid_lei() -> impl Strategy<Value = Lei> {
    prop::collection::vec(0..ALPHANUMERIC.len(), BASE_LEN).prop_map(|base| {
        let mut indices = [0usize; BASE_LEN];
        indices.copy_from_slice(&base);
        let bytes = build_valid_lei_bytes(&indices);
        Lei::from_bytes(bytes).expect("generated candidate is checksum-valid by construction")
    })
}

/// A strategy producing a valid [`Lei`] rendered as its canonical 20-character `String`, useful for
/// round-trip-through-parsing property tests.
pub fn valid_lei_string() -> impl Strategy<Value = String> {
    valid_lei().prop_map(|lei| lei.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_lei_always_round_trips_through_parse(lei in valid_lei()) {
            let reparsed = Lei::parse(lei.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(lei, reparsed.unwrap());
        }

        #[test]
        fn valid_lei_string_always_parses(s in valid_lei_string()) {
            prop_assert!(Lei::parse(&s).is_ok());
        }
    }
}
