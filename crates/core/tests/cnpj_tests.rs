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

const NUMERIC_ROOT: &str = "00.000.000/0001-91";
const ALPHANUMERIC: &str = "12ABC34501DE35";

mod construction {
    use super::*;
    use valqeron_core::identifiers::{Cnpj, CnpjError};

    #[test]
    fn parse_accepts_punctuated_numeric_input() {
        assert!(Cnpj::parse(NUMERIC_ROOT).is_ok());
    }

    #[test]
    fn parse_accepts_unpunctuated_numeric_input() {
        assert!(Cnpj::parse("00000000000191").is_ok());
    }

    #[test]
    fn parse_accepts_alphanumeric_input() {
        assert!(Cnpj::parse(ALPHANUMERIC).is_ok());
    }

    #[test]
    fn parse_accepts_lowercase_letters() {
        assert_eq!(
            Cnpj::parse("12abc34501de35").unwrap(),
            Cnpj::parse(ALPHANUMERIC).unwrap()
        );
    }

    #[test]
    fn parse_tolerates_extra_spaces() {
        assert_eq!(
            Cnpj::parse(" 00.000.000/0001-91 ").unwrap(),
            Cnpj::parse(NUMERIC_ROOT).unwrap()
        );
    }

    #[test]
    fn new_is_an_alias_for_parse() {
        assert_eq!(Cnpj::new(NUMERIC_ROOT), Cnpj::parse(NUMERIC_ROOT));
    }

    #[test]
    fn from_bytes_round_trips_with_as_bytes() {
        let cnpj = Cnpj::parse(ALPHANUMERIC).unwrap();
        let rebuilt = Cnpj::from_bytes(*cnpj.as_bytes()).unwrap();
        assert_eq!(cnpj, rebuilt);
    }

    #[test]
    fn empty_input_is_rejected() {
        assert_eq!(Cnpj::parse(""), Err(CnpjError::Empty));
    }
}

mod accessors {
    use super::*;
    use valqeron_core::identifiers::Cnpj;

    #[test]
    fn root_and_branch_are_split_correctly() {
        let cnpj = Cnpj::parse(NUMERIC_ROOT).unwrap();
        assert_eq!(cnpj.root(), "00000000");
        assert_eq!(cnpj.branch_code(), "0001");
        assert!(cnpj.is_root());
        assert_eq!(cnpj.branch_number(), Some(1));
    }

    #[test]
    fn non_root_branch_is_recognized() {
        let cnpj = Cnpj::parse("11222333000262").unwrap();

        assert!(!cnpj.is_root());
        assert_eq!(cnpj.branch_number(), Some(2));
    }

    #[test]
    fn alphanumeric_branch_has_no_numeric_value() {
        let cnpj = Cnpj::parse(ALPHANUMERIC).unwrap();
        assert_eq!(cnpj.branch_code(), "01DE");
        assert_eq!(cnpj.branch_number(), None);
    }

    #[test]
    fn check_digits_match_the_trailing_bytes() {
        let cnpj = Cnpj::parse(NUMERIC_ROOT).unwrap();
        assert_eq!(cnpj.check_digits(), (9, 1));
    }

    #[test]
    fn as_str_has_no_punctuation() {
        let cnpj = Cnpj::parse(NUMERIC_ROOT).unwrap();
        assert_eq!(cnpj.as_str(), "00000000000191");
    }
}

mod error_paths {
    use valqeron_core::identifiers::{Cnpj, CnpjError};

    #[test]
    fn reports_invalid_length() {
        assert_eq!(
            Cnpj::parse("123"),
            Err(CnpjError::InvalidLength { found: 3 })
        );
    }

    #[test]
    fn reports_invalid_character_with_position_and_expected_class() {
        let err = Cnpj::parse("00.000.000/0001-9A").unwrap_err();
        assert!(matches!(
            err,
            CnpjError::InvalidCharacter {
                character: 'A',
                position: 14,
                ..
            }
        ));
    }

    #[test]
    fn reports_invalid_check_digits() {
        let err = Cnpj::parse("00.000.000/0001-92").unwrap_err();
        assert!(matches!(
            err,
            CnpjError::InvalidCheckDigits { position: 14, .. }
        ));
    }

