//! Structural and checksum validation of an already-normalized 20-byte LEI candidate (ASCII,
//! uppercase, surrounding whitespace already trimmed).
//!
//! This module has no knowledge of the original user input or of formatting; that is
//! [`super::parser`]'s job. Everything here operates on a `[u8; 20]` and is pure,
//! standard-library arithmetic.
//!
//! # Checksum
//!
//! ISO 17442 specifies the two check digits (positions 19-20) using the **ISO/IEC 7064, MOD 97-10**
//! scheme, the same algorithm IBAN uses (ISO 13616). Each character is expanded to its numeric
//! value (a digit contributes itself; a letter contributes its two-digit ordinal, `'A'` = 10, ...,
//! `'Z'` = 35), the whole string is read as one large base-10 integer, and:
//!
//! - **Validation** accepts the candidate if and only if that integer is congruent to `1` modulo
//!   97 (`n mod 97 == 1`).
//! - **Computation** of the check digits for a fresh 18-character base appends `"00"`, takes the
//!   integer modulo 97, and returns `98 - (n mod 97)` as a two-digit value.
//!
//! The 20-character expansion is far larger than any fixed-width integer, so the modulo is folded
//! character-by-character (`rem = (rem * 10 + digit) % 97` for a digit, `rem = (rem * 100 + value)
//! % 97` for a letter). This keeps the whole routine allocation-free and within `u32`.
//!
//! # Validation policy (deliberate scope)
//!
//! This crate validates an LEI **structurally and by the ISO/IEC 7064 arithmetic only**:
//!
//! - Positions 1-18 must be ASCII alphanumeric (`[A-Z0-9]`) and positions 19-20 must be ASCII
//!   digits (`[0-9]`), then the two check digits must equal the pair computed from the base by the
//!   MOD 97-10 algorithm (equivalently, the full-string residue is `1`).
//! - It **does not** enforce GLEIF's *operational* convention that positions 5-6 are `"00"`. That
//!   is an allocation policy of the Global LEI System, not a rule of ISO 17442, and it may change;
//!   encoding it here would reject otherwise standard-conformant codes. This mirrors how
//!   [`Isin`](crate::Isin) validates its country prefix purely structurally.
//! - The check-digit values `00`, `01`, and `99` **cannot** appear in a valid LEI, and this crate
//!   rejects them. The check digits are `98 - (n mod 97)`, and since `n mod 97` ranges over
//!   `0...=96`, that expression ranges over `2...=98`; ISO 17442-1 likewise limits the pair to
//!   `02...=98`. This is not an extra rule layered on top of the arithmetic: because `validate`
//!   compares the candidate's digits against the pair recomputed by [`compute_check_digits`] (never
//!   `00`/`01`/`99`), the exclusion follows directly. Note that a *residue-only* `n mod 97 == 1`
//!   test would be weaker here, accepting some bases paired with `00`, `01`, or `99`; those all
//!   fail the compute-and-compare check this crate uses.
//!
//! In short: a value that passes here is a structurally valid, MOD 97-10-correct LEI per ISO 17442.
//! It is *not* a guarantee that GLEIF has actually issued that specific code.

use super::error::{CharacterClass, LeiError};

/// The number of leading positions (LOU prefix + entity-specific part) that precede the two check
/// digits.
pub(super) const BASE_LEN: usize = 18;

#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
pub(crate) const ALPHANUMERIC: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Runs every validation rule against a normalized candidate, in order from cheapest/most-specific
/// to most-expensive:
/// 1. Character class per position (alphanumeric base, numeric check digits).
/// 2. ISO/IEC 7064 MOD 97-10 check digits.
pub(super) fn validate(candidate: &[u8; 20]) -> Result<(), LeiError> {
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

/// Computes the two ISO/IEC 7064 MOD 97-10 check digits for a well-formed 18-character base
/// segment (each byte already `'0'...='9'` or `'A'...='Z'`), returned as a single value in
/// `0...=99`.
///
/// Appends the placeholder `"00"` to the base, folds the expanded integer modulo 97, and returns
/// `98 - (n mod 97)`.
///
/// Also used by the [`super::arbitrary`] and [`super::proptest`] generators (behind their features)
/// to produce checksum-correct values without duplicating the algorithm, hence the `allow`.
#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
pub(super) fn compute_check_digits(base: &[u8; BASE_LEN]) -> u8 {
    // Fold the base, then the two placeholder '0' characters, modulo 97.
    let mut rem = fold_mod_97(0, base);
    rem = (rem * 100) % 97; // equivalent to folding "00"
    (98 - rem) as u8
}

/// Folds a slice of already-validated ASCII bytes into an existing MOD 97 remainder.
///
/// A digit advances the running value by one decimal place (`rem * 10 + d`); a letter expands to
/// its two-digit ordinal and advances by two places (`rem * 100 + value`). The remainder is reduced
/// modulo 97 at every step so it never leaves `0...97`.
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

/// Computes the full ISO/IEC 7064 MOD 97-10 residue of all 20 characters. A conforming LEI yields
/// `1`. Exposed to sibling test modules for cross-checking; `validate` itself goes through
/// [`compute_check_digits`] so the acceptance and generation paths share one implementation.
#[cfg(test)]
pub(super) fn residue(candidate: &[u8; 20]) -> u32 {
    fold_mod_97(0, candidate)
}

/// Builds a structurally valid LEI candidate from alphabet indices, then appends the matching two
/// check digits.
///
/// This keeps the generator-specific code focused on randomness while centralizing the shape of a
/// valid LEI: eighteen alphanumeric base characters and two checksum digits.
#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
pub(crate) fn build_valid_lei_bytes(base_indices: &[usize; BASE_LEN]) -> [u8; 20] {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(s: &str) -> [u8; 20] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 20];
        out.copy_from_slice(bytes);
        out
    }

    /// A second, deliberately naive MOD 97-10 implementation used to cross-check
    /// [`compute_check_digits`]: it materializes the full expanded decimal string and reduces it
    /// digit by digit, independent of the folding in `fold_mod_97`.
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
