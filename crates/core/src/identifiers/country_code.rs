use core::fmt;
use fmt::{Debug, Display};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::str::{FromStr, from_utf8_unchecked};
use std::string::FromUtf8Error;
use thiserror::Error;

const COMBINATIONS: usize = 26 * 26;
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
    pub fn parse(input: &str) -> Result<Self, CountryCodeError> {
        let candidate = normalize(input)?;
        Self::from_bytes(candidate)
    }

    pub fn from_bytes(bytes: [u8; 2]) -> Result<Self, CountryCodeError> {
        validate(&bytes)?;
        Ok(CountryCode { bytes })
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
        let bytes: [u8; 2] = value
            .try_into()
            .map_err(|_| CountryCodeError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
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
        CountryCode::parse(s).map_err(serde::de::Error::custom)
    }
}

fn validate(candidate: &[u8; 2]) -> Result<(), CountryCodeError> {
    validate_character_classes(candidate)?;
    validate_membership(candidate)?;
    Ok(())
}

fn validate_character_classes(candidate: &[u8; 2]) -> Result<(), CountryCodeError> {
    for (i, &byte) in candidate.iter().enumerate() {
        if !byte.is_ascii_uppercase() {
            return Err(CountryCodeError::InvalidCharacter {
                character: byte as char,
                position: (i + 1) as u8,
            });
        }
    }
    Ok(())
}

fn validate_membership(candidate: &[u8; 2]) -> Result<(), CountryCodeError> {
    if is_assigned(candidate) {
        Ok(())
    } else {
        let code = String::from_utf8(vec![candidate[0], candidate[1]])?;
        Err(CountryCodeError::Unassigned { code })
    }
}

#[inline]
fn is_assigned(candidate: &[u8; 2]) -> bool {
    debug_assert!(candidate[0].is_ascii_uppercase() && candidate[1].is_ascii_uppercase());
    let index = bit_index(*candidate);
    (ASSIGNED[index / 64] >> (index % 64)) & 1 == 1
}

#[inline]
const fn bit_index(code: [u8; 2]) -> usize {
    (code[0] - b'A') as usize * 26 + (code[1] - b'A') as usize
}

const fn build_bitmap(codes: &[[u8; 2]]) -> [u64; WORDS] {
    let mut bits = [0u64; WORDS];
    let mut i = 0;
    while i < codes.len() {
        let index = bit_index(codes[i]);
        bits[index / 64] |= 1u64 << (index % 64);
        i += 1;
    }
    bits
}

const _: () = check_table(ASSIGNED_CODES);

const fn check_table(codes: &[[u8; 2]]) {
    let mut i = 0;
    while i < codes.len() {
        let a = codes[i][0];
        let b = codes[i][1];
        assert!(
            a >= b'A' && a <= b'Z' && b >= b'A' && b <= b'Z',
            "every assigned code must be two uppercase ASCII letters"
        );
        if i > 0 {
            let prev_a = codes[i - 1][0];
            let prev_b = codes[i - 1][1];
            assert!(
                prev_a < a || (prev_a == a && prev_b < b),
                "assigned codes must be listed in strictly ascending order"
            );
        }
        i += 1;
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

    let mut buf = [0u8; 2];
    for (i, ch) in trimmed.chars().enumerate() {
        if !ch.is_ascii() {
            return Err(CountryCodeError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
            });
        }
        buf[i] = ch.to_ascii_uppercase() as u8;
    }

    Ok(buf)
}

#[cfg(test)]
mod tests_formating {
    use crate::identifiers::country_code::CountryCode;
    use std::format;
    use std::string::ToString;

    #[test]
    fn display_is_the_canonical_string() {
        let code = CountryCode::parse("US").unwrap();
        assert_eq!(code.to_string(), "US");
    }

    #[test]
    fn debug_is_readable() {
        let code = CountryCode::parse("US").unwrap();
        assert_eq!(format!("{code:?}"), "CountryCode(\"US\")");
    }
}

#[cfg(test)]
mod tests_validation {
    use super::*;

    fn candidate(s: &str) -> [u8; 2] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 2];
        out.copy_from_slice(bytes);
        out
    }

    #[test]
    fn accepts_known_assigned_codes() {
        for s in ["US", "BR", "GB", "DE", "SS", "CW", "AD", "ZW"] {
            assert!(validate(&candidate(s)).is_ok(), "{s} should be valid");
        }
    }

    #[test]
    fn rejects_unassigned_but_well_formed() {
        let err = validate(&candidate("ZZ")).unwrap_err();
        assert_eq!(
            err,
            CountryCodeError::Unassigned {
                code: "ZZ".to_string()
            }
        );
    }

    #[test]
    fn rejects_reserved_codes() {
        // `EU` and `UK` are reserved, not officially assigned, so they are treated as unassigned.
        assert!(matches!(
            validate(&candidate("EU")),
            Err(CountryCodeError::Unassigned { .. })
        ));
        assert!(matches!(
            validate(&candidate("UK")),
            Err(CountryCodeError::Unassigned { .. })
        ));
    }

    #[test]
    fn rejects_lowercase_as_character_class() {
        let err = validate(&candidate("us")).unwrap_err();
        assert_eq!(
            err,
            CountryCodeError::InvalidCharacter {
                character: 'u',
                position: 1,
            }
        );
    }

    #[test]
    fn rejects_digit_as_character_class() {
        let err = validate(&candidate("U1")).unwrap_err();
        assert_eq!(
            err,
            CountryCodeError::InvalidCharacter {
                character: '1',
                position: 2,
            }
        );
    }
}

#[cfg(test)]
mod tests_parser {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize(""), Err(CountryCodeError::Empty));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(normalize("  US "), normalize("US"));
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(normalize("us").unwrap(), *b"US");
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            normalize("USA"),
            Err(CountryCodeError::InvalidLength { found: 3 })
        );
    }

    #[test]
    fn whitespace_only_is_a_length_error() {
        assert_eq!(
            normalize("   "),
            Err(CountryCodeError::InvalidLength { found: 0 })
        );
    }

    #[test]
    fn keeps_non_letter_characters_for_validation() {
        // A non letter that is not surrounding whitespace survives normalization (the count is
        // still two) and is left for validation to reject as an invalid character.
        assert_eq!(normalize("U.").unwrap(), *b"U.");
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("U£").unwrap_err();
        assert!(matches!(
            err,
            CountryCodeError::InvalidCharacter {
                character: '£', ..
            }
        ));
    }
}
