//! Turns raw user input into a normalized four-byte ASCII candidate ready for [`super::validation`].
//!
//! This module only knows about formatting. It trims surrounding whitespace a code might pick up
//! from a spreadsheet cell or a CSV column, and it folds an ASCII case. A MIC has no internal
//! punctuation, so nothing is stripped from the interior: an interior space or separator is left
//! in place and rejected later as an invalid character.
//!
//! Deciding which characters are valid, and whether the four characters name a registered code, is
//! the job of [`super::validation`], not this module.

use super::error::MicError;

/// Normalizes `input` into a four-byte ASCII array.
///
/// The steps are:
///
/// * Empty input is rejected as [`MicError::Empty`].
/// * Leading and trailing whitespace is trimmed. Interior characters are left untouched.
/// * Remaining characters are ASCII uppercased, so a lowercase code is accepted transparently.
/// * Any non-ASCII character, or a character count other than four after trimming, is rejected.
///
/// This function does not check the character class of each position, nor whether the four
/// characters name a registered code. See [`super::validation::validate`] for that.
pub(super) fn normalize(input: &str) -> Result<[u8; 4], MicError> {
    if input.is_empty() {
        return Err(MicError::Empty);
    }

    let trimmed = input.trim();
    let found = trimmed.chars().count();
    if found != 4 {
        return Err(MicError::InvalidLength { found });
    }

    let mut buf = [0u8; 4];
    for (i, ch) in trimmed.chars().enumerate() {
        if !ch.is_ascii() {
            return Err(MicError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
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
        assert_eq!(normalize(""), Err(MicError::Empty));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(normalize("  XNYS "), normalize("XNYS"));
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(normalize("xnys").unwrap(), *b"XNYS");
    }

    #[test]
    fn keeps_digits() {
        assert_eq!(normalize("360t").unwrap(), *b"360T");
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(normalize("XNY"), Err(MicError::InvalidLength { found: 3 }));
        assert_eq!(
            normalize("XNYSE"),
            Err(MicError::InvalidLength { found: 5 })
        );
    }

    #[test]
    fn whitespace_only_is_a_length_error() {
        assert_eq!(normalize("   "), Err(MicError::InvalidLength { found: 0 }));
    }

    #[test]
    fn keeps_non_alphanumeric_characters_for_validation() {
        // A separator not surrounding whitespace survives normalization (the count is
        // still four) and is left for validation to reject as an invalid character.
        assert_eq!(normalize("XN.S").unwrap(), *b"XN.S");
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("XNY£").unwrap_err();
        assert!(matches!(
            err,
            MicError::InvalidCharacter {
                character: '£', ..
            }
        ));
    }
}
