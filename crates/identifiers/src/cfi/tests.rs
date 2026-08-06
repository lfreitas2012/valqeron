use super::{Cfi, CfiError};

const EQUITY: &str = "ESVUFR";

mod construction {
    use super::*;

    #[test]
    fn parse_accepts_canonical_input() {
        assert!(Cfi::parse(EQUITY).is_ok());
    }

    #[test]
    fn parse_accepts_lowercase() {
        assert_eq!(Cfi::parse("esvufr").unwrap(), Cfi::parse(EQUITY).unwrap());
    }

    #[test]
    fn parse_tolerates_surrounding_whitespace() {
        assert_eq!(
            Cfi::parse("  ESVUFR ").unwrap(),
            Cfi::parse(EQUITY).unwrap()
        );
    }

    #[test]
    fn new_is_an_alias_for_parse() {
        assert_eq!(Cfi::new(EQUITY), Cfi::parse(EQUITY));
    }

    #[test]
    fn from_bytes_round_trips_with_as_bytes() {
        let cfi = Cfi::parse(EQUITY).unwrap();
        let rebuilt = Cfi::from_bytes(*cfi.as_bytes()).unwrap();
        assert_eq!(cfi, rebuilt);
    }

    #[test]
    fn empty_input_is_rejected() {
        assert_eq!(Cfi::parse(""), Err(CfiError::Empty));
    }

    #[test]
    fn wrong_length_is_rejected() {
        assert_eq!(
            Cfi::parse("ESVU"),
            Err(CfiError::InvalidLength { found: 4 })
        );
    }
}

mod accessors {
    use super::*;

    #[test]
    fn exposes_segments() {
        let cfi = Cfi::parse(EQUITY).unwrap();
        assert_eq!(cfi.category(), 'E');
        assert_eq!(cfi.group(), 'S');
        assert_eq!(cfi.attributes(), ['V', 'U', 'F', 'R']);
    }

    #[test]
    fn exposes_raw_forms() {
        let cfi = Cfi::parse(EQUITY).unwrap();
        assert_eq!(cfi.as_str(), "ESVUFR");
        assert_eq!(cfi.as_bytes(), b"ESVUFR");
    }
}

mod taxonomy_rejections {
    use super::*;

    #[test]
    fn unknown_category() {
        assert_eq!(
            Cfi::parse("QSVUFR"),
            Err(CfiError::UnknownCategory { code: 'Q' })
        );
    }

    #[test]
    fn unknown_group() {
        assert_eq!(
            Cfi::parse("EZVUFR"),
            Err(CfiError::UnknownGroup {
                category: 'E',
                code: 'Z',
            })
        );
    }

    #[test]
    fn invalid_attribute() {
        assert_eq!(
            Cfi::parse("ESZUFR"),
            Err(CfiError::InvalidAttribute {
                category: 'E',
                group: 'S',
                index: 1,
                code: 'Z',
            })
        );
    }

    #[test]
    fn non_letter_is_a_character_error() {
        assert_eq!(
            Cfi::parse("ESVUF1"),
            Err(CfiError::InvalidCharacter {
                character: '1',
                position: 6,
            })
        );
    }
}

mod traits {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn from_str_matches_parse() {
        let via_from_str: Cfi = "ESVUFR".parse().unwrap();
        assert_eq!(via_from_str, Cfi::parse(EQUITY).unwrap());
    }

    #[test]
    fn try_from_matches_parse() {
        let via_try: Cfi = Cfi::try_from("ESVUFR").unwrap();
        assert_eq!(via_try, Cfi::parse(EQUITY).unwrap());
    }

    #[test]
    fn try_from_byte_array_matches_from_bytes() {
        assert_eq!(
            Cfi::try_from(*b"ESVUFR").unwrap(),
            Cfi::parse(EQUITY).unwrap()
        );
    }

    #[test]
    fn try_from_byte_slice_validates_length() {
        let good: &[u8] = b"ESVUFR";
        assert_eq!(Cfi::try_from(good).unwrap(), Cfi::parse(EQUITY).unwrap());

        let bad: &[u8] = b"ESV";
        assert_eq!(
            Cfi::try_from(bad),
            Err(CfiError::InvalidLength { found: 3 })
        );
    }

    #[test]
    fn partial_eq_with_str() {
        let cfi = Cfi::parse(EQUITY).unwrap();
        assert_eq!(cfi, "ESVUFR");
        assert_eq!(cfi, *"ESVUFR");
        assert_eq!("ESVUFR", cfi);
        assert_ne!(cfi, "DBFTFB");
    }

    #[test]
    fn as_ref_str_and_bytes() {
        let cfi = Cfi::parse(EQUITY).unwrap();
        let as_str: &str = cfi.as_ref();
        let as_bytes: &[u8] = cfi.as_ref();
        assert_eq!(as_str, "ESVUFR");
        assert_eq!(as_bytes, b"ESVUFR");
    }

    #[test]
    fn display_is_canonical() {
        assert_eq!(Cfi::parse(EQUITY).unwrap().to_string(), "ESVUFR");
    }

    #[test]
    fn ordering_is_lexicographic() {
        let mut cfis = [
            Cfi::parse("ESVUFR").unwrap(),
            Cfi::parse("DBFTFB").unwrap(),
            Cfi::parse("ESVUFR").unwrap(),
        ];
        cfis.sort();
        assert_eq!(cfis[0].as_str(), "DBFTFB");
        assert_eq!(cfis[1].as_str(), "ESVUFR");
    }
}
