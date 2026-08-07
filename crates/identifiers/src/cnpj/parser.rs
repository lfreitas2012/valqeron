//! Turns raw, possibly formatted user input into a normalized 14-byte ASCII candidate ready
//! for [`super::validation`].
//!
//! This module only knows about *formatting*: stripping the punctuation conventionally used to
//! display a CNPJ (`.`, `/`, `-`, Unicode whitespace) and folding ASCII cases. It does not know which
//! characters are semantically valid at which position, that is [`super::validation`]'s job, since
//! it needs to distinguish the digit-only tail from the alphanumeric head.

use super::error::CnpjError;

/// Characters stripped from input before length/content checks apply.
#[inline]
fn is_formatting_char(c: char) -> bool {
    matches!(c, '.' | '/' | '-') || c.is_whitespace()
}

/// Normalizes `input` into a 14-byte ASCII array.
///
/// - Empty input is rejected as [`CnpjError::Empty`].
/// - Formatting characters (`.`, `/`, `-`, Unicode whitespace) are stripped anywhere they appear.
/// - Remaining characters are ASCII-uppercased (so lowercase letters in the alphanumeric portion
///   are accepted transparently).
/// - Any remaining non-ASCII character, or a meaningful-character count other than 14, is rejected.
///
/// This function does **not** check that each position holds a character valid for that position
/// (digit vs. alphanumeric); see [`super::validation::validate`] for that.
pub(super) fn normalize(input: &str) -> Result<[u8; 14], CnpjError> {
    if input.is_empty() {
        return Err(CnpjError::Empty);
    }

    let meaningful = input.chars().filter(|&c| !is_formatting_char(c));
    let found = meaningful.clone().count();
    if found != 14 {
        return Err(CnpjError::InvalidLength { found });
    }

    let mut buf = [0u8; 14];
    for (i, ch) in meaningful.enumerate() {
        if !ch.is_ascii() {
            return Err(CnpjError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
                expected: super::error::CharacterClass::Alphanumeric,
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
