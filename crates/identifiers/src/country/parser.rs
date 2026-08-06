//! Turns raw user input into a normalized two byte ASCII candidate ready for [`super::validation`].
//!
//! This module only knows about formatting. It trims surrounding whitespace a code might pick up
//! from a spreadsheet cell or a CSV column, and it folds ASCII case. A country code has no internal
//! punctuation, so nothing is stripped from the interior: an interior space or separator is left in
//! place and rejected later as an invalid character.
//!
//! Deciding which characters are valid, and whether the two letters name an assigned code, is the
//! job of [`super::validation`], not this module.

use super::error::CountryCodeError;

/// Normalizes `input` into a two byte ASCII array.
///
/// The steps are:
///
/// * Empty input is rejected as [`CountryCodeError::Empty`].
/// * Leading and trailing whitespace is trimmed. Interior characters are left untouched.
/// * Remaining characters are ASCII uppercased, so a lowercase code is accepted transparently.
/// * Any non ASCII character, or a character count other than two after trimming, is rejected.
///
/// This function does not check the character class of each position, nor whether the two letters
/// name an assigned code. See [`super::validation::validate`] for that.
pub(super) fn normalize(input: &str) -> Result<[u8; 2], CountryCodeError> {
    if input.is_empty() {
        return Err(CountryCodeError::Empty);
    }

    let trimmed = input.trim();
    let found = trimmed.chars().count();
    if found != 2 {
        return Err(CountryCodeError::InvalidLength { found });
    }

    let mut buf = [0u8; 2];
    for (i, ch) in trimmed.chars().enumerate() {
        if !ch.is_ascii() {
            return Err(CountryCodeError::InvalidCharacter {
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
        assert_eq!(normalize(""), Err(CountryCodeError::Empty));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(normalize("  US "), normalize("US"));
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(normalize("us").unwrap(), *b"US");
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            normalize("USA"),
            Err(CountryCodeError::InvalidLength { found: 3 })
        );
    }

    #[test]
    fn whitespace_only_is_a_length_error() {
        assert_eq!(
            normalize("   "),
            Err(CountryCodeError::InvalidLength { found: 0 })
        );
    }

    #[test]
    fn keeps_non_letter_characters_for_validation() {
        // A non letter that is not surrounding whitespace survives normalization (the count is
        // still two) and is left for validation to reject as an invalid character.
        assert_eq!(normalize("U.").unwrap(), *b"U.");
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("U£").unwrap_err();
        assert!(matches!(
            err,
            CountryCodeError::InvalidCharacter {
                character: '£', ..
            }
        ));
    }
}
