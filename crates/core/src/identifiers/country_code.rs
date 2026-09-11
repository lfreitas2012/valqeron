use core::fmt;
use fmt::{Debug, Display};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::str::{FromStr, from_utf8_unchecked};
use std::string::FromUtf8Error;
use thiserror::Error;

const COMBINATIONS: usize = 26usize.pow(2);
const WORDS: usize = COMBINATIONS.div_ceil(64);
const ASSIGNED_CODES: &[[u8; 2]] = &[
    *b"AD", *b"AE", *b"AF", *b"AG", *b"AI", *b"AL", *b"AM", *b"AO", *b"AQ", *b"AR", //
    *b"AS", *b"AT", *b"AU", *b"AW", *b"AX", *b"AZ", *b"BA", *b"BB", *b"BD", *b"BE", //
    *b"BF", *b"BG", *b"BH", *b"BI", *b"BJ", *b"BL", *b"BM", *b"BN", *b"BO", *b"BQ", //
    *b"BR", *b"BS", *b"BT", *b"BV", *b"BW", *b"BY", *b"BZ", *b"CA", *b"CC", *b"CD", //
    *b"CF", *b"CG", *b"CH", *b"CI", *b"CK", *b"CL", *b"CM", *b"CN", *b"CO", *b"CR", //
    *b"CU", *b"CV", *b"CW", *b"CX", *b"CY", *b"CZ", *b"DE", *b"DJ", *b"DK", *b"DM", //
    *b"DO", *b"DZ", *b"EC", *b"EE", *b"EG", *b"EH", *b"ER", *b"ES", *b"ET", *b"FI", //
    *b"FJ", *b"FK", *b"FM", *b"FO", *b"FR", *b"GA", *b"GB", *b"GD", *b"GE", *b"GF", //
    *b"GG", *b"GH", *b"GI", *b"GL", *b"GM", *b"GN", *b"GP", *b"GQ", *b"GR", *b"GS", //
    *b"GT", *b"GU", *b"GW", *b"GY", *b"HK", *b"HM", *b"HN", *b"HR", *b"HT", *b"HU", //
    *b"ID", *b"IE", *b"IL", *b"IM", *b"IN", *b"IO", *b"IQ", *b"IR", *b"IS", *b"IT", //
    *b"JE", *b"JM", *b"JO", *b"JP", *b"KE", *b"KG", *b"KH", *b"KI", *b"KM", *b"KN", //
    *b"KP", *b"KR", *b"KW", *b"KY", *b"KZ", *b"LA", *b"LB", *b"LC", *b"LI", *b"LK", //
    *b"LR", *b"LS", *b"LT", *b"LU", *b"LV", *b"LY", *b"MA", *b"MC", *b"MD", *b"ME", //
    *b"MF", *b"MG", *b"MH", *b"MK", *b"ML", *b"MM", *b"MN", *b"MO", *b"MP", *b"MQ", //
    *b"MR", *b"MS", *b"MT", *b"MU", *b"MV", *b"MW", *b"MX", *b"MY", *b"MZ", *b"NA", //
    *b"NC", *b"NE", *b"NF", *b"NG", *b"NI", *b"NL", *b"NO", *b"NP", *b"NR", *b"NU", //
    *b"NZ", *b"OM", *b"PA", *b"PE", *b"PF", *b"PG", *b"PH", *b"PK", *b"PL", *b"PM", //
    *b"PN", *b"PR", *b"PS", *b"PT", *b"PW", *b"PY", *b"QA", *b"RE", *b"RO", *b"RS", //
    *b"RU", *b"RW", *b"SA", *b"SB", *b"SC", *b"SD", *b"SE", *b"SG", *b"SH", *b"SI", //
    *b"SJ", *b"SK", *b"SL", *b"SM", *b"SN", *b"SO", *b"SR", *b"SS", *b"ST", *b"SV", //
    *b"SX", *b"SY", *b"SZ", *b"TC", *b"TD", *b"TF", *b"TG", *b"TH", *b"TJ", *b"TK", //
    *b"TL", *b"TM", *b"TN", *b"TO", *b"TR", *b"TT", *b"TV", *b"TW", *b"TZ", *b"UA", //
    *b"UG", *b"UM", *b"US", *b"UY", *b"UZ", *b"VA", *b"VC", *b"VE", *b"VG", *b"VI", //
    *b"VN", *b"VU", *b"WF", *b"WS", *b"YE", *b"YT", *b"ZA", *b"ZM", *b"ZW",
];

const ASSIGNED: [u64; WORDS] = build_bitmap(ASSIGNED_CODES);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed CountryCode should be used; discarding it wastes the validation work"]
pub struct CountryCode {
    bytes: [u8; 2],
}

