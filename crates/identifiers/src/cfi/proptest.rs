//! Reusable [`proptest`] strategies for [`Cfi`].
//!
//! Available internally for this crate's own property tests, and to downstream crates under the
//! `proptest` feature, so consumers can property-test code that takes a `Cfi` without hand-rolling
//! a taxonomically valid generator.

use super::Cfi;
use super::table::CATEGORIES;
use super::validation::nth_letter;
use proptest::prelude::{Just, Strategy, any, prop};
use std::string::{String, ToString};

/// A strategy producing taxonomically valid [`Cfi`] values by walking the embedded ISO 10962 table:
/// it picks a category, then a group within it, then a permitted letter for each of the four
/// attribute positions.
pub fn valid_cfi() -> impl Strategy<Value = Cfi> {
    (0..CATEGORIES.len())
        .prop_flat_map(|category_index| {
            let group_count = CATEGORIES[category_index].groups.len();
            (Just(category_index), 0..group_count)
        })
        .prop_flat_map(|(category_index, group_index)| {
            (
                Just(category_index),
                Just(group_index),
                prop::array::uniform4(any::<usize>()),
            )
        })
        .prop_map(|(category_index, group_index, selectors)| {
            let category = &CATEGORIES[category_index];
            let group = &category.groups[group_index];

            let mut bytes = [0u8; 6];
            bytes[0] = category.code;
            bytes[1] = group.code;
            for (i, selector) in selectors.iter().enumerate() {
                bytes[2 + i] = nth_letter(group.attrs[i], *selector);
            }

            Cfi::from_bytes(bytes)
                .expect("generated candidate is taxonomically valid by construction")
        })
}

/// A strategy producing a valid [`Cfi`] rendered as its canonical 6-character `String`, useful for
/// round-trip-through-parsing property tests.
pub fn valid_cfi_string() -> impl Strategy<Value = String> {
    valid_cfi().prop_map(|cfi| cfi.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_cfi_always_round_trips_through_parse(cfi in valid_cfi()) {
            let reparsed = Cfi::parse(cfi.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(cfi, reparsed.unwrap());
        }

        #[test]
        fn valid_cfi_string_always_parses(s in valid_cfi_string()) {
            prop_assert!(Cfi::parse(&s).is_ok());
        }
    }
}
