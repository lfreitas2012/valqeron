use core::fmt;

/// The class of characters permitted at a given position of an ISIN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharacterClass {
    /// An ASCII digit, `'0'...='9'` (the check digit at position 12).
    Digit,
    /// An uppercase ASCII letter, `'A'...='Z'` (the two-character country code).
    Letter,
    /// An ASCII digit or an uppercase ASCII letter, `'0'...='9' | 'A'...='Z'` (the NSIN body).
    Alphanumeric,
}

impl fmt::Display for CharacterClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CharacterClass::Digit => write!(f, "a digit (0-9)"),
            CharacterClass::Letter => write!(f, "an uppercase letter (A-Z)"),
            CharacterClass::Alphanumeric => {
                write!(f, "a digit (0-9) or an uppercase letter (A-Z)")
            }
        }
    }
}

/// The set of reasons an ISIN string can fail validation.
///
/// Every fallible constructor of [`Isin`](super::Isin) returns this type; each variant maps to a
/// single, specific failure so callers can react programmatically (for example, highlighting the
/// offending character in a form field) rather than parsing a human-readable message.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsinError {
    /// The input was an empty string.
    Empty,

    /// After trimming surrounding whitespace, the input did not contain exactly 12 characters.
    InvalidLength {
        /// The number of characters found after trimming.
        found: usize,
    },

    /// A character outside the allowed set was found at a given position.
    InvalidCharacter {
        /// The offending character, as originally provided (before case folding).
        character: char,
        /// 1-indexed position within the 12 characters.
        position: u8,
        /// The character class that was expected at this position.
        expected: CharacterClass,
    },

    /// The Luhn check digit (position 12) did not match the value computed from the first 11
    /// characters.
    InvalidCheckDigit {
        /// The check digit computed by the ISO 6166 Luhn algorithm.
        expected: u8,
        /// The check digit actually present in the input.
        found: u8,
    },
}

impl fmt::Display for IsinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IsinError::Empty => f.write_str("ISIN input is empty"),
            IsinError::InvalidLength { found } => {
                write!(f, "ISIN must contain exactly 12 characters, found {found}")
            }
            IsinError::InvalidCharacter {
                character,
                position,
                expected,
            } => write!(
                f,
                "invalid character '{character}' at position {position} of 12: expected {expected}"
            ),
            IsinError::InvalidCheckDigit { expected, found } => write!(
                f,
                "invalid check digit at position 12 of 12: expected {expected}, found {found}"
            ),
        }
    }
}

impl core::error::Error for IsinError {}
