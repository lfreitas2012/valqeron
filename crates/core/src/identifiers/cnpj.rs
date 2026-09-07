use crate::identifiers::common::CharacterClass;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    ops::Deref,
    str::{FromStr, from_utf8_unchecked},
};
use thiserror::Error;

const FORMATTED_LEN: usize = 18;
const WEIGHTS_DV1: [u32; 12] = [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
const WEIGHTS_DV2: [u32; 13] = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
const BASE_LEN: usize = 12;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed Cnpj should be used; discarding it wastes the validation work"]
pub struct Cnpj {
    bytes: [u8; 14],
}

impl Cnpj {
    pub fn parse(input: &str) -> Result<Self, CnpjError> {
        let candidate = normalize(input)?;
        Self::from_bytes(candidate)
    }

    #[doc(alias = "from_digits")]
    pub fn from_bytes(bytes: [u8; 14]) -> Result<Self, CnpjError> {
        validate(&bytes)?;
        Ok(Cnpj { bytes })
    }

    #[doc(alias = "digits")]
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 14] {
        &self.bytes
    }

    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: The bytes array is strictly guaranteed to contain only
        // valid ASCII uppercase alphanumeric characters by `Cnpj::from_bytes`.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }

    #[inline]
    #[must_use]
    pub fn formatted(&self) -> FormattedCnpj {
        FormattedCnpj::new(self)
    }

    #[inline]
    #[must_use]
    pub fn root(&self) -> &str {
        self.as_str().get(0..8).unwrap_or("")
    }

    #[inline]
    #[must_use]
    pub fn branch_code(&self) -> &str {
        self.as_str().get(8..12).unwrap_or("")
    }

    #[inline]
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.branch_code() == "0001"
    }

    #[must_use]
    pub fn branch_number(&self) -> Option<u16> {
        self.branch_code().parse().ok()
    }

    #[inline]
    #[must_use]
    pub fn check_digits(&self) -> (u8, u8) {
        (
            self.bytes[12].saturating_sub(b'0'),
            self.bytes[13].saturating_sub(b'0'),
        )
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CnpjError {
    #[error("CNPJ cannot be empty.")]
    Empty,

    #[error("CNPJ must have exactly 14 meaningful characters, found {found}")]
    InvalidLength { found: usize },

    #[error("CNPJ must contain only digits, found '{character}' at position {position}")]
    InvalidCharacter {
        character: char,
        position: u8,
        expected: CharacterClass,
    },

    #[error(
        "CNPJ verification digit at position {position} is invalid, expected {expected} but found {found}"
    )]
    InvalidCheckDigits {
        position: u8,
        expected: u8,
        found: u8,
    },

    #[error("A CNPJ cannot contain only repeated digits")]
    RepeatedDigits,
}

