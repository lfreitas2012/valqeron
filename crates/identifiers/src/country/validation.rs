//! Structural and membership validation of an already normalized two byte country code candidate
//! (ASCII, uppercase, surrounding whitespace already trimmed).
//!
//! This module has no knowledge of the original user input or of formatting. That is the job of
//! [`super::parser`]. Everything here operates on a `[u8; 2]`.
//!
//! # Membership, not checksum
//!
//! A country code has no check digit. It is valid exactly when its two letters name a code that ISO
//! 3166-1 officially assigns. That set is embedded as the compile time bitmap in [`super::table`],
//! so the assignment test is a single array index followed by a single bit test.

use super::error::CountryCodeError;
use super::table::{ASSIGNED, bit_index};

/// Runs every validation rule against a normalized candidate, cheapest first:
///
/// 1. Character class: both positions must be uppercase ASCII letters.
/// 2. Membership: the two letters together must name an officially assigned code.
pub(super) fn validate(candidate: &[u8; 2]) -> Result<(), CountryCodeError> {
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
        Err(CountryCodeError::Unassigned {
            code: [candidate[0] as char, candidate[1] as char],
        })
    }
}

/// Returns `true` when `candidate` names an officially assigned code.
///
/// The caller must have passed character class validation first, so both bytes are `b'A'..=b'Z'`
/// and the computed index is always within `ASSIGNED`.
#[inline]
fn is_assigned(candidate: &[u8; 2]) -> bool {
    debug_assert!(candidate[0].is_ascii_uppercase() && candidate[1].is_ascii_uppercase());
    let index = bit_index(*candidate);
    (ASSIGNED[index / 64] >> (index % 64)) & 1 == 1
}

#[cfg(test)]
mod tests {
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
        assert_eq!(err, CountryCodeError::Unassigned { code: ['Z', 'Z'] });
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
