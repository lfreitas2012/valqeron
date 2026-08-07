//! Structural and taxonomic validation of an already-normalized 6-byte CFI candidate (ASCII,
//! uppercase, surrounding whitespace already trimmed).
//!
//! This module has no knowledge of the original user input or of formatting; that is
//! [`super::parser`]'s job. Everything here operates on a `[u8; 6]`.
//!
//! # Taxonomy, not checksum
//!
//! A CFI has no check digit. A CFI is valid exactly when its letters describe a combination defined
//! by ISO 10962: a known category (position 1), a known group within it (position 2), and, for each
//! of the four attribute positions (3–6), a code the standard permits for that category and group.
//! Those permitted combinations are embedded as the generated, lookup table
//! in [`super::table`]; validation is a handful of allocation-free binary searches and bitmask
//! tests against it.

use super::error::CfiError;
use super::table::{CATEGORIES, CategoryEntry, GroupEntry};

/// Runs every validation rule against a normalized candidate, cheapest first:
/// 1. Character class — all six positions must be uppercase ASCII letters.
/// 2. Taxonomy — the category, group, and four attribute codes must exist in ISO 10962.
pub(super) fn validate(candidate: &[u8; 6]) -> Result<(), CfiError> {
    validate_character_classes(candidate)?;
    validate_taxonomy(candidate)?;
    Ok(())
}

fn validate_character_classes(candidate: &[u8; 6]) -> Result<(), CfiError> {
    for (i, &byte) in candidate.iter().enumerate() {
        if !byte.is_ascii_uppercase() {
            return Err(CfiError::InvalidCharacter {
                character: byte as char,
                position: (i + 1) as u8,
            });
        }
    }
    Ok(())
}

fn validate_taxonomy(candidate: &[u8; 6]) -> Result<(), CfiError> {
    let category_code = candidate[0];
    let category = find_category(category_code).ok_or(CfiError::UnknownCategory {
        code: category_code as char,
    })?;

    let group_code = candidate[1];
    let group = find_group(category, group_code).ok_or(CfiError::UnknownGroup {
        category: category_code as char,
        code: group_code as char,
    })?;

    for (i, &code) in candidate[2..].iter().enumerate() {
        if !attr_allows(group.attrs[i], code) {
            return Err(CfiError::InvalidAttribute {
                category: category_code as char,
                group: group_code as char,
                index: (i + 1) as u8,
                code: code as char,
            });
        }
    }

    Ok(())
}

/// Looks up a category by its code (position 1) via binary search over the sorted table.
pub(super) fn find_category(code: u8) -> Option<&'static CategoryEntry> {
    let index = CATEGORIES.binary_search_by_key(&code, |c| c.code).ok()?;
    Some(&CATEGORIES[index])
}

/// Looks up a group by its code (position 2) within a category via binary search.
pub(super) fn find_group(
    category: &'static CategoryEntry,
    code: u8,
) -> Option<&'static GroupEntry> {
    let index = category
        .groups
        .binary_search_by_key(&code, |g| g.code)
        .ok()?;
    Some(&category.groups[index])
}

/// Returns `true` when `code` (an uppercase ASCII letter) is permitted by an attribute bitmask.
///
/// Only correct for `b'A'...=b'Z'`; callers must have passed character-class validation first.
#[inline]
fn attr_allows(mask: u32, code: u8) -> bool {
    (mask >> (code - b'A')) & 1 == 1
}

/// Returns the `n`-th permitted letter of a (non-empty) attribute bitmask, wrapping by the number
/// of set bits. Used by the `arbitrary`/`proptest` generators to build taxonomically valid CFIs
/// without duplicating the table; hence the `dead_code` allowance when neither is enabled.
#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
pub(super) fn nth_letter(mask: u32, n: usize) -> u8 {
    let count = mask.count_ones() as usize;
    let target = n % count;
    (0u8..26)
        .filter(|bit| (mask >> bit) & 1 == 1)
        .nth(target)
        .map(|bit| b'A' + bit)
        .expect("attribute masks in the generated table are always non-zero")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(s: &str) -> [u8; 6] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 6];
        out.copy_from_slice(bytes);
        out
    }

    #[test]
    fn accepts_known_valid_cfis() {
        for s in [
            "ESVUFR", // equity / common share, voting, free, fully paid, registered
            "ESVTOB", // equity / common share, another valid attribute combination
            "DBFTFB", // debt / bond
            "MCATXB", // miscellaneous-form / currencies
            "OCASNS", // listed option / call
        ] {
            assert!(validate(&candidate(s)).is_ok(), "{s} should be valid");
        }
    }

    #[test]
    fn rejects_unknown_category() {
        let err = validate(&candidate("QSVUFR")).unwrap_err();
        assert_eq!(err, CfiError::UnknownCategory { code: 'Q' });
    }

    #[test]
    fn rejects_unknown_group() {
        let err = validate(&candidate("EZVUFR")).unwrap_err();
        assert_eq!(
            err,
            CfiError::UnknownGroup {
                category: 'E',
                code: 'Z',
            }
        );
    }

    #[test]
    fn rejects_invalid_attribute() {
        // Category E, group S permits attribute 1 in {E,N,R,V}; 'X' is not among them.
        let err = validate(&candidate("ESXUFR")).unwrap_err();
        assert_eq!(
            err,
            CfiError::InvalidAttribute {
                category: 'E',
                group: 'S',
                index: 1,
                code: 'X',
            }
        );
    }

    #[test]
    fn rejects_lowercase_as_character_class() {
        let err = validate(&candidate("esvufr")).unwrap_err();
        assert_eq!(
            err,
            CfiError::InvalidCharacter {
                character: 'e',
                position: 1,
            }
        );
    }

    #[test]
    fn rejects_digit_as_character_class() {
        let err = validate(&candidate("ESVUF1")).unwrap_err();
        assert_eq!(
            err,
            CfiError::InvalidCharacter {
                character: '1',
                position: 6,
            }
        );
    }
}
