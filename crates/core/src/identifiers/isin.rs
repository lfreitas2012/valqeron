use crate::identifiers::common::CharacterClass;
use crate::identifiers::country_code::CountryCode;
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
    /// # Errors
    ///
    /// Returns [`IsinError`] if the input is empty, is not exactly twelve
    /// characters long, contains a character outside its position's
    /// expected class, or has a check digit that doesn't match the Luhn
    /// algorithm.
    pub fn parse(input: &str) -> Result<Self, IsinError> {
        let candidate = normalize(input)?;
        Self::from_bytes(candidate)
    }

    /// Alias for [`Isin::parse`].
    ///
    /// # Errors
    ///
    /// See [`Isin::parse`].
    #[inline]
    pub fn new(input: &str) -> Result<Self, IsinError> {
        Self::parse(input)
    }

    /// # Errors
    ///
    /// Returns [`IsinError`] if the bytes are not ASCII in the right
    /// positions, or fail the Luhn check digit algorithm.
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
        // ASCII is one byte per character, so byte offset 2 is always a char boundary;
        // `unwrap_or` is a defensive fallback that can never actually be reached.
        self.as_str().get(0..2).unwrap_or("")
    }

    #[inline]
    #[must_use]
    pub fn country(&self) -> Option<CountryCode> {
        let &[a, b] = self.bytes.first_chunk::<2>().unwrap_or(b"ZZ");
        CountryCode::from_bytes([a, b]).ok()
    }

    #[inline]
    #[must_use]
    pub fn nsin(&self) -> &str {
        self.as_str().get(2..BASE_LEN).unwrap_or("")
    }

    #[inline]
    #[must_use]
    pub fn check_digit(&self) -> u8 {
        let byte = self.bytes.last().copied().unwrap_or(b'0');
        byte.wrapping_sub(b'0')
    }

    #[inline]
    #[must_use]
    pub fn computed_check_digit(&self) -> u8 {
        let base = self
            .bytes
            .first_chunk::<BASE_LEN>()
            .unwrap_or(&[b'0'; BASE_LEN]);
        compute_check_digit(base)
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

/// Which character class ISO 6166 expects at a given position: the first 2
/// characters are the country code (letters only), the next 9 are the NSIN
/// (alphanumeric), and the final 1 is the Luhn check digit.
fn expected_class(position_index: usize) -> CharacterClass {
    if position_index < 2 {
        CharacterClass::Letter
    } else if position_index < BASE_LEN {
        CharacterClass::Alphanumeric
    } else {
        CharacterClass::Digit
    }
}

fn validate(candidate: &[u8; 12]) -> Result<(), IsinError> {
    validate_character_classes(candidate)?;
    validate_check_digit(candidate)?;
    Ok(())
}

fn validate_character_classes(candidate: &[u8; 12]) -> Result<(), IsinError> {
    for ((i, &byte), position) in candidate.iter().enumerate().zip(1u8..) {
        let is_valid = if i < 2 {
            byte.is_ascii_uppercase()
        } else if i < BASE_LEN {
            byte.is_ascii_digit() || byte.is_ascii_uppercase()
        } else {
            byte.is_ascii_digit()
        };

        if !is_valid {
            return Err(IsinError::InvalidCharacter {
                character: char::from(byte),
                position,
                expected: expected_class(i),
            });
        }
    }
    Ok(())
}

fn validate_check_digit(candidate: &[u8; 12]) -> Result<(), IsinError> {
    let base = candidate
        .first_chunk::<BASE_LEN>()
        .unwrap_or(&[b'0'; BASE_LEN]);
    let expected = compute_check_digit(base);

    // Character-class validation above guarantees the final byte is an ASCII digit.
    let found = candidate.last().copied().unwrap_or(b'0').wrapping_sub(b'0');

    if expected == found {
        Ok(())
    } else {
        Err(IsinError::InvalidCheckDigit { expected, found })
    }
}

fn compute_check_digit(base: &[u8; BASE_LEN]) -> u8 {
    let mut sum = 0u32;
    let mut double = true;

    for &c in base.iter().rev() {
        if c.is_ascii_digit() {
            let digit = u32::from(c.wrapping_sub(b'0'));
            sum = sum.wrapping_add(luhn_step(digit, double));
            double = !double;
        } else {
            // 'A' => 10, ..., 'Z' => 35; split into tens and units.
            let value = u32::from(c.wrapping_sub(b'A').wrapping_add(10));
            let tens = value.checked_div(10).unwrap_or(0);
            let units = value.checked_rem(10).unwrap_or(0);
            // Units is the rightmost digit of the expanded pair, so it is processed first.
            sum = sum.wrapping_add(luhn_step(units, double));
            double = !double;
            sum = sum.wrapping_add(luhn_step(tens, double));
            double = !double;
        }
    }

    let remainder = sum.checked_rem(10).unwrap_or(0);
    let check = 10u32.wrapping_sub(remainder).checked_rem(10).unwrap_or(0);
    u8::try_from(check).unwrap_or(0)
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

    // First pass: reject any non-ASCII character with a precise position/expected-class.
    for ((i, ch), position) in trimmed.chars().enumerate().zip(1u8..) {
        if !ch.is_ascii() {
            return Err(IsinError::InvalidCharacter {
                character: ch,
                position,
                expected: expected_class(i),
            });
        }
    }

    // Second pass: every character is now known to be ASCII, so this can't fail.
    let mut buf = [0u8; 12];
    for (slot, ch) in buf.iter_mut().zip(trimmed.chars()) {
        *slot = u8::try_from(ch.to_ascii_uppercase()).unwrap_or(0);
    }

    Ok(buf)
}

#[inline]
fn luhn_step(value: u32, double: bool) -> u32 {
    if double {
        let doubled = value.wrapping_mul(2);
        if doubled > 9 {
            doubled.wrapping_sub(9)
        } else {
            doubled
        }
    } else {
        value
    }
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
    clippy::cast_possible_truncation,
    clippy::cast_lossless
)]
mod tests {
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
        let mut digits = Vec::new();
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

