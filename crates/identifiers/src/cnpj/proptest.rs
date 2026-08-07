//! Reusable [`proptest`] strategies for [`Cnpj`].
//!
//! Available internally for this crate's own property tests, and to downstream crates under the
//! `proptest` feature, so consumers can property-test code that takes a `Cnpj` without
//! hand-rolling a checksum-valid generator.

use super::Cnpj;
use super::validation::{BASE_LEN, avoid_all_repeated, compute_valid_check_digits};
use proptest::prelude::{Strategy, prop};

const ALPHABET: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// A strategy producing structurally valid, checksum-correct [`Cnpj`] values, spanning both the
/// legacy numeric-only format and the alphanumeric format.
pub fn valid_cnpj() -> impl Strategy<Value = Cnpj> {
    prop::collection::vec(0..ALPHABET.len(), BASE_LEN).prop_map(|indices| {
        let mut base = [0u8; BASE_LEN];
        for (slot, idx) in base.iter_mut().zip(indices) {
            *slot = ALPHABET[idx];
        }
        avoid_all_repeated(&mut base);

        let (dv1, dv2) = compute_valid_check_digits(&base);
        let mut bytes = [0u8; 14];
        bytes[..BASE_LEN].copy_from_slice(&base);
        bytes[BASE_LEN] = dv1 + b'0';
        bytes[BASE_LEN + 1] = dv2 + b'0';

        Cnpj::from_bytes(bytes).expect("generated candidate is checksum-valid by construction")
    })
}

/// A strategy producing a valid [`Cnpj`] rendered with conventional `AA.AAA.AAA/AAAA-DD` punctuation,
/// useful for round-trip-through-formatting property tests.
pub fn valid_cnpj_formatted_string() -> impl Strategy<Value = String> {
    valid_cnpj().prop_map(|c| c.formatted().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_cnpj_always_round_trips_through_parse(cnpj in valid_cnpj()) {
            let reparsed = Cnpj::parse(cnpj.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(cnpj, reparsed.unwrap());
        }

        #[test]
        fn formatted_string_always_round_trips(s in valid_cnpj_formatted_string()) {
            prop_assert!(Cnpj::parse(&s).is_ok());
        }
    }
}