impl FromStr for Cnpj {
    type Err = CnpjError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Cnpj {
    type Error = CnpjError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 14]> for Cnpj {
    type Error = CnpjError;

    fn try_from(value: [u8; 14]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for Cnpj {
    type Error = CnpjError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 14] = value
            .try_into()
            .map_err(|_| CnpjError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
    }
}

impl PartialEq<str> for Cnpj {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Cnpj {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Cnpj> for str {
    fn eq(&self, other: &Cnpj) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Cnpj> for &str {
    fn eq(&self, other: &Cnpj) -> bool {
        *self == other.as_str()
    }
}

impl AsRef<[u8]> for Cnpj {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for Cnpj {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Serialize for Cnpj {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Cnpj {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        Cnpj::parse(s).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormattedCnpj([u8; FORMATTED_LEN]);

impl FormattedCnpj {
    pub(super) fn new(cnpj: &Cnpj) -> Self {
        let d = cnpj.as_bytes();
        let mut out = [0u8; FORMATTED_LEN];
        let layout: [u8; FORMATTED_LEN] = [
            d[0], d[1], b'.', d[2], d[3], d[4], b'.', d[5], d[6], d[7], b'/', d[8], d[9], d[10],
            d[11], b'-', d[12], d[13],
        ];
        out.copy_from_slice(&layout);
        FormattedCnpj(out)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `FormattedCnpj::new` builds this buffer exclusively from a validated `Cnpj`'s
        // ASCII bytes interleaved with ASCII punctuation (`.`, `/`, `-`), so it is always UTF-8.
        unsafe { from_utf8_unchecked(&self.0) }
    }
}

impl Deref for FormattedCnpj {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for FormattedCnpj {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for FormattedCnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for FormattedCnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("FormattedCnpj")
            .field(&self.as_str())
            .finish()
    }
}

impl fmt::Display for Cnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.formatted().as_str())
    }
}

impl fmt::Debug for Cnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Cnpj")
            .field(&self.formatted().as_str())
            .finish()
    }
}

fn validate(candidate: &[u8; 14]) -> Result<(), CnpjError> {
    validate_character_classes(candidate)?;
    validate_not_repeated(candidate)?;
    validate_check_digits(candidate)?;
    Ok(())
}

#[inline]
fn char_value(byte: u8) -> u32 {
    u32::from(byte.saturating_sub(b'0'))
}

fn validate_character_classes(candidate: &[u8; 14]) -> Result<(), CnpjError> {
    for (i, &byte) in candidate.iter().enumerate() {
        let is_valid = if i < BASE_LEN {
            byte.is_ascii_digit() || byte.is_ascii_uppercase()
        } else {
            byte.is_ascii_digit()
        };

        if !is_valid {
            let expected = if i < BASE_LEN {
                CharacterClass::Alphanumeric
            } else {
                CharacterClass::Digit
            };

            return Err(CnpjError::InvalidCharacter {
                character: char::from(byte),
                position: u8::try_from(i).unwrap_or(0).saturating_add(1),
                expected,
            });
        }
    }
    Ok(())
}

fn validate_not_repeated(candidate: &[u8; 14]) -> Result<(), CnpjError> {
    if candidate.iter().all(|&b| b == candidate[0]) {
        return Err(CnpjError::RepeatedDigits);
    }
    Ok(())
}

fn validate_check_digits(candidate: &[u8; 14]) -> Result<(), CnpjError> {
    let base: &[u8; BASE_LEN] = candidate
        .get(..BASE_LEN)
        .and_then(|s| s.try_into().ok())
        .unwrap_or(&[b'0'; BASE_LEN]);

    let (expected_dv1, expected_dv2) = compute_valid_check_digits(base);

    let found_dv1 = candidate
        .get(12)
        .and_then(|&b| u8::try_from(char_value(b)).ok())
        .unwrap_or(0);

    if expected_dv1 != found_dv1 {
        return Err(CnpjError::InvalidCheckDigits {
            position: 13,
            expected: expected_dv1,
            found: found_dv1,
        });
    }

    let found_dv2 = candidate
        .get(13)
        .and_then(|&b| u8::try_from(char_value(b)).ok())
        .unwrap_or(0);

    if expected_dv2 != found_dv2 {
        return Err(CnpjError::InvalidCheckDigits {
            position: 14,
            expected: expected_dv2,
            found: found_dv2,
        });
    }

    Ok(())
}

fn compute_valid_check_digits(base: &[u8; BASE_LEN]) -> (u8, u8) {
    let base_values: [u32; BASE_LEN] = base.map(char_value);
    let dv1 = compute_check_digit(&base_values, &WEIGHTS_DV1);

    let dv2_input: [u32; BASE_LEN + 1] = core::array::from_fn(|i| {
        base_values
            .get(i)
            .copied()
            .unwrap_or_else(|| u32::from(dv1))
    });

    let dv2 = compute_check_digit(&dv2_input, &WEIGHTS_DV2);

    (dv1, dv2)
}

fn compute_check_digit(values: &[u32], weights: &[u32]) -> u8 {
    debug_assert_eq!(values.len(), weights.len());

    let sum: u32 = values
        .iter()
        .zip(weights)
        .fold(0u32, |acc, (v, w)| acc.saturating_add(v.saturating_mul(*w)));

    let remainder = sum.checked_rem(11).unwrap_or(0);

    if remainder < 2 {
        0
    } else {
        u8::try_from(11_u32.saturating_sub(remainder)).unwrap_or(0)
    }
}

#[inline]
fn is_formatting_char(c: char) -> bool {
    matches!(c, '.' | '/' | '-') || c.is_whitespace()
}

fn normalize(input: &str) -> Result<[u8; 14], CnpjError> {
    if input.is_empty() {
        return Err(CnpjError::Empty);
    }

    let meaningful = input.chars().filter(|&c| !is_formatting_char(c));
    let found = meaningful.clone().count();
    if found != 14 {
        return Err(CnpjError::InvalidLength { found });
    }

    let mut buf = [0u8; 14];
    for (i, (slot, ch)) in buf.iter_mut().zip(meaningful).enumerate() {
        if !ch.is_ascii() {
            return Err(CnpjError::InvalidCharacter {
                character: ch,
                // i is bounded by 14, so try_from will never fail.
                // unwrap_or(0) satisfies the type system and linter safely
                position: u8::try_from(i).unwrap_or(0).saturating_add(1),
                expected: CharacterClass::Alphanumeric,
            });
        }
        // We just checked !ch.is_ascii(), so this try_from is guaranteed to succeed.
        // unwrap_or(b'\0') provides a safe, unreachable fallback.
        *slot = u8::try_from(u32::from(ch.to_ascii_uppercase())).unwrap_or(b'\0');
    }

    Ok(buf)
}

#[cfg(test)]
mod tests_parsers {
    use crate::identifiers::cnpj::{CnpjError, normalize};

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize(""), Err(CnpjError::Empty));
    }

    #[test]
    fn strips_conventional_punctuation() {
        assert_eq!(normalize("12.345.678/0001-95"), normalize("12345678000195"));
    }

    #[test]
    fn strips_all_whitespace() {
        assert_eq!(
            normalize(" \t12.345.678/0001-95\n \r"),
            normalize("12345678000195")
        );
    }

    #[test]
    fn strips_non_ascii_whitespace() {
        assert_eq!(
            normalize("\u{00A0}12.345.678/0001-95\u{00A0}"),
            normalize("12345678000195")
        );
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(normalize("12abc34501de35").unwrap(), *b"12ABC34501DE35");
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            normalize("1234"),
            Err(CnpjError::InvalidLength { found: 4 })
        );
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("12ç45678000195").unwrap_err();
        assert!(matches!(
            err,
            CnpjError::InvalidCharacter {
                character: 'ç', ..
            }
        ));
    }
}

#[cfg(test)]
mod tests_validation {
    use crate::identifiers::cnpj::{CharacterClass, CnpjError, validate};