    mod formatting {
        use super::*;

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

        #[test]
        fn error_messages_are_human_readable() {
            assert_eq!(IsinError::Empty.to_string(), "ISIN code cannot be empty");
            assert_eq!(
                IsinError::InvalidCheckDigit {
                    expected: 5,
                    found: 6
                }
                .to_string(),
                "ISIN code has an invalid check digit; expected 5, found 6"
            );
        }
    }

    mod validation {
        use super::*;

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
                    compute_check_digit(base.as_bytes().try_into().unwrap()),
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

        #[test]
        fn try_from_slice_rejects_wrong_length() {
            let err = Isin::try_from(&b"US037833100"[..]).unwrap_err();
            assert_eq!(err, IsinError::InvalidLength { found: 11 });
        }

        #[test]
        fn try_from_array_matches_from_bytes() {
            let bytes = candidate("US0378331005");
            assert_eq!(Isin::try_from(bytes), Isin::from_bytes(bytes));
        }
    }

    mod parsing {
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

        #[test]
        fn parse_and_from_str_agree() {
            let via_parse = Isin::parse("US0378331005").unwrap();
            let via_from_str: Isin = "US0378331005".parse().unwrap();
            assert_eq!(via_parse, via_from_str);
        }

        #[test]
        fn new_is_an_alias_for_parse() {
            assert_eq!(Isin::new("US0378331005"), Isin::parse("US0378331005"));
        }
    }

    mod accessors {
        use super::*;

        #[test]
        fn country_code_and_nsin_partition_the_base() {
            let isin = Isin::parse("US0378331005").unwrap();
            assert_eq!(isin.country_code(), "US");
            assert_eq!(isin.nsin(), "037833100");
        }

        #[test]
        fn country_resolves_a_known_country() {
            let isin = Isin::parse("US0378331005").unwrap();
            assert!(isin.country().is_some());
        }

        #[test]
        fn check_digit_matches_computed_check_digit() {
            let isin = Isin::parse("US0378331005").unwrap();
            assert_eq!(isin.check_digit(), 5);
            assert_eq!(isin.check_digit(), isin.computed_check_digit());
        }
    }

    mod comparisons {
        use super::*;
        use std::collections::HashSet;

        #[test]
        fn compares_equal_to_matching_str() {
            let isin = Isin::parse("US0378331005").unwrap();
            assert_eq!(isin, "US0378331005");
            assert_eq!("US0378331005", isin);
            assert_ne!(isin, "US0231351067");
        }

        #[test]
        fn orders_the_same_as_the_underlying_bytes() {
            let amzn = Isin::parse("US0231351067").unwrap();
            let aapl = Isin::parse("US0378331005").unwrap();
            assert!(amzn < aapl);
        }

        #[test]
        fn hash_is_consistent_with_equality() {
            let a = Isin::parse("US0378331005").unwrap();
            let b = Isin::parse("us0378331005").unwrap();
            let mut set = HashSet::new();
            set.insert(a);
            assert!(!set.insert(b), "equal Isins must hash to the same bucket");
        }

        #[test]
        fn as_ref_bytes_matches_as_bytes() {
            let isin = Isin::parse("US0378331005").unwrap();
            assert_eq!(AsRef::<[u8]>::as_ref(&isin), isin.as_bytes());
        }
    }
}
