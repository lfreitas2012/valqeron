#![allow(
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::string_slice,
    clippy::unreachable,
    clippy::unwrap_used
)]

const BBC: &str = "5493000IBP32UQZ0KL24";
const JLR: &str = "213800WSGIIZCXF1P572";

mod construction {
    use crate::{BBC, JLR};
    use valqeron_identifiers::{Lei, LeiError};

    #[test]
    fn parse_accepts_canonical_input() {
        assert!(Lei::parse(BBC).is_ok());
    }

    #[test]
    fn parse_accepts_alphanumeric_entity_part() {
        assert!(Lei::parse(JLR).is_ok());
    }

    #[test]
    fn parse_accepts_lowercase_letters() {
        assert_eq!(
            Lei::parse("5493000ibp32uqz0kl24").unwrap(),
            Lei::parse(BBC).unwrap()
        );
    }

    #[test]
    fn parse_trims_surrounding_whitespace() {
        assert_eq!(
            Lei::parse("  5493000IBP32UQZ0KL24\t").unwrap(),
            Lei::parse(BBC).unwrap()
        );
    }

    #[test]
    fn new_is_an_alias_for_parse() {
        assert_eq!(Lei::new(BBC), Lei::parse(BBC));
    }

    #[test]
    fn from_bytes_round_trips_with_as_bytes() {
        let lei = Lei::parse(JLR).unwrap();
        let rebuilt = Lei::from_bytes(*lei.as_bytes()).unwrap();
        assert_eq!(lei, rebuilt);
    }

    #[test]
    fn empty_input_is_rejected() {
        assert_eq!(Lei::parse(""), Err(LeiError::Empty));
    }

    #[test]
    fn accepts_a_spread_of_real_world_leis() {
        for s in [
            "506700GE1G29325QX363", // GLEIF
            "54930084UKLVMY22DS16", // G.E. Financing GmbH
        ] {
            assert!(Lei::parse(s).is_ok(), "{s} should parse");
        }
    }
}

mod accessors {
    use crate::{BBC, JLR};
    use valqeron_identifiers::Lei;

    #[test]
    fn segments_are_split_correctly() {
        let lei = Lei::parse(BBC).unwrap();
        assert_eq!(lei.lou_prefix(), "5493");
        assert_eq!(lei.entity_id(), "000IBP32UQZ0KL");
        assert_eq!(lei.check_digits(), 24);
        assert_eq!(lei.as_str(), "5493000IBP32UQZ0KL24");
    }

    #[test]
    fn alphanumeric_entity_part_is_preserved() {
        let lei = Lei::parse(JLR).unwrap();
        assert_eq!(lei.lou_prefix(), "2138");
        assert_eq!(lei.entity_id(), "00WSGIIZCXF1P5");
        assert_eq!(lei.check_digits(), 72);
    }

    #[test]
    fn computed_check_digits_match_stored_check_digits() {
        for s in [BBC, JLR, "506700GE1G29325QX363", "54930084UKLVMY22DS16"] {
            let lei = Lei::parse(s).unwrap();
            assert_eq!(lei.computed_check_digits(), lei.check_digits(), "{s}");
        }
    }
}

mod error_paths {
    use valqeron_identifiers::lei::CharacterClass;
    use valqeron_identifiers::{Lei, LeiError};

    #[test]
    fn reports_invalid_length() {
        assert_eq!(
            Lei::parse("5493000IBP32UQZ0KL2"),
            Err(LeiError::InvalidLength { found: 19 })
        );
    }

    #[test]
    fn reports_non_alphanumeric_in_base() {
        let err = Lei::parse("5493000IBP32UQZ0K!24").unwrap_err();
        assert_eq!(
            err,
            LeiError::InvalidCharacter {
                character: '!',
                position: 18,
                expected: CharacterClass::Alphanumeric,
            }
        );
    }