    fn candidate(s: &str) -> [u8; 14] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 14];
        out.copy_from_slice(bytes);
        out
    }

    #[test]
    fn accepts_valid_legacy_numeric_cnpj() {
        // A well-known real CNPJ root (Banco do Brasil).
        assert!(validate(&candidate("00000000000191")).is_ok());
    }

    #[test]
    fn accepts_valid_alphanumeric_cnpj() {
        // Worked example from the official SERPRO technical note.
        assert!(validate(&candidate("12ABC34501DE35")).is_ok());
    }

    #[test]
    fn rejects_letter_in_check_digit_position() {
        let err = validate(&candidate("12ABC34501DEA5")).unwrap_err();
        assert_eq!(
            err,
            CnpjError::InvalidCharacter {
                character: 'A',
                position: 13,
                expected: CharacterClass::Digit,
            }
        );
    }

    #[test]
    fn rejects_symbol_in_base() {
        let err = validate(&candidate("12!BC34501DE35")).unwrap_err();
        assert_eq!(
            err,
            CnpjError::InvalidCharacter {
                character: '!',
                position: 3,
                expected: CharacterClass::Alphanumeric,
            }
        );
    }

    #[test]
    fn rejects_all_repeated_digit() {
        assert_eq!(
            validate(&candidate("11111111111111")).unwrap_err(),
            CnpjError::RepeatedDigits
        );
    }

    #[test]
    fn rejects_bad_checksum() {
        let err = validate(&candidate("00000000000192")).unwrap_err();
        assert_eq!(
            err,
            CnpjError::InvalidCheckDigits {
                position: 14,
                expected: 1,
                found: 2,
            }
        );
    }
}

#[cfg(test)]
mod tests_formated {
    use crate::identifiers::cnpj::Cnpj;
    use std::format;
    use std::string::ToString;

    #[test]
    fn formats_numeric_cnpj() {
        let cnpj = Cnpj::parse("00000000000191").unwrap();
        assert_eq!(cnpj.formatted().as_str(), "00.000.000/0001-91");
        assert_eq!(cnpj.to_string(), "00.000.000/0001-91");
    }

    #[test]
    fn formats_alphanumeric_cnpj() {
        let cnpj = Cnpj::parse("12ABC34501DE35").unwrap();
        assert_eq!(cnpj.formatted().as_str(), "12.ABC.345/01DE-35");
    }

    #[test]
    fn debug_is_readable() {
        let cnpj = Cnpj::parse("00000000000191").unwrap();
        assert_eq!(format!("{cnpj:?}"), "Cnpj(\"00.000.000/0001-91\")");
    }
}
