use crate::identifiers::CountryCode;
use crate::identifiers::common::CharacterClass;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::{FromStr, from_utf8_unchecked};
use thiserror::Error;

const BASE_LEN: usize = 11;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed Isin should be used; discarding it wastes the validation work"]
pub struct Isin {
    bytes: [u8; 12],
}

impl Isin {
    pub fn parse(input: &str) -> Result<Self, IsinError> {
        let candidate = normalize(input)?;
        Self::from_bytes(candidate)
    }

    #[inline]
    pub fn new(input: &str) -> Result<Self, IsinError> {
        Self::parse(input)
    }

    pub fn from_bytes(bytes: [u8; 12]) -> Result<Self, IsinError> {
        validate(&bytes)?;
        Ok(Isin { bytes })
    }

    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 12] {
        &self.bytes
    }

    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `Isin::from_bytes` guarantees the bytes are ASCII letters and digits only.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }

    #[inline]
    #[must_use]
    pub fn country_code(&self) -> &str {
        &self.as_str()[0..2]
    }

    #[inline]
    #[must_use]
    pub fn country(&self) -> Option<CountryCode> {
        CountryCode::from_bytes([self.bytes[0], self.bytes[1]]).ok()
    }

    #[inline]
    #[must_use]
    pub fn nsin(&self) -> &str {
        &self.as_str()[2..11]
    }

    #[inline]
    #[must_use]
    pub fn check_digit(&self) -> u8 {
        self.bytes[11] - b'0'
    }

    #[inline]
    #[must_use]
    pub fn computed_check_digit(&self) -> u8 {
        compute_check_digit(&self.bytes[..11])
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IsinError {
    #[error("ISIN code cannot be empty")]
    Empty,

    #[error("ISIN code must contain exactly 12 characters")]
    InvalidLength { found: usize },

    #[error("ISIN code contains an invalid character at position {position}")]
    InvalidCharacter {
        character: char,
        position: u8,
        expected: CharacterClass,
    },

    #[error("ISIN code has an invalid check digit; expected {expected}, found {found}")]
    InvalidCheckDigit { expected: u8, found: u8 },
}

impl FromStr for Isin {
    type Err = IsinError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Isin {
    type Error = IsinError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 12]> for Isin {
    type Error = IsinError;

    fn try_from(value: [u8; 12]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for Isin {
    type Error = IsinError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 12] = value
            .try_into()
            .map_err(|_| IsinError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
    }
}

impl PartialEq<str> for Isin {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Isin {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Isin> for str {
    fn eq(&self, other: &Isin) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Isin> for &str {
    fn eq(&self, other: &Isin) -> bool {
        *self == other.as_str()
    }
}

impl AsRef<[u8]> for Isin {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for Isin {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Isin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Isin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Isin").field(&self.as_str()).finish()
    }
}

impl Serialize for Isin {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Isin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        Isin::parse(s).map_err(serde::de::Error::custom)
    }
}

fn validate(candidate: &[u8; 12]) -> Result<(), IsinError> {
    validate_character_classes(candidate)?;
    validate_check_digit(candidate)?;
    Ok(())
}

fn validate_character_classes(candidate: &[u8; 12]) -> Result<(), IsinError> {
    for (i, &byte) in candidate.iter().enumerate() {
        let (is_valid, expected) = if i < 2 {
            (byte.is_ascii_uppercase(), CharacterClass::Letter)
        } else if i < BASE_LEN {
            (
                byte.is_ascii_digit() || byte.is_ascii_uppercase(),
                CharacterClass::Alphanumeric,
            )
        } else {
            (byte.is_ascii_digit(), CharacterClass::Digit)
        };

        if !is_valid {
            return Err(IsinError::InvalidCharacter {
                character: byte as char,
                position: (i + 1) as u8,
                expected,
            });
        }
    }
    Ok(())
}

fn validate_check_digit(candidate: &[u8; 12]) -> Result<(), IsinError> {
    let expected = compute_check_digit(&candidate[..BASE_LEN]);
    // Character-class validation above guarantees `candidate[11]` is an ASCII digit.
    let found = candidate[BASE_LEN] - b'0';
    if expected != found {
        return Err(IsinError::InvalidCheckDigit { expected, found });
    }
    Ok(())
}

fn compute_check_digit(base: &[u8]) -> u8 {
    debug_assert_eq!(base.len(), BASE_LEN);

    let mut sum = 0u32;
    let mut double = true;

    for &c in base.iter().rev() {
        if c.is_ascii_digit() {
            sum += luhn_step((c - b'0') as u32, double);
            double = !double;
        } else {
            // 'A' => 10, ..., 'Z' => 35; split into tens and units.
            let value = (c - b'A' + 10) as u32;
            // Units is the rightmost digit of the expanded pair, so it is processed first.
            sum += luhn_step(value % 10, double);
            double = !double;
            sum += luhn_step(value / 10, double);
            double = !double;
        }
    }

    ((10 - (sum % 10)) % 10) as u8
}

fn normalize(input: &str) -> Result<[u8; 12], IsinError> {
    if input.is_empty() {
        return Err(IsinError::Empty);
    }

    let trimmed = input.trim();
    let found = trimmed.chars().count();
    if found != 12 {
        return Err(IsinError::InvalidLength { found });
    }

    let mut buf = [0u8; 12];
    for (i, ch) in trimmed.chars().enumerate() {
        if !ch.is_ascii() {
            let expected = match i {
                0 | 1 => CharacterClass::Letter,
                2..=10 => CharacterClass::Alphanumeric,
                11 => CharacterClass::Digit,
                _ => unreachable!(),
            };
            return Err(IsinError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
                expected,
            });
        }
        buf[i] = ch.to_ascii_uppercase() as u8;
    }

    Ok(buf)
}

#[inline]
fn luhn_step(value: u32, double: bool) -> u32 {
    if double {
        let doubled = value * 2;
        if doubled > 9 { doubled - 9 } else { doubled }
    } else {
        value
    }
}

#[cfg(test)]
mod tests_formating {
    use crate::identifiers::isin::Isin;
    use std::format;
    use std::string::ToString;

    #[test]
    fn display_is_the_canonical_string() {
        let isin = Isin::parse("US0378331005").unwrap();
        assert_eq!(isin.to_string(), "US0378331005");
    }

    #[test]
    fn debug_is_readable() {
        let isin = Isin::parse("US0378331005").unwrap();
        assert_eq!(format!("{isin:?}"), "Isin(\"US0378331005\")");
    }
}

#[cfg(test)]
mod tests_validation {
    use super::*;

    fn candidate(s: &str) -> [u8; 12] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 12];
        out.copy_from_slice(bytes);
        out
    }

    /// A second, deliberately naive Luhn implementation used to cross-check
    /// [`compute_check_digit`]: it materializes the full expanded digit buffer instead of doing a
    /// single reverse pass.
    fn reference_check_digit(base: &str) -> u8 {
        let mut digits = std::vec::Vec::new();
        for &c in base.as_bytes() {
            if c.is_ascii_digit() {
                digits.push((c - b'0') as u32);
            } else {
                let v = (c - b'A' + 10) as u32;
                digits.push(v / 10);
                digits.push(v % 10);
            }
        }
        // The check digit will be appended to the right, so the rightmost base digit is doubled.
        let mut sum = 0u32;
        let n = digits.len();
        for (i, &d) in digits.iter().enumerate() {
            let from_right = n - i; // 1-based position of this digit once the check digit exists
            let mut v = d;
            if from_right % 2 == 1 {
                v *= 2;
                if v > 9 {
                    v -= 9;
                }
            }
            sum += v;
        }
        ((10 - (sum % 10)) % 10) as u8
    }

    #[test]
    fn accepts_known_real_world_isins() {
        for s in [
            "US0378331005", // Apple
            "US0231351067", // Amazon
            "BRPETRACNOR9", // Petrobras ON
            "GB0002634946", // UK gilt
            "DE0001102333", // German Bund
            "JP3633400001", // Japanese equity
            "AU000000BHP4", // BHP
            "CH0012221716", // Nestlé
        ] {
            assert!(validate(&candidate(s)).is_ok(), "{s} should be valid");
        }
    }

    #[test]
    fn computes_the_documented_apple_check_digit() {
        assert_eq!(compute_check_digit(b"US037833100"), 5);
    }

    #[test]
    fn computes_an_all_letter_nsin_check_digit() {
        assert_eq!(compute_check_digit(b"BRPETRACNOR"), 9);
    }

    #[test]
    fn single_pass_matches_the_reference_implementation() {
        for base in [
            "US037833100",
            "US023135106",
            "BRPETRACNOR",
            "GB000263494",
            "AU000000BHP",
            "AA000000000",
            "ZZZZZZZZZZZ",
        ] {
            assert_eq!(
                compute_check_digit(base.as_bytes()),
                reference_check_digit(base),
                "{base}"
            );
        }
    }

    #[test]
    fn rejects_lowercase_country_code() {
        let err = validate(&candidate("uS0378331005")).unwrap_err();
        assert_eq!(
            err,
            IsinError::InvalidCharacter {
                character: 'u',
                position: 1,
                expected: CharacterClass::Letter,
            }
        );
    }

    #[test]
    fn rejects_digit_in_country_code() {
        let err = validate(&candidate("1S0378331005")).unwrap_err();
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
    fn rejects_letter_in_check_digit_position() {
        let err = validate(&candidate("US037833100X")).unwrap_err();
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
    fn rejects_wrong_check_digit() {
        let err = validate(&candidate("US0378331006")).unwrap_err();
        assert_eq!(
            err,
            IsinError::InvalidCheckDigit {
                expected: 5,
                found: 6,
            }
        );
    }

    #[test]
    fn rejects_adjacent_transposition() {
        // Luhn catches most single adjacent transpositions.
        assert!(validate(&candidate("US3078331005")).is_err());
    }
}

#[cfg(test)]
mod tests_parser {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize(""), Err(IsinError::Empty));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(normalize("  US0378331005 "), normalize("US0378331005"));
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(normalize("us0378331005").unwrap(), *b"US0378331005");
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            normalize("US037833100"),
            Err(IsinError::InvalidLength { found: 11 })
        );
    }

    #[test]
    fn whitespace_only_is_a_length_error() {
        assert_eq!(normalize("   "), Err(IsinError::InvalidLength { found: 0 }));
    }

    #[test]
    fn keeps_interior_characters_for_validation() {
        // An interior space survives normalization (count is still 12) and is left for
        // `validation` to reject as a non-alphanumeric character.
        assert_eq!(normalize("US 378331005").unwrap(), *b"US 378331005");
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("US03783310£5").unwrap_err();
        assert!(matches!(
            err,
            IsinError::InvalidCharacter {
                character: '£',
                position: 11,
                expected: CharacterClass::Alphanumeric,
            }
        ));
    }

    #[test]
    fn reports_correct_expected_class_for_non_ascii() {
        // Position 1 (Letter expected)
        let err1 = normalize("£S0378331005").unwrap_err();
        assert_eq!(
            err1,
            IsinError::InvalidCharacter {
                character: '£',
                position: 1,
                expected: CharacterClass::Letter,
            }
        );

        // Position 12 (Digit expected)
        let err12 = normalize("US037833100£").unwrap_err();
        assert_eq!(
            err12,
            IsinError::InvalidCharacter {
                character: '£',
                position: 12,
                expected: CharacterClass::Digit,
            }
        );
    }
}
