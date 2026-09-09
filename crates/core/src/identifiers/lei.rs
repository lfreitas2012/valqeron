use crate::identifiers::common::CharacterClass;
use core::convert::TryFrom;
use core::str::{FromStr, from_utf8_unchecked};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use thiserror::Error;

const BASE_LEN: usize = 18;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed Lei should be used; discarding it wastes the validation work"]
pub struct Lei {
    bytes: [u8; 20],
}

impl Lei {
    /// # Errors
    ///
    /// Returns [`LeiError`] if the input is empty, is not exactly twenty
    /// characters long, contains a non-ASCII-alphanumeric character, or has
    /// check digits that don't match the ISO 17442 MOD 97-10 algorithm.
    pub fn parse(input: &str) -> Result<Self, LeiError> {
        let candidate = normalize(input)?;
        Self::from_bytes(candidate)
    }

    /// Alias for [`Lei::parse`].
    ///
    /// # Errors
    ///
    /// See [`Lei::parse`].
    #[inline]
    pub fn new(input: &str) -> Result<Self, LeiError> {
        Self::parse(input)
    }

    /// # Errors
    ///
    /// Returns [`LeiError`] if the bytes are not ASCII alphanumeric in the
    /// right positions, or fail the MOD 97-10 check digit algorithm.
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
        // ASCII is one byte per character, so byte offset 4 is always a char boundary;
        // `unwrap_or` is a defensive fallback that can never actually be reached.
        self.as_str().get(0..4).unwrap_or("")
    }

    #[inline]
    #[must_use]
    pub fn entity_id(&self) -> &str {
        self.as_str().get(4..BASE_LEN).unwrap_or("")
    }

    #[inline]
    #[must_use]
    pub fn check_digits(&self) -> u8 {
        let &[tens, ones] = self.bytes.last_chunk::<2>().unwrap_or(b"00");
        digit_pair_value(tens, ones)
    }

    #[inline]
    #[must_use]
    pub fn computed_check_digits(&self) -> u8 {
        let base = self
            .bytes
            .first_chunk::<BASE_LEN>()
            .unwrap_or(&[b'0'; BASE_LEN]);
        compute_check_digits(base)
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

/// Which character class ISO 17442 expects at a given position: the first 18
/// characters are alphanumeric, the final 2 (the check digits) are decimal digits.
fn expected_class(position_index: usize) -> CharacterClass {
    if position_index < BASE_LEN {
        CharacterClass::Alphanumeric
    } else {
        CharacterClass::Digit
    }
}

fn validate(candidate: &[u8; 20]) -> Result<(), LeiError> {
    validate_character_classes(candidate)?;
    validate_check_digits(candidate)?;
    Ok(())
}

fn validate_character_classes(candidate: &[u8; 20]) -> Result<(), LeiError> {
    for ((i, &byte), position) in candidate.iter().enumerate().zip(1u8..) {
        let is_valid = if i < BASE_LEN {
            byte.is_ascii_digit() || byte.is_ascii_uppercase()
        } else {
            byte.is_ascii_digit()
        };

        if !is_valid {
            return Err(LeiError::InvalidCharacter {
                character: char::from(byte),
                position,
                expected: expected_class(i),
            });
        }
    }
    Ok(())
}

fn validate_check_digits(candidate: &[u8; 20]) -> Result<(), LeiError> {
    let base = candidate
        .first_chunk::<BASE_LEN>()
        .unwrap_or(&[b'0'; BASE_LEN]);
    let expected = compute_check_digits(base);

    // Character-class validation above guarantees the final two bytes are ASCII digits.
    let &[tens, ones] = candidate.last_chunk::<2>().unwrap_or(b"00");
    let found = digit_pair_value(tens, ones);

    if expected == found {
        Ok(())
    } else {
        Err(LeiError::InvalidCheckDigits { expected, found })
    }
}

/// Interprets two ASCII digit bytes as a base-10 value in `0..=99`.
fn digit_pair_value(tens: u8, ones: u8) -> u8 {
    let tens_digit = tens.wrapping_sub(b'0');
    let ones_digit = ones.wrapping_sub(b'0');
    tens_digit.saturating_mul(10).saturating_add(ones_digit)
}

fn compute_check_digits(base: &[u8; BASE_LEN]) -> u8 {
    // Fold the base, then the two placeholder '0' characters, modulo 97.
    let rem = fold_mod_97(0, base);
    let rem = rem.wrapping_mul(100).checked_rem(97).unwrap_or(rem); // equivalent to folding "00"
    u8::try_from(98u32.wrapping_sub(rem)).unwrap_or(0)
}

#[inline]
fn fold_mod_97(mut rem: u32, bytes: &[u8]) -> u32 {
    for &c in bytes {
        if c.is_ascii_digit() {
            let digit = u32::from(c.wrapping_sub(b'0'));
            rem = rem
                .wrapping_mul(10)
                .wrapping_add(digit)
                .checked_rem(97)
                .unwrap_or(0);
        } else {
            // 'A' => 10, ..., 'Z' => 35.
            let value = u32::from(c.wrapping_sub(b'A').wrapping_add(10));
            rem = rem
                .wrapping_mul(100)
                .wrapping_add(value)
                .checked_rem(97)
                .unwrap_or(0);
        }
    }
    rem
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

    // First pass: reject any non-ASCII character with a precise position/expected-class.
    for ((i, ch), position) in trimmed.chars().enumerate().zip(1u8..) {
        if !ch.is_ascii() {
            return Err(LeiError::InvalidCharacter {
                character: ch,
                position,
                expected: expected_class(i),
            });
        }
    }

    // Second pass: every character is now known to be ASCII, so this can't fail.
    let mut buf = [0u8; 20];
    for (slot, ch) in buf.iter_mut().zip(trimmed.chars()) {
        *slot = u8::try_from(ch.to_ascii_uppercase()).unwrap_or(0);
    }

    Ok(buf)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::string_slice,
    clippy::cast_possible_truncation
)]
mod tests {
    use super::*;

