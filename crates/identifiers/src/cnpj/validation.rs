//! Structural and checksum validation of an already-normalized 14-byte CNPJ candidate (ASCII,
//! uppercase, punctuation already stripped).
//!
//! This module has no knowledge of the original user input or of formatting; that is [`super::parser`]'s job.
//! Everything here operates on a `[u8; 14]` and is pure, standard-library arithmetic.

use super::error::{CharacterClass, CnpjError};

/// Weights applied left-to-right to the 12 base characters (root plus branch) when computing the
/// first verification digit (DV1).
const WEIGHTS_DV1: [u32; 12] = [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];

/// Weights applied left-to-right to the 12 base characters plus DV1 (13 values total) when computing
/// the second verification digit (DV2).
const WEIGHTS_DV2: [u32; 13] = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];

/// The number of positions occupied by the root + branch/order segment.
pub(super) const BASE_LEN: usize = 12;

/// Adjusts a generated base segment so it is not a degenerate all-repeated-character value.
///
/// This is only used by fuzz/property generators. It preserves the generated shape while avoiding
/// the one pattern that `validate` rejects independently.
#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
pub(super) fn avoid_all_repeated(base: &mut [u8; BASE_LEN]) {
    if base.iter().all(|&b| b == base[0]) {
        base[0] = if base[0] == b'0' { b'1' } else { b'0' };
    }

    debug_assert!(
        !base.iter().all(|&b| b == base[0]),
        "Base segment must not consist of entirely repeated characters. Is BASE_LEN too small?"
    );
}

/// Runs every validation rule against a normalized candidate, in order from cheapest/most-specific
/// to most-expensive:
/// 1. Character class per position (digit-only tail, alphanumeric head).
/// 2. Rejection of degenerate all-repeated-character input.
/// 3. Módulo 11 checksum for both verification digits.
pub(super) fn validate(candidate: &[u8; 14]) -> Result<(), CnpjError> {
    validate_character_classes(candidate)?;
    validate_not_repeated(candidate)?;
    validate_check_digits(candidate)?;
    Ok(())
}

/// Converts a validated ASCII byte (`'0'...='9'` or `'A'...='Z'`) into its numeric value for the
/// Módulo 11 calculation: `ASCII code - 48`.
///
/// For digits this is simply the digit's value (`'0'` -> 0, ..., `'9'` -> 9).
/// For uppercase letters this yields 17...=42 (`'A'` -> 17, ..., `'Z'` -> 42), per `Nota Técnica
/// Conjunta COCAD/SUARA/RFB nº 49/2024`, which keeps the legacy numeric-only calculation unchanged
/// as a special case.
#[inline]
fn char_value(byte: u8) -> u32 {
    (byte - b'0') as u32
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
                character: byte as char,
                position: (i + 1) as u8,
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
    let values: [u32; 14] = core::array::from_fn(|i| char_value(candidate[i]));

    let dv1 = compute_check_digit(&values[..BASE_LEN], &WEIGHTS_DV1);
    let found_dv1 = values[12] as u8;
    if dv1 != found_dv1 {
        return Err(CnpjError::InvalidCheckDigits {
            position: 13,
            expected: dv1,
            found: found_dv1,
        });
    }

    let mut dv2_input = [0u32; BASE_LEN + 1];
    dv2_input[..BASE_LEN].copy_from_slice(&values[..BASE_LEN]);
    dv2_input[BASE_LEN] = dv1 as u32;
    let dv2 = compute_check_digit(&dv2_input, &WEIGHTS_DV2);
    let found_dv2 = values[13] as u8;
    if dv2 != found_dv2 {
        return Err(CnpjError::InvalidCheckDigits {
            position: 14,
            expected: dv2,
            found: found_dv2,
        });
    }

    Ok(())
}

/// Computes the two verification digits for a well-formed 12-character base segment (each byte
/// already `'0'...='9'` or `'A'...='Z'`).
///
/// Exposed to sibling modules ([`super::arbitrary`], [`super::proptest`]) so they can generate
/// structurally valid, checksum-correct `Cnpj` values without duplicating the Módulo 11 algorithm.
/// Only called when one of those optional features is enabled, hence the `allow`.
#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
pub(super) fn compute_valid_check_digits(base: &[u8; BASE_LEN]) -> (u8, u8) {
    let base_values: [u32; BASE_LEN] = core::array::from_fn(|i| char_value(base[i]));
    let dv1 = compute_check_digit(&base_values, &WEIGHTS_DV1);

    let mut dv2_input = [0u32; BASE_LEN + 1];
    dv2_input[..BASE_LEN].copy_from_slice(&base_values);
    dv2_input[BASE_LEN] = dv1 as u32;
    let dv2 = compute_check_digit(&dv2_input, &WEIGHTS_DV2);

    (dv1, dv2)
}

/// The classic Módulo 11 verification-digit algorithm, unchanged since the numeric-only CNPJ era:
/// weighted sum, remainder mod 11, then `0` if the remainder is `0` or `1`, otherwise `11 - remainder`.
fn compute_check_digit(values: &[u32], weights: &[u32]) -> u8 {
    debug_assert_eq!(values.len(), weights.len());
    let sum: u32 = values.iter().zip(weights).map(|(v, w)| v * w).sum();
    let remainder = sum % 11;
    if remainder < 2 {
        0
    } else {
        (11 - remainder) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
