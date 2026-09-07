use crate::identifiers::common::CharacterClass;
use core::convert::TryFrom;
use core::str::{FromStr, from_utf8_unchecked};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use thiserror::Error;

const BASE_LEN: usize = 18;
const ALPHANUMERIC: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed Lei should be used; discarding it wastes the validation work"]
pub struct Lei {
    bytes: [u8; 20],
}

impl Lei {
    pub fn parse(input: &str) -> Result<Self, LeiError> {
        let candidate = normalize(input)?;
        Self::from_bytes(candidate)
    }

    #[inline]
    pub fn new(input: &str) -> Result<Self, LeiError> {
        Self::parse(input)
    }

    pub fn from_bytes(bytes: [u8; 20]) -> Result<Self, LeiError> {
        validate(&bytes)?;
        Ok(Lei { bytes })
    }

    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.bytes
    }

    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `Lei::from_bytes` guarantees the bytes are ASCII letters and digits only.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }

    #[inline]
    #[must_use]
    pub fn lou_prefix(&self) -> &str {
        &self.as_str()[0..4]
    }

    #[inline]
    #[must_use]
    pub fn entity_id(&self) -> &str {
        &self.as_str()[4..18]
    }

    #[inline]
    #[must_use]
    pub fn check_digits(&self) -> u8 {
        (self.bytes[18] - b'0') * 10 + (self.bytes[19] - b'0')
    }

    #[inline]
    #[must_use]
    pub fn computed_check_digits(&self) -> u8 {
        compute_check_digits(
            self.bytes[..18]
                .try_into()
                .expect("a Lei always has 18 base characters"),
        )
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LeiError {
    #[error("LEI cannot be empty")]
    Empty,

    #[error("LEI must be 20 characters long, found {found}")]
    InvalidLength { found: usize },

    #[error(
        "LEI must contain only alphanumeric characters, found '{character}' at position {position}. Expected {expected}"
    )]
    InvalidCharacter {
        character: char,
        position: u8,
        expected: CharacterClass,
    },

    #[error("LEI must have valid check digits, found {found}, expected {expected}")]
    InvalidCheckDigits { expected: u8, found: u8 },
}