    #[test]
    fn reports_repeated_digits() {
        assert_eq!(
            Cnpj::parse("11111111111111"),
            Err(CnpjError::RepeatedDigits)
        );
    }

    #[test]
    fn error_messages_are_human_readable() {
        let err = Cnpj::parse("").unwrap_err();
        assert_eq!(err.to_string(), "CNPJ input is empty");
    }
}

mod trait_impls {
    use super::*;
    use std::collections::{BTreeSet, HashSet};
    use valqeron_core::identifiers::{Cnpj, CnpjError};

    #[test]
    fn from_str_delegates_to_parse() {
        let a: Cnpj = NUMERIC_ROOT.parse().unwrap();
        let b = Cnpj::parse(NUMERIC_ROOT).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn try_from_str_delegates_to_parse() {
        let a = Cnpj::try_from(NUMERIC_ROOT).unwrap();
        let b = Cnpj::parse(NUMERIC_ROOT).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn try_from_byte_array_delegates_to_from_bytes() {
        let a = Cnpj::try_from(*b"00000000000191").unwrap();
        assert_eq!(a, Cnpj::parse(NUMERIC_ROOT).unwrap());
    }

    #[test]
    fn try_from_byte_slice_validates_length() {
        let good: &[u8] = b"00000000000191";
        assert_eq!(
            Cnpj::try_from(good).unwrap(),
            Cnpj::parse(NUMERIC_ROOT).unwrap()
        );

        let short: &[u8] = b"000000000001";
        assert_eq!(
            Cnpj::try_from(short),
            Err(CnpjError::InvalidLength { found: 12 })
        );
    }

    #[test]
    fn partial_eq_with_str_uses_compact_form() {
        let cnpj = Cnpj::parse(NUMERIC_ROOT).unwrap();
        assert_eq!(cnpj, "00000000000191");
        assert_eq!(cnpj, *"00000000000191");
        assert_eq!("00000000000191", cnpj);
        // The punctuated form is not what `PartialEq<str>` compares against.
        assert_ne!(cnpj, "00.000.000/0001-91");
    }

    #[test]
    fn as_ref_bytes_matches_as_bytes() {
        let cnpj = Cnpj::parse(NUMERIC_ROOT).unwrap();
        let as_ref: &[u8] = cnpj.as_ref();
        assert_eq!(as_ref, cnpj.as_bytes().as_slice());
    }

    #[test]
    fn as_ref_str_matches_as_str() {
        let cnpj = Cnpj::parse(NUMERIC_ROOT).unwrap();
        let as_ref: &str = cnpj.as_ref();
        assert_eq!(as_ref, cnpj.as_str());
    }

    #[test]
    fn is_copy_and_clone() {
        let cnpj = Cnpj::parse(NUMERIC_ROOT).unwrap();
        let copied = cnpj;
        assert_eq!(cnpj, copied);
        assert_eq!(cnpj, cnpj.clone());
    }

    #[test]
    fn ordering_is_lexicographic_over_bytes() {
        let a = Cnpj::parse("00000000000191").unwrap();
        let b = Cnpj::parse("11222333000181").unwrap();
        assert!(a < b);
    }

    #[test]
    fn works_as_a_hashset_key() {
        let mut set = HashSet::new();
        set.insert(Cnpj::parse(NUMERIC_ROOT).unwrap());
        assert!(set.contains(&Cnpj::parse(NUMERIC_ROOT).unwrap()));
    }

    #[test]
    fn works_as_a_btree_set_key() {
        let mut set = BTreeSet::new();
        set.insert(Cnpj::parse(NUMERIC_ROOT).unwrap());
        set.insert(Cnpj::parse(ALPHANUMERIC).unwrap());
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn display_and_debug_are_both_readable() {
        let cnpj = Cnpj::parse(NUMERIC_ROOT).unwrap();
        assert_eq!(cnpj.to_string(), "00.000.000/0001-91");
        assert_eq!(format!("{cnpj:?}"), "Cnpj(\"00.000.000/0001-91\")");
    }
}