    #[test]
    fn reports_non_digit_in_check_position() {
        let err = Lei::parse("5493000IBP32UQZ0KLX4").unwrap_err();
        assert_eq!(
            err,
            LeiError::InvalidCharacter {
                character: 'X',
                position: 19,
                expected: CharacterClass::Digit,
            }
        );
    }

    #[test]
    fn reports_invalid_check_digits() {
        assert_eq!(
            Lei::parse("5493000IBP32UQZ0KL25"),
            Err(LeiError::InvalidCheckDigits {
                expected: 24,
                found: 25,
            })
        );
    }

    #[test]
    fn error_messages_are_human_readable() {
        assert_eq!(
            Lei::parse("").unwrap_err().to_string(),
            "LEI input is empty"
        );
    }
}

mod trait_impls {
    use crate::{BBC, JLR};
    use std::collections::{BTreeSet, HashSet};
    use valqeron_identifiers::{Lei, LeiError};

    #[test]
    fn from_str_delegates_to_parse() {
        let a: Lei = BBC.parse().unwrap();
        let b = Lei::parse(BBC).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn try_from_str_delegates_to_parse() {
        let a = Lei::try_from(BBC).unwrap();
        let b = Lei::parse(BBC).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn try_from_byte_array_delegates_to_from_bytes() {
        let a = Lei::try_from(*b"5493000IBP32UQZ0KL24").unwrap();
        assert_eq!(a, Lei::parse(BBC).unwrap());
    }

    #[test]
    fn try_from_byte_slice_validates_length() {
        let good: &[u8] = b"5493000IBP32UQZ0KL24";
        assert_eq!(Lei::try_from(good).unwrap(), Lei::parse(BBC).unwrap());

        let short: &[u8] = b"5493000IBP32UQZ0KL2";
        assert_eq!(
            Lei::try_from(short),
            Err(LeiError::InvalidLength { found: 19 })
        );
    }

    #[test]
    fn partial_eq_with_str() {
        let lei = Lei::parse(BBC).unwrap();
        assert_eq!(lei, *"5493000IBP32UQZ0KL24");
        assert_eq!(lei, "5493000IBP32UQZ0KL24");
        assert_eq!("5493000IBP32UQZ0KL24", lei);
        assert_ne!(lei, "0000000000000000000O");
    }

    #[test]
    fn as_ref_bytes_matches_as_bytes() {
        let lei = Lei::parse(BBC).unwrap();
        let as_ref: &[u8] = lei.as_ref();
        assert_eq!(as_ref, lei.as_bytes().as_slice());
    }

    #[test]
    fn as_ref_str_matches_as_str() {
        let lei = Lei::parse(BBC).unwrap();
        let as_ref: &str = lei.as_ref();
        assert_eq!(as_ref, lei.as_str());
    }

    #[test]
    fn is_copy_and_clone() {
        let lei = Lei::parse(BBC).unwrap();
        let copied = lei;
        assert_eq!(lei, copied);
        assert_eq!(lei, lei.clone());
    }

    #[test]
    fn ordering_is_lexicographic_over_bytes() {
        let jlr = Lei::parse(JLR).unwrap();
        let bbc = Lei::parse(BBC).unwrap();
        // "2138..." sorts before "5493..."
        assert!(jlr < bbc);
    }

    #[test]
    fn works_as_a_hashset_key() {
        let mut set = HashSet::new();
        set.insert(Lei::parse(BBC).unwrap());
        assert!(set.contains(&Lei::parse(BBC).unwrap()));
    }

    #[test]
    fn works_as_a_btree_set_key() {
        let mut set = BTreeSet::new();
        set.insert(Lei::parse(BBC).unwrap());
        set.insert(Lei::parse(JLR).unwrap());
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn display_and_debug_are_both_readable() {
        let lei = Lei::parse(BBC).unwrap();
        assert_eq!(lei.to_string(), "5493000IBP32UQZ0KL24");
        assert_eq!(format!("{lei:?}"), "Lei(\"5493000IBP32UQZ0KL24\")");
    }
}
