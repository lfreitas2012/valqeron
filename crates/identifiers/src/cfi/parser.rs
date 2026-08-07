//! Turns raw user input into a normalized 6-byte ASCII candidate ready for [`super::validation`].
//!
//! This module only knows about *formatting*: trimming surrounding Unicode whitespace a CFI might pick up
//! from a spreadsheet cell or CSV column, and folding an ASCII case. A CFI has no conventional
//! internal punctuation, so nothing is stripped from the interior — an interior space or separator
//! is left in place and rejected later as an invalid character.
//!
//! Deciding which characters are valid and whether the code exists in the ISO 10962 taxonomy is
//! [`super::validation`]'s job, not this module's.

use super::error::CfiError;

/// Normalizes `input` into a 6-byte ASCII array.
///
/// - Empty input is rejected as [`CfiError::Empty`].
/// - Leading and trailing Unicode whitespace is trimmed; interior characters are left untouched.
/// - Remaining characters are ASCII-uppercased (so a lowercase CFI is accepted transparently).
/// - Any non-ASCII character, or a character count other than 6 after trimming, is rejected.
///
/// This function does **not** check the character class of each position, nor whether the code is
/// a valid ISO 10962 combination; see [`super::validation::validate`] for that.
pub(super) fn normalize(input: &str) -> Result<[u8; 6], CfiError> {
    if input.is_empty() {
        return Err(CfiError::Empty);
    }

    let trimmed = input.trim();
    let found = trimmed.chars().count();
    if found != 6 {
        return Err(CfiError::InvalidLength { found });
    }

    let mut buf = [0u8; 6];
    for (i, ch) in trimmed.chars().enumerate() {
        if !ch.is_ascii() {
            return Err(CfiError::InvalidCharacter {
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
        assert_eq!(normalize(""), Err(CfiError::Empty));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(normalize("  ESVUFR \t\n"), normalize("ESVUFR"));
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(normalize("esvufr").unwrap(), *b"ESVUFR");
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(normalize("ESVU"), Err(CfiError::InvalidLength { found: 4 }));
    }

    #[test]
    fn whitespace_only_is_a_length_error() {
        assert_eq!(normalize("   "), Err(CfiError::InvalidLength { found: 0 }));
    }

    #[test]
    fn keeps_interior_characters_for_validation() {
        // An interior space survives normalization (count is still 6) and is left for
        // `validation` to reject as a non-letter character.
        assert_eq!(normalize("ES VFR").unwrap(), *b"ES VFR");
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("ESVUF£").unwrap_err();
        assert!(matches!(
            err,
            CfiError::InvalidCharacter {
                character: '£', ..
            }
        ));
    }

    #[test]
    fn trims_non_ascii_whitespace() {
        assert_eq!(normalize("\u{00A0}ESVUFR\u{00A0}"), normalize("ESVUFR"));
    }
}