impl CountryCode {
    /// # Errors
    ///
    /// Returns [`CountryCodeError`] if the input is empty, is not exactly
    /// two characters long, contains a character that isn't an uppercase
    /// ASCII letter, or is well-formed but not assigned by ISO 3166-1.
    pub fn parse(input: &str) -> Result<Self, CountryCodeError> {
        let candidate = normalize(input)?;
        Self::from_bytes(candidate)
    }

    /// # Errors
    ///
    /// See [`CountryCode::parse`].
    pub fn from_bytes(bytes: [u8; 2]) -> Result<Self, CountryCodeError> {
        validate(bytes)?;
        Ok(Self { bytes })
    }

    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 2] {
        &self.bytes
    }

    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `CountryCode::from_bytes` guarantees both bytes are uppercase ASCII letters.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CountryCodeError {
    #[error("country code cannot be empty")]
    Empty,

    #[error("country code must be two characters, found {found}")]
    InvalidLength { found: usize },

    #[error(
        "invalid character '{character}' at position {position} of 2: expected an uppercase letter (A-Z) ASCII character"
    )]
    InvalidCharacter { character: char, position: u8 },

    #[error("country code '{code}' is not assigned by ISO 3166-1")]
    Unassigned { code: String },

    #[error(transparent)]
    FromUtf8Error(#[from] FromUtf8Error),
}

