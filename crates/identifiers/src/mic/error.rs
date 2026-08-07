use core::fmt;

/// The set of reasons a market identifier code string can fail validation.
///
/// Every fallible constructor of [`Mic`](super::Mic) returns this type. A MIC carries no checksum.
/// Its validity is defined by membership in the ISO 10383 registry, so beyond the structural
/// checks there is one membership failure mode ([`MicError::Unregistered`]). Each variant maps to
/// a single, specific failure, so callers can react programmatically rather than parsing a
/// message.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MicError {
    /// The input was an empty string.
    Empty,

    /// After trimming surrounding whitespace, the input did not contain exactly four characters.
    InvalidLength {
        /// The number of characters found after trimming.
        found: usize,
    },

    /// A character outside `A` to `Z` and `0` to `9` was found at a given position. Every position
    /// of a MIC is an uppercase ASCII letter or a decimal digit.
    InvalidCharacter {
        /// The offending character, as originally provided (before case folding).
        character: char,
        /// The 1 indexed position within the four characters.
        position: u8,
    },

    /// The four characters are well formed but are not a code registered in ISO 10383.
    Unregistered {
        /// The offending code, as four uppercase letters or digits.
        code: [char; 4],
    },
}

impl fmt::Display for MicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MicError::Empty => f.write_str("market identifier code input is empty"),
            MicError::InvalidLength { found } => {
                write!(
                    f,
                    "market identifier code must contain exactly 4 characters, found {found}"
                )
            }
            MicError::InvalidCharacter {
                character,
                position,
            } => write!(
                f,
                "invalid character '{character}' at position {position} of 4: expected an uppercase letter (A to Z) or a digit (0 to 9)"
            ),
            MicError::Unregistered { code } => write!(
                f,
                "'{}{}{}{}' is not a market identifier code registered in ISO 10383",
                code[0], code[1], code[2], code[3]
            ),
        }
    }
}

impl core::error::Error for MicError {}
