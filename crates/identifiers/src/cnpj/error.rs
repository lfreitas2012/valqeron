use core::fmt;

/// The set of characters permitted at a given position of a CNPJ.
///
/// Reported by [`CnpjError::InvalidCharacter`] to describe what was expected where an invalid
/// character was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharacterClass {
    /// An ASCII digit, `'0'...='9'`.
    Digit,
    /// An ASCII digit or an uppercase ASCII letter, `'0'...='9' | 'A'...='Z'`.
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

/// The error returned when a [`Cnpj`](crate::Cnpj) fails to parse or validate.
///
/// Each variant corresponds to one validation rule, in the order the rules are applied. See the
/// [module level documentation](crate::cnpj) for the full rule list.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CnpjError {
    /// The input was an empty string.
    Empty,

    /// After stripping punctuation (`.`, `/`, `-`, whitespace), the input did not contain exactly
    /// 14 meaningful characters.
    InvalidLength {
        /// The number of meaningful (non-punctuation) characters found.
        found: usize,
    },

    /// A character outside the allowed set was found at a given position.
    InvalidCharacter {
        /// The offending character, as originally provided (before case folding).
        character: char,
        /// 1-indexed position within the 14 meaningful characters.
        position: u8,
        /// The character class that was expected at this position.
        expected: CharacterClass,
    },

    /// The Módulo 11 checksum did not match one of the two verification digits.
    InvalidCheckDigits {
        /// 1-indexed position of the mismatching verification digit (13 or 14).
        position: u8,
        /// The verification digit computed from the Módulo 11 algorithm.
        expected: u8,
        /// The verification digit actually present in the input.
        found: u8,
    },

    /// All 14 characters were identical (e.g. `"00000000000000"`).
    ///
    /// Such inputs are structurally well-formed and can even satisfy the Módulo 11 checksum for
    /// certain repeated digits, but the Receita Federal never issues them;
    /// they are reliably placeholder or data-entry artifacts.
    RepeatedDigits,
}

impl fmt::Display for CnpjError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CnpjError::Empty => f.write_str("CNPJ input is empty"),
            CnpjError::InvalidLength { found } => write!(
                f,
                "CNPJ must contain exactly 14 characters once formatting is removed, found {found}"
            ),
            CnpjError::InvalidCharacter {
                character,
                position,
                expected,
            } => write!(
                f,
                "invalid character '{character}' at position {position} of 14: expected {expected}"
            ),
            CnpjError::InvalidCheckDigits {
                position,
                expected,
                found,
            } => write!(
                f,
                "invalid check digit at position {position} of 14: expected {expected}, found {found}"
            ),
            CnpjError::RepeatedDigits => {
                f.write_str("CNPJ cannot consist of a single character repeated 14 times")
            }
        }
    }
}

impl core::error::Error for CnpjError {}
