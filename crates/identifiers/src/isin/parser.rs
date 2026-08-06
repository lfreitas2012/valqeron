//! Turns raw user input into a normalized 12-byte ASCII candidate ready for
//! [`super::validation`].
//!
//! This module only knows about *formatting*: trimming the surrounding whitespace an ISIN might
//! pick up from a spreadsheet cell or CSV column, and folding ASCII case. Unlike a CNPJ, an ISIN
//! has no conventional internal punctuation, so nothing is stripped from the interior — an interior
//! space or separator is left in place and rejected later as an invalid character.
//!
//! Deciding which characters are valid at which position (letter, digit, or either) is
//! [`super::validation`]'s job, not this module's.

use super::error::{CharacterClass, IsinError};

/// Normalizes `input` into a 12-byte ASCII array.
///
/// - Empty input is rejected as [`IsinError::Empty`].
/// - Leading and trailing whitespace is trimmed; interior characters are left untouched.
/// - Remaining characters are ASCII-uppercased (so a lowercase ISIN is accepted transparently).
/// - Any non-ASCII character, or a character count other than 12 after trimming, is rejected.
///
/// This function does **not** check that each position holds a character valid for that position
/// (letter vs. digit vs. alphanumeric); see [`super::validation::validate`] for that.
pub(super) fn normalize(input: &str) -> Result<[u8; 12], IsinError> {
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
            return Err(IsinError::InvalidCharacter {
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
                character: '£', ..
            }
        ));
    }
}
