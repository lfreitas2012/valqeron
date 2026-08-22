const APPLE: &str = "US0378331005";
const PETROBRAS: &str = "BRPETRACNOR9";

mod construction {
    use super::*;
    use valqeron_core::identifiers::{Isin, IsinError};

    #[test]
    fn parse_accepts_numeric_nsin() {
        assert!(Isin::parse(APPLE).is_ok());
    }

    #[test]
    fn parse_accepts_alphanumeric_nsin() {
        assert!(Isin::parse(PETROBRAS).is_ok());
    }

    #[test]
    fn parse_accepts_lowercase_letters() {
        assert_eq!(
            Isin::parse("us0378331005").unwrap(),
            Isin::parse(APPLE).unwrap()
        );
    }

    #[test]
    fn parse_trims_surrounding_whitespace() {
        assert_eq!(
            Isin::parse("  US0378331005\t").unwrap(),
            Isin::parse(APPLE).unwrap()
        );
    }

    #[test]
    fn new_is_an_alias_for_parse() {
        assert_eq!(Isin::new(APPLE), Isin::parse(APPLE));
    }

    #[test]
    fn from_bytes_round_trips_with_as_bytes() {
        let isin = Isin::parse(PETROBRAS).unwrap();
        let rebuilt = Isin::from_bytes(*isin.as_bytes()).unwrap();
        assert_eq!(isin, rebuilt);
    }

    #[test]
    fn empty_input_is_rejected() {
        assert_eq!(Isin::parse(""), Err(IsinError::Empty));
    }

    #[test]
    fn accepts_a_spread_of_real_world_isins() {
        for s in [
            "US0231351067", // Amazon
            "GB0002634946", // UK gilt
            "DE0001102333", // German Bund
            "JP3633400001", // Japanese equity
            "AU000000BHP4", // BHP
            "CH0012221716", // Nestlé
        ] {
            assert!(Isin::parse(s).is_ok(), "{s} should parse");
        }
    }
}

mod accessors {
    use super::*;
    use valqeron_core::identifiers::Isin;

    #[test]
    fn segments_are_split_correctly() {
        let isin = Isin::parse(APPLE).unwrap();
        assert_eq!(isin.country_code(), "US");
        assert_eq!(isin.nsin(), "037833100");
        assert_eq!(isin.check_digit(), 5);
        assert_eq!(isin.as_str(), "US0378331005");
    }

    #[test]
    fn alphanumeric_nsin_is_preserved() {
        let isin = Isin::parse(PETROBRAS).unwrap();
        assert_eq!(isin.country_code(), "BR");
        assert_eq!(isin.nsin(), "PETRACNOR");
        assert_eq!(isin.check_digit(), 9);
    }

    #[test]
    fn computed_check_digit_matches_stored_check_digit() {
        for s in [APPLE, PETROBRAS, "GB0002634946", "AU000000BHP4"] {
            let isin = Isin::parse(s).unwrap();
            assert_eq!(isin.computed_check_digit(), isin.check_digit(), "{s}");
        }
    }
}

mod error_paths {
    use std::string::ToString;
    use valqeron_core::identifiers::{CharacterClass, Isin, IsinError};

    #[test]
    fn reports_invalid_length() {
        assert_eq!(
            Isin::parse("US03783310"),
            Err(IsinError::InvalidLength { found: 10 })
        );
    }

    #[test]
    fn reports_non_letter_in_country_code() {
        let err = Isin::parse("1S0378331005").unwrap_err();
        assert_eq!(
            err,
            IsinError::InvalidCharacter {
                character: '1',
                position: 1,
                expected: CharacterClass::Letter,
            }
        );
    }

    #[test]
    fn reports_non_digit_in_check_position() {
        let err = Isin::parse("US037833100X").unwrap_err();
        assert_eq!(
            err,
            IsinError::InvalidCharacter {
                character: 'X',
                position: 12,
                expected: CharacterClass::Digit,
            }
        );
    }