impl FromStr for Lei {
    type Err = LeiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Lei {
    type Error = LeiError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 20]> for Lei {
    type Error = LeiError;

    fn try_from(value: [u8; 20]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for Lei {
    type Error = LeiError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 20] = value
            .try_into()
            .map_err(|_| LeiError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
    }
}

impl PartialEq<str> for Lei {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Lei {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Lei> for str {
    fn eq(&self, other: &Lei) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Lei> for &str {
    fn eq(&self, other: &Lei) -> bool {
        *self == other.as_str()
    }
}

impl AsRef<[u8]> for Lei {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for Lei {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Serialize for Lei {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Lei {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        Lei::parse(s).map_err(serde::de::Error::custom)
    }
}

fn validate(candidate: &[u8; 20]) -> Result<(), LeiError> {
    validate_character_classes(candidate)?;
    validate_check_digits(candidate)?;
    Ok(())
}

fn validate_character_classes(candidate: &[u8; 20]) -> Result<(), LeiError> {
    for (i, &byte) in candidate.iter().enumerate() {
        let (is_valid, expected) = if i < BASE_LEN {
            (
                byte.is_ascii_digit() || byte.is_ascii_uppercase(),
                CharacterClass::Alphanumeric,
            )
        } else {
            (byte.is_ascii_digit(), CharacterClass::Digit)
        };

        if !is_valid {
            return Err(LeiError::InvalidCharacter {
                character: byte as char,
                position: (i + 1) as u8,
                expected,
            });
        }
    }
    Ok(())
}

fn validate_check_digits(candidate: &[u8; 20]) -> Result<(), LeiError> {
    let expected = compute_check_digits(
        candidate[..BASE_LEN]
            .try_into()
            .expect("BASE_LEN bytes precede the check digits"),
    );
    // Character-class validation above guarantees the final two bytes are ASCII digits.
    let found = (candidate[BASE_LEN] - b'0') * 10 + (candidate[BASE_LEN + 1] - b'0');
    if expected != found {
        return Err(LeiError::InvalidCheckDigits { expected, found });
    }
    Ok(())
}

fn compute_check_digits(base: &[u8; BASE_LEN]) -> u8 {
    // Fold the base, then the two placeholder '0' characters, modulo 97.
    let mut rem = fold_mod_97(0, base);
    rem = (rem * 100) % 97; // equivalent to folding "00"
    (98 - rem) as u8
}

#[inline]
fn fold_mod_97(mut rem: u32, bytes: &[u8]) -> u32 {
    for &c in bytes {
        if c.is_ascii_digit() {
            rem = (rem * 10 + (c - b'0') as u32) % 97;
        } else {
            // 'A' => 10, ..., 'Z' => 35.
            let value = (c - b'A' + 10) as u32;
            rem = (rem * 100 + value) % 97;
        }
    }
    rem
}

#[cfg(test)]
fn residue(candidate: &[u8; 20]) -> u32 {
    fold_mod_97(0, candidate)
}

fn build_valid_lei_bytes(base_indices: &[usize; BASE_LEN]) -> [u8; 20] {
    let mut base = [0u8; BASE_LEN];
    for (slot, idx) in base.iter_mut().zip(base_indices) {
        *slot = ALPHANUMERIC[*idx];
    }

    let check = compute_check_digits(&base);
    let mut bytes = [0u8; 20];
    bytes[..BASE_LEN].copy_from_slice(&base);
    bytes[BASE_LEN] = b'0' + check / 10;
    bytes[BASE_LEN + 1] = b'0' + check % 10;
    bytes
}

impl fmt::Display for Lei {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Lei {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Lei").field(&self.as_str()).finish()
    }
}

fn normalize(input: &str) -> Result<[u8; 20], LeiError> {
    if input.is_empty() {
        return Err(LeiError::Empty);
    }

    let trimmed = input.trim();
    let found = trimmed.chars().count();
    if found != 20 {
        return Err(LeiError::InvalidLength { found });
    }

    let mut buf = [0u8; 20];
    for (i, ch) in trimmed.chars().enumerate() {
        if !ch.is_ascii() {
            let expected = if i < 18 {
                CharacterClass::Alphanumeric
            } else {
                CharacterClass::Digit
            };

            return Err(LeiError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
                expected,
            });
        }
        buf[i] = ch.to_ascii_uppercase() as u8;
    }

    Ok(buf)
}

#[cfg(test)]
mod tests_parser {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize(""), Err(LeiError::Empty));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(
            normalize("  5493000IBP32UQZ0KL24 "),
            normalize("5493000IBP32UQZ0KL24")
        );
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(
            normalize("5493000ibp32uqz0kl24").unwrap(),
            *b"5493000IBP32UQZ0KL24"
        );
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            normalize("5493000IBP32UQZ0KL2"),
            Err(LeiError::InvalidLength { found: 19 })
        );
    }

    #[test]
    fn whitespace_only_is_a_length_error() {
        assert_eq!(normalize("   "), Err(LeiError::InvalidLength { found: 0 }));
    }

    #[test]
    fn keeps_interior_characters_for_validation() {
        // An interior space survives normalization (count is still 20) and is left for
        // `validation` to reject as a non-alphanumeric character.
        assert_eq!(
            normalize("5493000IBP32UQZ0K 24"),
            Ok(*b"5493000IBP32UQZ0K 24")
        );
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("5493000IBP32UQZ0KL2£").unwrap_err();
        assert!(matches!(
            err,
            LeiError::InvalidCharacter {
                character: '£', ..
            }
        ));
    }
}

#[cfg(test)]
mod tests_formating {
    use crate::identifiers::lei::Lei;
    use std::format;
    use std::string::ToString;

    #[test]
    fn display_is_the_canonical_string() {
        let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
        assert_eq!(lei.to_string(), "5493000IBP32UQZ0KL24");
    }

    #[test]
    fn debug_is_readable() {
        let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
        assert_eq!(format!("{lei:?}"), "Lei(\"5493000IBP32UQZ0KL24\")");
    }
}

#[cfg(test)]
mod tests_validation {
    use super::*;

    fn candidate(s: &str) -> [u8; 20] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 20];
        out.copy_from_slice(bytes);
        out
    }

    fn reference_check_digits(base: &str) -> u8 {
        let mut expanded = std::string::String::new();
        for &c in base.as_bytes() {
            if c.is_ascii_digit() {
                expanded.push(c as char);
            } else {
                let v = c - b'A' + 10;
                expanded.push((b'0' + v / 10) as char);
                expanded.push((b'0' + v % 10) as char);
            }
        }
        expanded.push('0');
        expanded.push('0');

        let mut rem = 0u32;
        for ch in expanded.chars() {
            rem = (rem * 10 + ch.to_digit(10).unwrap()) % 97;
        }
        (98 - rem) as u8
    }

    #[test]
    fn accepts_known_real_world_leis() {
        // Each verified to satisfy `residue == 1` before being committed here.
        for s in [
            "5493000IBP32UQZ0KL24", // British Broadcasting Corporation
            "213800WSGIIZCXF1P572", // Jaguar Land Rover Ltd
            "506700GE1G29325QX363", // GLEIF itself
            "54930084UKLVMY22DS16", // G.E. Financing GmbH
        ] {
            assert!(validate(&candidate(s)).is_ok(), "{s} should be valid");
            assert_eq!(residue(&candidate(s)), 1, "{s} residue must be 1");
        }
    }

    #[test]
    fn computes_documented_check_digits() {
        assert_eq!(compute_check_digits(b"5493000IBP32UQZ0KL"), 24);
        assert_eq!(compute_check_digits(b"213800WSGIIZCXF1P5"), 72);
        assert_eq!(compute_check_digits(b"506700GE1G29325QX3"), 63);
        assert_eq!(compute_check_digits(b"54930084UKLVMY22DS"), 16);
    }

    #[test]
    fn folding_matches_the_reference_implementation() {
        for base in [
            "5493000IBP32UQZ0KL",
            "213800WSGIIZCXF1P5",
            "506700GE1G29325QX3",
            "54930084UKLVMY22DS",
            "000000000000000000",
            "ZZZZZZZZZZZZZZZZZZ",
        ] {
            assert_eq!(
                compute_check_digits(base.as_bytes().try_into().unwrap()),
                reference_check_digits(base),
                "{base}"
            );
        }
    }

    #[test]
    fn rejects_lowercase_in_base() {
        let err = validate(&candidate("5493000ibp32UQZ0KL24")).unwrap_err();
        assert_eq!(
            err,
            LeiError::InvalidCharacter {
                character: 'i',
                position: 8,
                expected: CharacterClass::Alphanumeric,
            }
        );
    }

    #[test]
    fn rejects_letter_in_check_digit_position() {
        let err = validate(&candidate("5493000IBP32UQZ0KLX4")).unwrap_err();
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
    fn rejects_wrong_check_digits() {
        let err = validate(&candidate("5493000IBP32UQZ0KL25")).unwrap_err();
        assert_eq!(
            err,
            LeiError::InvalidCheckDigits {
                expected: 24,
                found: 25,
            }
        );
    }

    #[test]
    fn rejects_residue_one_with_reserved_check_digits() {
        for (s, expected, found) in [
            ("PRKYQO9OOQ90FWGOFC00", 97u8, 0u8),
            ("TS43UAPFUU97VO4FE001", 98, 1),
            ("2MZDL7DS67LXXZ93H099", 2, 99),
        ] {
            assert_eq!(residue(&candidate(s)), 1, "{s} residue must be 1");
            assert_eq!(
                validate(&candidate(s)),
                Err(LeiError::InvalidCheckDigits { expected, found }),
                "{s} must be rejected despite residue 1"
            );
        }
    }

    #[test]
    fn rejects_adjacent_transposition() {
        // MOD 97-10 catches all single adjacent transpositions of unequal characters.
        assert!(validate(&candidate("5493000IBP32UQ0ZKL24")).is_err());
    }
}
