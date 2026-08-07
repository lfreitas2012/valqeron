use core::fmt;

/// The set of reasons a CFI string can fail validation.
///
/// Every fallible constructor of [`Cfi`](super::Cfi) returns this type. CFI carries no checksum;
/// instead, its validity is defined by the ISO 10962 code taxonomy, so beyond the structural checks
/// there are three *taxonomic* failure modes ([`CfiError::UnknownCategory`], [`CfiError::UnknownGroup`],
/// [`CfiError::InvalidAttribute`]). Each variant maps to a single, specific failure, so callers can
/// react programmatically rather than parsing a message.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfiError {
    /// The input was an empty string.
    Empty,

    /// After trimming surrounding whitespace, the input did not contain exactly 6 characters.
    InvalidLength {
        /// The number of characters found after trimming.
        found: usize,
    },

    /// A character outside `A`–`Z` was found at a given position. Every CFI position is an
    /// uppercase ASCII letter.
    InvalidCharacter {
        /// The offending character, as originally provided (before case folding).
        character: char,
        /// 1-indexed position within the 6 characters.
        position: u8,
    },

    /// The category letter (position 1) is not one defined by ISO 10962.
    UnknownCategory {
        /// The offending category code.
        code: char,
    },

    /// The group letter (position 2) is not defined for the otherwise-valid category.
    UnknownGroup {
        /// The (valid) category code the group was looked up under.
        category: char,
        /// The offending group code.
        code: char,
    },

    /// An attribute letter (positions 3–6) is not permitted for the resolved category and group at
    /// that attribute position.
    InvalidAttribute {
        /// The (valid) category code.
        category: char,
        /// The (valid) group code.
        group: char,
        /// Which attribute failed, 1–4 (corresponding to CFI positions 3–6).
        index: u8,
        /// The offending attribute code.
        code: char,
    },
}

impl fmt::Display for CfiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CfiError::Empty => f.write_str("CFI input is empty"),
            CfiError::InvalidLength { found } => {
                write!(f, "CFI must contain exactly 6 characters, found {found}")
            }
            CfiError::InvalidCharacter {
                character,
                position,
            } => write!(
                f,
                "invalid character '{character}' at position {position} of 6: expected an uppercase letter (A-Z)"
            ),
            CfiError::UnknownCategory { code } => {
                write!(f, "unknown CFI category '{code}' at position 1")
            }
            CfiError::UnknownGroup { category, code } => write!(
                f,
                "unknown CFI group '{code}' at position 2 for category '{category}'"
            ),
            CfiError::InvalidAttribute {
                category,
                group,
                index,
                code,
            } => write!(
                f,
                "invalid CFI attribute '{code}' at position {} (attribute {index}) for category '{category}' group '{group}'",
                index + 2
            ),
        }
    }
}

impl core::error::Error for CfiError {}
