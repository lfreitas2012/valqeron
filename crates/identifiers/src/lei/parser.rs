//! Turns raw user input into a normalized 20-byte ASCII candidate ready for [`super::validation`].
//!
//! This module only knows about *formatting*: trimming the surrounding whitespace an LEI might
//! pick up from a spreadsheet cell or CSV column, and folding ASCII case. Like an ISIN (and unlike
//! a CNPJ), an LEI has no conventional internal punctuation, so nothing is stripped from the
//! interior. An interior space or separator is left in place and rejected later as an invalid
//! character.
//!
//! Deciding which characters are valid at which position (alphanumeric vs. digit) is
//! [`super::validation`]'s job, not this module's.

use super::error::{CharacterClass, LeiError};

/// Normalizes `input` into a 20-byte ASCII array.
///
/// - Empty input is rejected as [`LeiError::Empty`].
/// - Leading and trailing whitespace is trimmed; interior characters are left untouched.
/// - Remaining characters are ASCII-uppercased (so a lowercase LEI is accepted transparently).
/// - Any non-ASCII character, or a character count other than 20 after trimming, is rejected.
///
/// This function does **not** check that each position holds a character valid for that position
/// (alphanumeric vs. digit); see [`super::validation::validate`] for that.
pub(super) fn normalize(input: &str) -> Result<[u8; 20], LeiError> {
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
            return Err(LeiError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
                expected: CharacterClass::Alphanumeric,
            });
        }
        buf[i] = ch.to_ascii_uppercase() as u8;
    }

    Ok(buf)
}

#[cfg(test)]
mod tests {
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
            normalize("5493000IBP32UQZ0KL2 ".replace(' ', "_").as_str()),
            Ok(*b"5493000IBP32UQZ0KL2_")
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
