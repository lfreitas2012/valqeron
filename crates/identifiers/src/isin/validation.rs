//! Structural and checksum validation of an already-normalized 12-byte ISIN candidate (ASCII,
//! uppercase, surrounding whitespace already trimmed).
//!
//! This module has no knowledge of the original user input or of formatting; that is
//! [`super::parser`]'s job. Everything here operates on a `[u8; 12]` and is pure,
//! `#[no_std]`-friendly arithmetic.
//!
//! # Checksum
//!
//! ISIN uses the modulus-10 "double-add-double" (Luhn) algorithm described in ISO 6166 Annex C.
//! Each of the first 11 characters is expanded into decimal digits — a digit contributes itself,
//! and a letter contributes its two-digit ordinal value (`'A'` = 10, ..., `'Z'` = 35). The Luhn
//! doubling rule is then applied so that the check digit, once appended, lands in the units
//! position: the rightmost *expanded* character sits in a doubled position.

use super::error::{CharacterClass, IsinError};

/// The number of positions occupied by the country code plus the NSIN (everything except the
/// trailing check digit).
pub(super) const BASE_LEN: usize = 11;

#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
pub(crate) const LETTERS: &[u8; 26] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
pub(crate) const ALPHANUMERIC: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Runs every validation rule against a normalized candidate, in order from cheapest/most-specific
/// to most-expensive:
/// 1. Character class per position (two-letter country code, alphanumeric NSIN, numeric check digit).
/// 2. Luhn (ISO 6166 Annex C) check digit.
pub(super) fn validate(candidate: &[u8; 12]) -> Result<(), IsinError> {
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

/// Computes the ISO 6166 Luhn check digit for the first 11 characters of an ISIN.
///
/// Walks the base right-to-left so expansion and doubling happen in a single allocation-free pass:
/// for a letter, its units digit is the right-most of the pair and is therefore processed (and
/// toggles the doubling flag) before its tens digit. The rightmost expanded digit starts in a
/// doubled position, because the check digit this function returns will occupy the units position
/// once appended.
///
/// Also used by the [`super::arbitrary`] and [`super::proptest`] generators (behind their features)
/// to produce checksum-correct values without duplicating the algorithm, hence the `allow`.
#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
pub(super) fn compute_check_digit(base: &[u8]) -> u8 {
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

/// Builds a structurally valid ISIN candidate from alphabet indices, then appends the matching
/// check digit.
///
/// This keeps the generator-specific code focused on randomness while centralizing the shape of a
/// valid ISIN: two country-code letters, nine alphanumeric NSIN characters, and one checksum
/// digit.
#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
pub(crate) fn build_valid_isin_bytes(country: [usize; 2], nsin: &[usize]) -> [u8; 12] {
    debug_assert_eq!(nsin.len(), BASE_LEN - 2);

    let mut base = [0u8; BASE_LEN];
    base[0] = LETTERS[country[0]];
    base[1] = LETTERS[country[1]];
    for (slot, idx) in base[2..].iter_mut().zip(nsin) {
        *slot = ALPHANUMERIC[*idx];
    }

    let check = compute_check_digit(&base);
    let mut bytes = [0u8; 12];
    bytes[..BASE_LEN].copy_from_slice(&base);
    bytes[BASE_LEN] = check + b'0';
    bytes
}

/// Applies the Luhn per-digit rule: double when in a doubled position, then cast the two-digit
/// product back to a single digit by subtracting 9.
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
        let mut digits = alloc::vec::Vec::new();
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