    #[test]
    fn reports_invalid_check_digit() {
        assert_eq!(
            Isin::parse("US0378331006"),
            Err(IsinError::InvalidCheckDigit {
                expected: 5,
                found: 6,
            })
        );
    }

    #[test]
    fn error_messages_are_human_readable() {
        assert_eq!(
            Isin::parse("").unwrap_err().to_string(),
            "ISIN input is empty"
        );
    }
}

mod trait_impls {
    use super::*;
    use std::collections::{BTreeSet, HashSet};
    use valqeron_core::identifiers::{CountryCode, Isin, IsinError};

    #[test]
    fn from_str_delegates_to_parse() {
        let a: Isin = APPLE.parse().unwrap();
        let b = Isin::parse(APPLE).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn try_from_str_delegates_to_parse() {
        let a = Isin::try_from(APPLE).unwrap();
        let b = Isin::parse(APPLE).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn try_from_byte_array_delegates_to_from_bytes() {
        let a = Isin::try_from(*b"US0378331005").unwrap();
        assert_eq!(a, Isin::parse(APPLE).unwrap());
    }

    #[test]
    fn try_from_byte_slice_validates_length() {
        let good: &[u8] = b"US0378331005";
        assert_eq!(Isin::try_from(good).unwrap(), Isin::parse(APPLE).unwrap());

        let short: &[u8] = b"US03783310";
        assert_eq!(
            Isin::try_from(short),
            Err(IsinError::InvalidLength { found: 10 })
        );
    }

    #[test]
    fn partial_eq_with_str() {
        let isin = Isin::parse(APPLE).unwrap();
        assert_eq!(isin, *"US0378331005");
        assert_eq!(isin, "US0378331005");
        assert_eq!("US0378331005", isin);
        assert_ne!(isin, "US0000000000");
    }

    #[test]
    fn country_maps_assigned_prefix() {
        let isin = Isin::parse(APPLE).unwrap();
        assert_eq!(isin.country(), Some(CountryCode::parse("US").unwrap()));
    }

    #[test]
    fn country_is_none_for_unassigned_prefix() {
        // `XS` is a valid ISIN prefix (Euroclear/Clearstream) but not an assigned ISO 3166-1 code.
        let isin = Isin::parse("XS0000198795").unwrap();
        assert_eq!(isin.country_code(), "XS");
        assert_eq!(isin.country(), None);
    }

    #[test]
    fn as_ref_bytes_matches_as_bytes() {
        let isin = Isin::parse(APPLE).unwrap();
        let as_ref: &[u8] = isin.as_ref();
        assert_eq!(as_ref, isin.as_bytes().as_slice());
    }

    #[test]
    fn as_ref_str_matches_as_str() {
        let isin = Isin::parse(APPLE).unwrap();
        let as_ref: &str = isin.as_ref();
        assert_eq!(as_ref, isin.as_str());
    }

    #[test]
    fn is_copy_and_clone() {
        let isin = Isin::parse(APPLE).unwrap();
        let copied = isin;
        assert_eq!(isin, copied);
        assert_eq!(isin, isin.clone());
    }

    #[test]
    fn ordering_is_lexicographic_over_bytes() {
        let de = Isin::parse("DE0001102333").unwrap();
        let us = Isin::parse(APPLE).unwrap();
        assert!(de < us);
    }

    #[test]
    fn works_as_a_hashset_key() {
        let mut set = HashSet::new();
        set.insert(Isin::parse(APPLE).unwrap());
        assert!(set.contains(&Isin::parse(APPLE).unwrap()));
    }

    #[test]
    fn works_as_a_btree_set_key() {
        let mut set = BTreeSet::new();
        set.insert(Isin::parse(APPLE).unwrap());
        set.insert(Isin::parse(PETROBRAS).unwrap());
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn display_and_debug_are_both_readable() {
        let isin = Isin::parse(APPLE).unwrap();
        assert_eq!(isin.to_string(), "US0378331005");
        assert_eq!(format!("{isin:?}"), "Isin(\"US0378331005\")");
    }
}
