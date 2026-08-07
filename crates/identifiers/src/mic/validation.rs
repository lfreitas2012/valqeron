//! Structural and membership validation of an already normalized four-byte MIC candidate (ASCII,
//! uppercase, surrounding whitespace already trimmed).
//!
//! This module has no knowledge of the original user input or of formatting. That is the job of
//! [`super::parser`]. Everything here operates on a `[u8; 4]`.
//!
//! # Membership, not checksum
//!
//! A MIC has no check digit. It is valid exactly when its four characters name a code registered
//! in ISO 10383 (active or expired). That registry snapshot is embedded as the sorted table in
//! [`super::table`], so the membership test is a single binary search. [`find`] exposes the
//! matched entry to the accessors on [`Mic`](super::Mic), which report the registered facts about
//! a code (status, operating MIC, country).

use super::error::MicError;
use super::table::{ENTRIES, MicEntry};

/// Runs every validation rule against a normalized candidate, cheapest first:
///
/// 1. Character class: every position must be an uppercase ASCII letter or a decimal digit.
/// 2. Membership: the four characters together must name a code registered in ISO 10383.
pub(super) fn validate(candidate: &[u8; 4]) -> Result<(), MicError> {
    validate_character_classes(candidate)?;
    validate_membership(candidate)?;
    Ok(())
}

fn validate_character_classes(candidate: &[u8; 4]) -> Result<(), MicError> {
    for (i, &byte) in candidate.iter().enumerate() {
        if !byte.is_ascii_uppercase() && !byte.is_ascii_digit() {
            return Err(MicError::InvalidCharacter {
                character: byte as char,
                position: (i + 1) as u8,
            });
        }
    }
    Ok(())
}

fn validate_membership(candidate: &[u8; 4]) -> Result<(), MicError> {
    if find(candidate).is_some() {
        Ok(())
    } else {
        Err(MicError::Unregistered {
            code: [
                candidate[0] as char,
                candidate[1] as char,
                candidate[2] as char,
                candidate[3] as char,
            ],
        })
    }
}

/// Looks up the registry entry for a code, or `None` when the code is not registered.
///
/// The table is sorted by code (a compile time guard in [`super::table`] proves it), so this is a
/// binary search over the raw bytes.
#[inline]
pub(super) fn find(candidate: &[u8; 4]) -> Option<&'static MicEntry> {
    ENTRIES
        .binary_search_by_key(candidate, |entry| entry.code)
        .ok()
        .map(|index| &ENTRIES[index])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(s: &str) -> [u8; 4] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 4];
        out.copy_from_slice(bytes);
        out
    }

    #[test]
    fn accepts_known_registered_codes() {
        for s in ["XNYS", "XLON", "BVMF", "360T", "XOFF", "XXXX", "ALDP"] {
            assert!(validate(&candidate(s)).is_ok(), "{s} should be valid");
        }
    }

    #[test]
    fn accepts_expired_codes() {
        // `ALDP` (NYSE Alternext Dark) is expired but registered; membership is about the
        // registry, not the lifecycle state.
        let entry = find(&candidate("ALDP")).unwrap();
        assert!(!entry.active);
        assert!(validate(&candidate("ALDP")).is_ok());
    }

    #[test]
    fn rejects_unregistered_but_well_formed() {
        let err = validate(&candidate("ZZZZ")).unwrap_err();
        assert_eq!(
            err,
            MicError::Unregistered {
                code: ['Z', 'Z', 'Z', 'Z'],
            }
        );
    }

    #[test]
    fn rejects_lowercase_as_character_class() {
        let err = validate(&candidate("xnys")).unwrap_err();
        assert_eq!(
            err,
            MicError::InvalidCharacter {
                character: 'x',
                position: 1,
            }
        );
    }

    #[test]
    fn rejects_punctuation_as_character_class() {
        let err = validate(&candidate("XN.S")).unwrap_err();
        assert_eq!(
            err,
            MicError::InvalidCharacter {
                character: '.',
                position: 3,
            }
        );
    }

    #[test]
    fn find_resolves_operating_references() {
        // `ARCX` (NYSE Arca) is a segment of `XNYS`; its operating index must name that entry.
        let entry = find(&candidate("ARCX")).unwrap();
        let operating = &ENTRIES[usize::from(entry.operating)];
        assert_eq!(operating.code, *b"XNYS");
        assert_eq!(usize::from(operating.operating), {
            ENTRIES.binary_search_by_key(b"XNYS", |e| e.code).unwrap()
        });
    }
}
