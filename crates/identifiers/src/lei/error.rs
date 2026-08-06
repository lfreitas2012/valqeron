use core::fmt;

/// The class of characters permitted at a given position of an LEI.
/// Reported by [`LeiError::InvalidCharacter`] to describe what was expected where an invalid
/// character was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharacterClass {
    /// An ASCII digit, `'0'...='9'` (the two check digits at positions 19-20).
    Digit,
    /// An ASCII digit or an uppercase ASCII letter, `'0'...='9' | 'A'...='Z'` (positions 1-18: the
    /// LOU prefix and the entity-specific part).
    Alphanumeric,
}

impl fmt::Display for CharacterClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CharacterClass::Digit => write!(f, "a digit (0-9)"),
            CharacterClass::Alphanumeric => {
                write!(f, "a digit (0-9) or an uppercase letter (A-Z)")
            }
        }
    }
}

/// The set of reasons an LEI string can fail validation.
/// Every fallible constructor of [`Lei`](super::Lei) returns this type; each variant maps to a
/// single, specific failure so callers can react programmatically (for example, highlighting the
/// offending character in a form field) rather than parsing a human-readable message.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeiError {
    /// The input was an empty string.
    Empty,

    /// After trimming surrounding whitespace, the input did not contain exactly 20 characters.
    InvalidLength {
        /// The number of characters found after trimming.
        found: usize,
    },

    /// A character outside the allowed set was found at a given position.
    InvalidCharacter {
        /// The offending character, as originally provided (before case folding).
        character: char,
        /// 1-indexed position within the 20 characters.
        position: u8,
        /// The character class that was expected at this position.
        expected: CharacterClass,
    },

    /// The two check digits (positions 19-20) did not satisfy the ISO/IEC 7064 MOD 97-10 checksum
    /// required by ISO 17442.
    ///
    /// `expected` is the two-digit value the algorithm derives from the first 18 characters;
    /// `found` is the value actually present in the input. Both are in the range `0...=99`.
    InvalidCheckDigits {
        /// The check-digit value computed by the ISO/IEC 7064 MOD 97-10 algorithm (`0...=99`).
        expected: u8,
        /// The check-digit value actually present in the input (`0...=99`).
        found: u8,
    },
}

impl fmt::Display for LeiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeiError::Empty => f.write_str("LEI input is empty"),
            LeiError::InvalidLength { found } => {
                write!(f, "LEI must contain exactly 20 characters, found {found}")
            }
            LeiError::InvalidCharacter {
                character,
                position,
                expected,
            } => write!(
                f,
                "invalid character '{character}' at position {position} of 20: expected {expected}"
            ),
            LeiError::InvalidCheckDigits { expected, found } => write!(
                f,
                "invalid check digits at positions 19-20 of 20: expected {expected:02}, found {found:02}"
            ),
        }
    }
}

impl core::error::Error for LeiError {}
