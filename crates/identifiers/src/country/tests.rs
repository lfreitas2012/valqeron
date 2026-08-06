use super::{CountryCode, CountryCodeError};

const SAMPLE: &str = "US";

mod construction {
    use super::*;

    #[test]
    fn parse_accepts_canonical_input() {
        assert!(CountryCode::parse(SAMPLE).is_ok());
    }

    #[test]
    fn parse_accepts_lowercase() {
        assert_eq!(
            CountryCode::parse("us").unwrap(),
            CountryCode::parse(SAMPLE).unwrap()
        );
    }

    #[test]
    fn parse_tolerates_surrounding_whitespace() {
        assert_eq!(
            CountryCode::parse("  US ").unwrap(),
            CountryCode::parse(SAMPLE).unwrap()
        );
    }

    #[test]
    fn new_is_an_alias_for_parse() {
        assert_eq!(CountryCode::new(SAMPLE), CountryCode::parse(SAMPLE));
    }

    #[test]
    fn from_bytes_round_trips_with_as_bytes() {
        let code = CountryCode::parse(SAMPLE).unwrap();
        let rebuilt = CountryCode::from_bytes(*code.as_bytes()).unwrap();
        assert_eq!(code, rebuilt);
    }

    #[test]
    fn empty_input_is_rejected() {
        assert_eq!(CountryCode::parse(""), Err(CountryCodeError::Empty));
    }

    #[test]
    fn wrong_length_is_rejected() {
        assert_eq!(
            CountryCode::parse("USA"),
            Err(CountryCodeError::InvalidLength { found: 3 })
        );
    }
}

mod accessors {
    use super::*;

    #[test]
    fn exposes_raw_forms() {
        let code = CountryCode::parse(SAMPLE).unwrap();
        assert_eq!(code.as_str(), "US");
        assert_eq!(code.as_bytes(), b"US");
    }
}

mod membership_rejections {
    use super::*;

    #[test]
    fn well_formed_but_unassigned() {
        assert_eq!(
            CountryCode::parse("ZZ"),
            Err(CountryCodeError::Unassigned { code: ['Z', 'Z'] })
        );
    }

    #[test]
    fn reserved_codes_are_unassigned() {
        for reserved in ["EU", "UK", "UN"] {
            assert!(
                matches!(
                    CountryCode::parse(reserved),
                    Err(CountryCodeError::Unassigned { .. })
                ),
                "{reserved} should be treated as unassigned"
            );
        }
    }

    #[test]
    fn non_letter_is_a_character_error() {
        assert_eq!(
            CountryCode::parse("U1"),
            Err(CountryCodeError::InvalidCharacter {
                character: '1',
                position: 2,
            })
        );
    }
}

mod traits {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn from_str_matches_parse() {
        let via_from_str: CountryCode = "US".parse().unwrap();
        assert_eq!(via_from_str, CountryCode::parse(SAMPLE).unwrap());
    }

    #[test]
    fn try_from_matches_parse() {
        let via_try: CountryCode = CountryCode::try_from("US").unwrap();
        assert_eq!(via_try, CountryCode::parse(SAMPLE).unwrap());
    }

    #[test]
    fn try_from_byte_array_matches_from_bytes() {
        assert_eq!(
            CountryCode::try_from(*b"US").unwrap(),
            CountryCode::parse(SAMPLE).unwrap()
        );
    }

    #[test]
    fn try_from_byte_slice_validates_length() {
        let good: &[u8] = b"US";
        assert_eq!(
            CountryCode::try_from(good).unwrap(),
            CountryCode::parse(SAMPLE).unwrap()
        );

        let bad: &[u8] = b"USA";
        assert_eq!(
            CountryCode::try_from(bad),
            Err(CountryCodeError::InvalidLength { found: 3 })
        );
    }

    #[test]
    fn partial_eq_with_str() {
        let code = CountryCode::parse(SAMPLE).unwrap();
        assert_eq!(code, "US");
        assert_eq!(code, *"US");
        assert_eq!("US", code);
        assert_ne!(code, "BR");
    }

    #[test]
    fn as_ref_str_and_bytes() {
        let code = CountryCode::parse(SAMPLE).unwrap();
        let as_str: &str = code.as_ref();
        let as_bytes: &[u8] = code.as_ref();
        assert_eq!(as_str, "US");
        assert_eq!(as_bytes, b"US");
    }

    #[test]
    fn display_is_canonical() {
        assert_eq!(CountryCode::parse(SAMPLE).unwrap().to_string(), "US");
    }

    #[test]
    fn ordering_is_lexicographic() {
        let mut codes = [
            CountryCode::parse("US").unwrap(),
            CountryCode::parse("BR").unwrap(),
            CountryCode::parse("US").unwrap(),
        ];
        codes.sort();
        assert_eq!(codes[0].as_str(), "BR");
        assert_eq!(codes[1].as_str(), "US");
    }
}

mod table {
    use super::super::table::{ASSIGNED, ASSIGNED_CODES};
    use super::*;

    #[test]
    fn holds_exactly_the_expected_number_of_codes() {
        assert_eq!(ASSIGNED_CODES.len(), 249);
    }

    #[test]
    fn bitmap_has_one_bit_per_listed_code() {
        let popcount: u32 = ASSIGNED.iter().map(|word| word.count_ones()).sum();
        assert_eq!(popcount as usize, ASSIGNED_CODES.len());
    }

    #[test]
    fn every_listed_code_parses() {
        for code in ASSIGNED_CODES {
            let s = core::str::from_utf8(code).unwrap();
            assert!(CountryCode::parse(s).is_ok(), "{s} should parse");
        }
    }
}
