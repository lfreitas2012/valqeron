//! Reusable [`proptest`] strategies for [`Mic`].
//!
//! Available internally for this crate's own property tests, and to downstream crates under the
//! `proptest` feature, so consumers can property test code that takes a [`Mic`] without hand
//! rolling a generator that only yields registered codes.

use super::Mic;
use super::table::ENTRIES;
use proptest::prelude::Strategy;

/// A strategy producing valid [`Mic`] values by picking from the registered set.
pub fn valid_mic() -> impl Strategy<Value = Mic> {
    (0..ENTRIES.len()).prop_map(|index| {
        Mic::from_bytes(ENTRIES[index].code)
            .expect("codes in the registry table are valid by construction")
    })
}

/// A strategy producing a valid [`Mic`] rendered as its canonical four character `String`, useful
/// for round trip through parsing property tests.
pub fn valid_mic_string() -> impl Strategy<Value = String> {
    valid_mic().prop_map(|mic| mic.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_mic_always_round_trips_through_parse(mic in valid_mic()) {
            let reparsed = Mic::parse(mic.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(mic, reparsed.unwrap());
        }

        #[test]
        fn valid_mic_string_always_parses(s in valid_mic_string()) {
            prop_assert!(Mic::parse(&s).is_ok());
        }

        #[test]
        fn operating_references_stay_inside_the_registry(mic in valid_mic()) {
            // The published operating reference of any registered code is itself registered, and
            // an operating MIC is exactly a code that references itself.
            let operating = mic.operating_mic();
            prop_assert!(Mic::parse(operating.as_str()).is_ok());
            prop_assert_eq!(mic.is_operating(), operating == mic);
        }
    }
}
