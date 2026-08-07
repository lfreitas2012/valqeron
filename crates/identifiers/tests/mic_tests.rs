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

pub use valqeron_identifiers::{Mic, MicError};

/// The New York Stock Exchange: an active operating MIC in the US.
const SAMPLE: &str = "XNYS";

mod construction {
    use super::{Mic, MicError, SAMPLE};

    #[test]
    fn parse_accepts_canonical_input() {
        assert!(Mic::parse(SAMPLE).is_ok());
    }

    #[test]
    fn parse_accepts_lowercase() {
        assert_eq!(Mic::parse("xnys").unwrap(), Mic::parse(SAMPLE).unwrap());
    }

    #[test]
    fn parse_tolerates_surrounding_whitespace() {
        assert_eq!(Mic::parse("  XNYS ").unwrap(), Mic::parse(SAMPLE).unwrap());
    }

    #[test]
    fn parse_accepts_digits() {
        // MICs may contain digits anywhere, including the first position.
        assert!(Mic::parse("360T").is_ok());
    }

    #[test]
    fn parse_accepts_expired_codes() {
        // `ALDP` (NYSE Alternext Dark) has expired but stays registered forever.
        assert!(Mic::parse("ALDP").is_ok());
    }

    #[test]
    fn parse_accepts_off_exchange_pseudo_mics() {
        for pseudo in ["XOFF", "XXXX", "BILT"] {
            assert!(Mic::parse(pseudo).is_ok(), "{pseudo} should be registered");
        }
    }

    #[test]
    fn new_is_an_alias_for_parse() {
        assert_eq!(Mic::new(SAMPLE), Mic::parse(SAMPLE));
    }

    #[test]
    fn from_bytes_round_trips_with_as_bytes() {
        let mic = Mic::parse(SAMPLE).unwrap();
        let rebuilt = Mic::from_bytes(*mic.as_bytes()).unwrap();
        assert_eq!(mic, rebuilt);
    }

    #[test]
    fn empty_input_is_rejected() {
        assert_eq!(Mic::parse(""), Err(MicError::Empty));
    }

    #[test]
    fn wrong_length_is_rejected() {
        assert_eq!(Mic::parse("XNY"), Err(MicError::InvalidLength { found: 3 }));
        assert_eq!(
            Mic::parse("XNYSE"),
            Err(MicError::InvalidLength { found: 5 })
        );
    }
}

mod accessors {
    use super::{Mic, SAMPLE};

    #[test]
    fn exposes_raw_forms() {
        let mic = Mic::parse(SAMPLE).unwrap();
        assert_eq!(mic.as_str(), "XNYS");
        assert_eq!(mic.as_bytes(), b"XNYS");
    }

    #[test]
    fn reports_lifecycle_state() {
        assert!(Mic::parse(SAMPLE).unwrap().is_active());
        assert!(!Mic::parse("ALDP").unwrap().is_active());
    }

    #[test]
    fn distinguishes_operating_from_segment() {
        let nyse = Mic::parse(SAMPLE).unwrap();
        assert!(nyse.is_operating());
        assert!(!nyse.is_segment());

        let arca = Mic::parse("ARCX").unwrap();
        assert!(arca.is_segment());
        assert!(!arca.is_operating());
    }

    #[test]
    fn operating_mic_of_a_segment_is_its_operator() {
        let arca = Mic::parse("ARCX").unwrap();
        assert_eq!(arca.operating_mic(), Mic::parse(SAMPLE).unwrap());
    }

    #[test]
    fn operating_mic_of_an_operating_mic_is_itself() {
        let nyse = Mic::parse(SAMPLE).unwrap();
        assert_eq!(nyse.operating_mic(), nyse);
    }

    #[test]
    fn operating_mic_of_an_expired_segment_is_the_published_reference() {
        // The registry keeps the operating MIC a segment had at the time on expired rows.
        let expired = Mic::parse("ALDP").unwrap();
        assert_eq!(expired.operating_mic(), Mic::parse(SAMPLE).unwrap());
    }

    #[test]
    fn country_code_is_the_registered_country() {
        let country = Mic::parse(SAMPLE).unwrap().country_code().unwrap();
        assert_eq!(country.as_str(), "US");
    }

    #[test]
    fn country_code_is_none_for_pseudo_mics() {
        // The registry files the off-exchange pseudo-MICs under the `ZZ` placeholder.
        for pseudo in ["XOFF", "XXXX", "BILT"] {
            assert_eq!(
                Mic::parse(pseudo).unwrap().country_code(),
                None,
                "{pseudo} should have no country"
            );
        }
    }
}

mod membership_rejections {
    use super::{Mic, MicError};

    #[test]
    fn well_formed_but_unregistered() {
        assert_eq!(
            Mic::parse("ZZZZ"),
            Err(MicError::Unregistered {
                code: ['Z', 'Z', 'Z', 'Z'],
            })
        );
        assert!(matches!(
            Mic::parse("AAAA"),
            Err(MicError::Unregistered { .. })
        ));
    }

    #[test]
    fn punctuation_is_a_character_error() {
        assert_eq!(
            Mic::parse("XN.S"),
            Err(MicError::InvalidCharacter {
                character: '.',
                position: 3,
            })
        );
    }
}

mod traits {
    use super::{Mic, MicError, SAMPLE};

    #[test]
    fn from_str_matches_parse() {
        let via_from_str: Mic = "XNYS".parse().unwrap();
        assert_eq!(via_from_str, Mic::parse(SAMPLE).unwrap());
    }

    #[test]
    fn try_from_matches_parse() {
        let via_try: Mic = Mic::try_from("XNYS").unwrap();
        assert_eq!(via_try, Mic::parse(SAMPLE).unwrap());
    }

    #[test]
    fn try_from_byte_array_matches_from_bytes() {
        assert_eq!(
            Mic::try_from(*b"XNYS").unwrap(),
            Mic::parse(SAMPLE).unwrap()
        );
    }

    #[test]
    fn try_from_byte_slice_validates_length() {
        let good: &[u8] = b"XNYS";
        assert_eq!(Mic::try_from(good).unwrap(), Mic::parse(SAMPLE).unwrap());

        let bad: &[u8] = b"XNYSE";
        assert_eq!(
            Mic::try_from(bad),
            Err(MicError::InvalidLength { found: 5 })
        );
    }

    #[test]
    fn partial_eq_with_str() {
        let mic = Mic::parse(SAMPLE).unwrap();
        assert_eq!(mic, "XNYS");
        assert_eq!(mic, *"XNYS");
        assert_eq!("XNYS", mic);
        assert_ne!(mic, "XLON");
    }

    #[test]
    fn as_ref_str_and_bytes() {
        let mic = Mic::parse(SAMPLE).unwrap();
        let as_str: &str = mic.as_ref();
        let as_bytes: &[u8] = mic.as_ref();
        assert_eq!(as_str, "XNYS");
        assert_eq!(as_bytes, b"XNYS");
    }

    #[test]
    fn display_is_canonical() {
        assert_eq!(Mic::parse(SAMPLE).unwrap().to_string(), "XNYS");
    }

    #[test]
    fn ordering_is_lexicographic() {
        let mut mics = [
            Mic::parse("XNYS").unwrap(),
            Mic::parse("360T").unwrap(),
            Mic::parse("XLON").unwrap(),
        ];
        mics.sort();
        assert_eq!(mics[0].as_str(), "360T");
        assert_eq!(mics[1].as_str(), "XLON");
        assert_eq!(mics[2].as_str(), "XNYS");
    }
}