impl FromStr for CountryCode {
    type Err = CountryCodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for CountryCode {
    type Error = CountryCodeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 2]> for CountryCode {
    type Error = CountryCodeError;

    fn try_from(value: [u8; 2]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for CountryCode {
    type Error = CountryCodeError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value {
            [first, second] => Self::from_bytes([*first, *second]),
            _ => Err(CountryCodeError::InvalidLength { found: value.len() }),
        }
    }
}

impl PartialEq<str> for CountryCode {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for CountryCode {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<CountryCode> for str {
    fn eq(&self, other: &CountryCode) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<CountryCode> for &str {
    fn eq(&self, other: &CountryCode) -> bool {
        *self == other.as_str()
    }
}

impl AsRef<[u8]> for CountryCode {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for CountryCode {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for CountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Debug for CountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CountryCode").field(&self.as_str()).finish()
    }
}

impl Serialize for CountryCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CountryCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        Self::parse(s).map_err(serde::de::Error::custom)
    }
}

fn validate(candidate: [u8; 2]) -> Result<(), CountryCodeError> {
    validate_character_classes(candidate)?;
    validate_membership(candidate)
}

fn validate_character_classes(candidate: [u8; 2]) -> Result<(), CountryCodeError> {
    let [first, second] = candidate;
    validate_character(first, 1)?;
    validate_character(second, 2)?;
    Ok(())
}

fn validate_character(byte: u8, position: u8) -> Result<(), CountryCodeError> {
    if byte.is_ascii_uppercase() {
        Ok(())
    } else {
        Err(CountryCodeError::InvalidCharacter {
            character: char::from(byte),
            position,
        })
    }
}

fn validate_membership(candidate: [u8; 2]) -> Result<(), CountryCodeError> {
    let [first, second] = candidate;

    if is_assigned(candidate) {
        Ok(())
    } else {
        let code = String::from_utf8([first, second].to_vec())?;
        Err(CountryCodeError::Unassigned { code })
    }
}

#[inline]
fn is_assigned(candidate: [u8; 2]) -> bool {
    let [first, second] = candidate;
    debug_assert!(first.is_ascii_uppercase() && second.is_ascii_uppercase());

    let index = bit_index([first, second]);
    let word_index = index / 64;
    let bit = index % 64;

    ASSIGNED
        .get(word_index)
        .is_some_and(|word| (word >> bit) & 1 == 1)
}

#[inline]
#[allow(clippy::as_conversions)]
const fn bit_index(code: [u8; 2]) -> usize {
    let first = code[0].wrapping_sub(b'A') as usize;
    let second = code[1].wrapping_sub(b'A') as usize;
    first.wrapping_mul(26).wrapping_add(second)
}

#[allow(clippy::indexing_slicing)]
const fn build_bitmap(codes: &[[u8; 2]]) -> [u64; WORDS] {
    let mut bits = [0u64; WORDS];
    let mut i = 0;

    while i < codes.len() {
        let index = bit_index(codes[i]);
        bits[index / 64] |= 1u64 << (index % 64);
        i = i.wrapping_add(1);
    }

    bits
}

const _: () = check_table(ASSIGNED_CODES);

#[allow(clippy::indexing_slicing)]
const fn check_table(codes: &[[u8; 2]]) {
    let mut previous: Option<[u8; 2]> = None;
    let mut i = 0;

    while i < codes.len() {
        let [a, b] = codes[i];

        assert!(
            a.is_ascii_uppercase() && b.is_ascii_uppercase(),
            "every assigned code must be two uppercase ASCII letters"
        );

        if let Some([prev_a, prev_b]) = previous {
            assert!(
                prev_a < a || (prev_a == a && prev_b < b),
                "assigned codes must be listed in strictly ascending order"
            );
        }

        previous = Some([a, b]);
        i = i.wrapping_add(1);
    }
}

fn normalize(input: &str) -> Result<[u8; 2], CountryCodeError> {
    if input.is_empty() {
        return Err(CountryCodeError::Empty);
    }

    let trimmed = input.trim();
    let found = trimmed.chars().count();
    if found != 2 {
        return Err(CountryCodeError::InvalidLength { found });
    }

    let mut chars = trimmed.chars();
    let first = chars
        .next()
        .ok_or(CountryCodeError::InvalidLength { found: 0 })?;
    let second = chars
        .next()
        .ok_or(CountryCodeError::InvalidLength { found: 1 })?;

    Ok([
        normalize_character(first, 1)?,
        normalize_character(second, 2)?,
    ])
}

fn normalize_character(ch: char, position: u8) -> Result<u8, CountryCodeError> {
    if !ch.is_ascii() {
        return Err(CountryCodeError::InvalidCharacter {
            character: ch,
            position,
        });
    }

    u8::try_from(ch.to_ascii_uppercase()).map_err(|_| CountryCodeError::InvalidCharacter {
        character: ch,
        position,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::as_conversions)]
mod tests {
    use super::*;

    mod formatting {
        use super::*;

        #[test]
        fn display_is_the_canonical_string() {
            assert_eq!(
                CountryCode::parse("US").map(|code| code.to_string()),
                Ok("US".to_string())
            );
        }

        #[test]
        fn debug_is_readable() {
            assert_eq!(
                CountryCode::parse("US").map(|code| format!("{code:?}")),
                Ok("CountryCode(\"US\")".to_string())
            );
        }

        #[test]
        fn as_str_and_as_bytes_are_canonical() {
            let result = CountryCode::parse("US");

            assert_eq!(result.as_ref().map(CountryCode::as_str), Ok("US"));
            assert_eq!(result.as_ref().map(CountryCode::as_bytes), Ok(b"US"));
        }

        #[test]
        fn as_ref_implementations_match_accessors() {
            let result = CountryCode::parse("US");

            assert_eq!(
                result.as_ref().map(<CountryCode as AsRef<[u8]>>::as_ref),
                Ok(b"US" as &[u8])
            );
            assert_eq!(
                result.as_ref().map(<CountryCode as AsRef<str>>::as_ref),
                Ok("US")
            );
        }

        #[test]
        fn string_equality_is_symmetric() {
            let result = CountryCode::parse("US");

            assert_eq!(result.as_ref().map(|code| code == "US"), Ok(true));
            assert_eq!(result.as_ref().map(|code| "US" == *code), Ok(true));
            assert_eq!(result.as_ref().map(|code| code != "GB"), Ok(true));
            assert_eq!(result.as_ref().map(|code| "GB" != *code), Ok(true));
        }

        #[test]
        fn error_messages_are_human_readable() {
            assert_eq!(
                CountryCodeError::Empty.to_string(),
                "country code cannot be empty"
            );
            assert_eq!(
                CountryCodeError::Unassigned {
                    code: "ZZ".to_string()
                }
                .to_string(),
                "country code 'ZZ' is not assigned by ISO 3166-1"
            );
        }
    }

    mod validation {
        use super::*;

        #[test]
        fn every_assigned_code_is_accepted_by_the_bitmap() {
            for code in ASSIGNED_CODES {
                assert!(validate(*code).is_ok(), "{code:?} should be valid");
            }
        }

        #[test]
        fn accepts_representative_assigned_codes() {
            for code in [
                *b"US", *b"BR", *b"GB", *b"DE", *b"SS", *b"CW", *b"AD", *b"ZW",
            ] {
                assert!(validate(code).is_ok(), "{code:?} should be valid");
            }
        }

        #[test]
        fn rejects_unassigned_but_well_formed_codes() {
            for code in [*b"AA", *b"EU", *b"UK", *b"ZZ"] {
                assert!(matches!(
                    validate(code),
                    Err(CountryCodeError::Unassigned { .. })
                ));
            }
        }

        #[test]
        fn reports_the_first_invalid_character_position() {
            assert_eq!(
                validate(*b"us"),
                Err(CountryCodeError::InvalidCharacter {
                    character: 'u',
                    position: 1,
                })
            );

            assert_eq!(
                validate(*b"U1"),
                Err(CountryCodeError::InvalidCharacter {
                    character: '1',
                    position: 2,
                })
            );
        }

        #[test]
        fn rejects_non_ascii_bytes_before_membership_lookup() {
            assert_eq!(
                validate(*b"\x80A"),
                Err(CountryCodeError::InvalidCharacter {
                    character: '\u{80}',
                    position: 1,
                })
            );
        }

        #[test]
        fn bitmap_matches_a_linear_scan_over_every_possible_code() {
            for first in b'A'..=b'Z' {
                for second in b'A'..=b'Z' {
                    let code = [first, second];
                    let expected = ASSIGNED_CODES.contains(&code);
                    assert_eq!(is_assigned(code), expected, "{code:?}");
                }
            }
        }

        #[test]
        fn try_from_slice_rejects_wrong_length() {
            assert_eq!(
                CountryCode::try_from(b"USA" as &[u8]),
                Err(CountryCodeError::InvalidLength { found: 3 })
            );
        }

        #[test]
        fn try_from_array_matches_from_bytes() {
            assert_eq!(
                CountryCode::try_from(*b"US"),
                CountryCode::from_bytes(*b"US")
            );
        }
    }

    mod parsing {
        use super::*;

        #[test]
        fn rejects_empty() {
            assert_eq!(normalize(""), Err(CountryCodeError::Empty));
            assert_eq!(CountryCode::parse(""), Err(CountryCodeError::Empty));
        }

        #[test]
        fn trims_surrounding_whitespace() {
            assert_eq!(normalize("  US "), normalize("US"));
            assert_eq!(
                CountryCode::parse("  US ").map(|code| code.to_string()),
                Ok("US".to_string())
            );
        }

        #[test]
        fn uppercases_letters() {
            assert_eq!(normalize("us"), Ok(*b"US"));
            assert_eq!(
                CountryCode::parse("br").map(|code| code.to_string()),
                Ok("BR".to_string())
            );
        }

        #[test]
        fn preserves_current_length_error_semantics() {
            assert_eq!(
                normalize("USA"),
                Err(CountryCodeError::InvalidLength { found: 3 })
            );

            assert_eq!(
                normalize("   "),
                Err(CountryCodeError::InvalidLength { found: 0 })
            );
        }

        #[test]
        fn non_letter_ascii_is_rejected_by_validation() {
            assert_eq!(normalize("U."), Ok(*b"U."));
            assert_eq!(
                CountryCode::parse("U."),
                Err(CountryCodeError::InvalidCharacter {
                    character: '.',
                    position: 2,
                })
            );
        }

        #[test]
        fn rejects_non_ascii_input_with_the_original_character() {
            assert_eq!(
                CountryCode::parse("U£"),
                Err(CountryCodeError::InvalidCharacter {
                    character: '£',
                    position: 2,
                })
            );
        }

        #[test]
        fn supports_from_str_and_try_from_string() {
            let from_str = "US".parse::<CountryCode>();
            let try_from = CountryCode::try_from("US");

            assert_eq!(
                from_str.map(|code| code.to_string()),
                try_from.map(|code| code.to_string())
            );
        }

        #[test]
        fn supports_try_from_fixed_array() {
            assert_eq!(
                CountryCode::try_from(*b"US").map(|code| code.to_string()),
                Ok("US".to_string())
            );

            assert_eq!(
                CountryCode::try_from(*b"ZZ"),
                Err(CountryCodeError::Unassigned {
                    code: "ZZ".to_string()
                })
            );
        }

        #[test]
        fn supports_try_from_slice_and_reports_slice_length() {
            assert_eq!(
                CountryCode::try_from(b"US" as &[u8]).map(|code| code.to_string()),
                Ok("US".to_string())
            );

            assert_eq!(
                CountryCode::try_from(b"USA" as &[u8]),
                Err(CountryCodeError::InvalidLength { found: 3 })
            );

            assert_eq!(
                CountryCode::try_from(b"" as &[u8]),
                Err(CountryCodeError::InvalidLength { found: 0 })
            );
        }
    }

    mod comparisons {
        use super::*;
        use std::collections::HashSet;

        #[test]
        fn orders_the_same_as_the_underlying_bytes() {
            let br = CountryCode::parse("BR").unwrap();
            let us = CountryCode::parse("US").unwrap();
            assert!(br < us);
        }

        #[test]
        fn hash_is_consistent_with_equality() {
            let a = CountryCode::parse("US").unwrap();
            let b = CountryCode::parse("us").unwrap(); // normalizes to the same bytes
            let mut set = HashSet::new();
            set.insert(a);
            assert!(
                !set.insert(b),
                "equal CountryCodes must hash to the same bucket"
            );
        }
    }
}
