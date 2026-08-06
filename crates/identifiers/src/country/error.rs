use core::fmt;

/// The set of reasons a country code string can fail validation.
///
/// Every fallible constructor of [`CountryCode`](super::CountryCode) returns this type. A country
/// code carries no checksum. Its validity is defined by membership in the officially assigned ISO
/// 3166-1 alpha-2 set, so beyond the structural checks there is one assignment failure mode
/// ([`CountryCodeError::Unassigned`]). Each variant maps to a single, specific failure, so callers
/// can react programmatically rather than parsing a message.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountryCodeError {
    /// The input was an empty string.
    Empty,

    /// After trimming surrounding whitespace, the input did not contain exactly two characters.
    InvalidLength {
        /// The number of characters found after trimming.
        found: usize,
    },

    /// A character outside `A` to `Z` was found at a given position. Every position of a country
    /// code is an uppercase ASCII letter.
    InvalidCharacter {
        /// The offending character, as originally provided (before case folding).
        character: char,
        /// The 1 indexed position within the two characters.
        position: u8,
    },

    /// The two letters are well formed but are not a code that ISO 3166-1 officially assigns.
    Unassigned {
        /// The offending code, as two uppercase letters.
        code: [char; 2],
    },
}

impl fmt::Display for CountryCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CountryCodeError::Empty => f.write_str("country code input is empty"),
            CountryCodeError::InvalidLength { found } => {
                write!(
                    f,
                    "country code must contain exactly 2 characters, found {found}"
                )
            }
            CountryCodeError::InvalidCharacter {
                character,
                position,
            } => write!(
                f,
                "invalid character '{character}' at position {position} of 2: expected an uppercase letter (A to Z)"
            ),
            CountryCodeError::Unassigned { code } => write!(
                f,
                "'{}{}' is not an officially assigned ISO 3166-1 alpha-2 code",
                code[0], code[1]
            ),
        }
    }
}

impl core::error::Error for CountryCodeError {}