    fn candidate(s: &str) -> [u8; 20] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 20];
        out.copy_from_slice(bytes);
        out
    }

    fn residue(candidate: &[u8; 20]) -> u32 {
        fold_mod_97(0, candidate)
    }

    /// A deliberately naive, independently-written MOD 97-10 implementation used only
    /// to cross-check `compute_check_digits` — arithmetic-heavy on purpose.
    fn reference_check_digits(base: &str) -> u8 {
        let mut expanded = String::new();
        for c in base.bytes() {
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

    mod parsing {
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

        #[test]
        fn parse_and_from_str_agree() {
            let via_parse = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
            let via_from_str: Lei = "5493000IBP32UQZ0KL24".parse().unwrap();
            assert_eq!(via_parse, via_from_str);
        }

        #[test]
        fn new_is_an_alias_for_parse() {
            assert_eq!(
                Lei::new("5493000IBP32UQZ0KL24"),
                Lei::parse("5493000IBP32UQZ0KL24")
            );
        }
    }

    mod formatting {
        use super::*;

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

        #[test]
        fn error_messages_are_human_readable() {
            assert_eq!(LeiError::Empty.to_string(), "LEI cannot be empty");
            assert_eq!(
                LeiError::InvalidLength { found: 19 }.to_string(),
                "LEI must be 20 characters long, found 19"
            );
        }
    }

    mod validation {
        use super::*;

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

        #[test]
        fn try_from_slice_rejects_wrong_length() {
            let err = Lei::try_from(&b"5493000IBP32UQZ0KL"[..]).unwrap_err();
            assert_eq!(err, LeiError::InvalidLength { found: 18 });
        }

        #[test]
        fn try_from_array_matches_from_bytes() {
            let bytes = candidate("5493000IBP32UQZ0KL24");
            assert_eq!(Lei::try_from(bytes), Lei::from_bytes(bytes));
        }
    }

    mod accessors {
        use super::*;

        #[test]
        fn lou_prefix_and_entity_id_partition_the_base() {
            let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
            assert_eq!(lei.lou_prefix(), "5493");
            assert_eq!(lei.entity_id(), "000IBP32UQZ0KL");
            assert_eq!(
                format!("{}{}", lei.lou_prefix(), lei.entity_id()),
                &lei.as_str()[..BASE_LEN]
            );
        }

        #[test]
        fn check_digits_matches_computed_check_digits() {
            let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
            assert_eq!(lei.check_digits(), 24);
            assert_eq!(lei.check_digits(), lei.computed_check_digits());
        }
    }

    mod comparisons {
        use super::*;
        use std::collections::HashSet;

        #[test]
        fn compares_equal_to_matching_str() {
            let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
            assert_eq!(lei, "5493000IBP32UQZ0KL24");
            assert_eq!("5493000IBP32UQZ0KL24", lei);
            assert_ne!(lei, "213800WSGIIZCXF1P572");
        }

        #[test]
        fn orders_the_same_as_the_underlying_bytes() {
            let bbc = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
            let jlr = Lei::parse("213800WSGIIZCXF1P572").unwrap();
            assert!(jlr < bbc);
        }

        #[test]
        fn hash_is_consistent_with_equality() {
            let a = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
            let b = Lei::parse("5493000ibp32uqz0kl24").unwrap(); // normalizes to the same bytes
            let mut set = HashSet::new();
            set.insert(a);
            assert!(!set.insert(b), "equal Leis must hash to the same bucket");
        }

        #[test]
        fn as_ref_bytes_matches_as_bytes() {
            let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
            assert_eq!(AsRef::<[u8]>::as_ref(&lei), lei.as_bytes());
        }
    }
}
