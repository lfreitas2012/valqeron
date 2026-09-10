use core::convert::TryFrom;
use core::str::{FromStr, from_utf8_unchecked};
use fmt::{Debug, Display};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use valqeron_macros::generate_cfi_table;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "A parsed CFI must be used."]
pub struct Cfi {
    bytes: [u8; 6],
}

impl Cfi {
    /// Parses a string into a [`Cfi`].
    ///
    /// # Errors
    ///
    /// Returns a [`CfiError`] if the input is empty, has an invalid length,
    /// contains non-ASCII characters, or fails taxonomy validation.
    pub fn parse(input: &str) -> Result<Self, CfiError> {
        let candidate = normalize(input)?;
        Self::from_bytes(candidate)
    }

    /// Creates a [`Cfi`] from a 6-byte array.
    ///
    /// # Errors
    ///
    /// Returns a [`CfiError`] if any byte is not a valid uppercase ASCII letter
    /// or if the bytes fail taxonomy validation.
    pub fn from_bytes(bytes: [u8; 6]) -> Result<Self, CfiError> {
        validate(bytes)?;
        Ok(Cfi { bytes })
    }

    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 6] {
        &self.bytes
    }

    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `Cfi::from_bytes` guarantees every byte is an uppercase ASCII letter.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }

    #[inline]
    #[must_use]
    pub fn category(&self) -> char {
        let [c, ..] = self.bytes;
        char::from(c)
    }

    #[inline]
    #[must_use]
    pub fn group(&self) -> char {
        let [_, g, ..] = self.bytes;
        char::from(g)
    }

    #[inline]
    #[must_use]
    pub fn attributes(&self) -> [char; 4] {
        let [_, _, a1, a2, a3, a4] = self.bytes;
        [
            char::from(a1),
            char::from(a2),
            char::from(a3),
            char::from(a4),
        ]
    }
}

generate_cfi_table!("data/cfi.json");

struct CfiGroupEntry {
    pub code: u8,
    pub attrs: [u32; 4],
}

struct CfiCategoryEntry {
    pub code: u8,
    pub groups: &'static [CfiGroupEntry],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CfiError {
    #[error("empty CFI")]
    Empty,

    #[error("CFI must contain exactly 6 characters, found {found}")]
    InvalidLength { found: usize },

    #[error(
        "invalid character '{character}' at position {position} of 6: expected an uppercase letter (A-Z) ASCII character"
    )]
    InvalidCharacter { character: char, position: u8 },

    #[error("unknown CFI category '{code}' at position 1")]
    UnknownCategory { code: char },

    #[error("unknown CFI group '{code}' at position 2")]
    UnknownGroup { category: char, code: char },

    #[error("invalid attribute '{code}' at position {index} of 6")]
    InvalidAttribute {
        category: char,
        group: char,
        index: u8,
        code: char,
    },
}

impl FromStr for Cfi {
    type Err = CfiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Cfi {
    type Error = CfiError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 6]> for Cfi {
    type Error = CfiError;

    fn try_from(value: [u8; 6]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for Cfi {
    type Error = CfiError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 6] = value
            .try_into()
            .map_err(|_| CfiError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
    }
}

impl PartialEq<str> for Cfi {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Cfi {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Cfi> for str {
    fn eq(&self, other: &Cfi) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Cfi> for &str {
    fn eq(&self, other: &Cfi) -> bool {
        *self == other.as_str()
    }
}

impl AsRef<[u8]> for Cfi {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for Cfi {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for Cfi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Debug for Cfi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Cfi").field(&self.as_str()).finish()
    }
}

impl Serialize for Cfi {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Cfi {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        Cfi::parse(s).map_err(serde::de::Error::custom)
    }
}

fn normalize(input: &str) -> Result<[u8; 6], CfiError> {
    if input.is_empty() {
        return Err(CfiError::Empty);
    }

    let trimmed = input.trim();
    let found = trimmed.chars().count();
    if found != 6 {
        return Err(CfiError::InvalidLength { found });
    }

    let mut buf = [0u8; 6];
    for (i, ch) in trimmed.chars().enumerate() {
        if !ch.is_ascii() {
            let position = u8::try_from(i).unwrap_or_default().saturating_add(1);
            return Err(CfiError::InvalidCharacter {
                character: ch,
                position,
            });
        }
        if let Some(b) = buf.get_mut(i) {
            *b = u8::try_from(u32::from(ch.to_ascii_uppercase())).unwrap_or_default();
        }
    }

    Ok(buf)
}

fn validate(candidate: [u8; 6]) -> Result<(), CfiError> {
    validate_character_classes(candidate)?;
    validate_taxonomy(candidate)?;
    Ok(())
}

fn validate_character_classes(candidate: [u8; 6]) -> Result<(), CfiError> {
    for (i, &byte) in candidate.iter().enumerate() {
        if !byte.is_ascii_uppercase() {
            let position = u8::try_from(i).unwrap_or_default().saturating_add(1);
            return Err(CfiError::InvalidCharacter {
                character: char::from(byte),
                position,
            });
        }
    }
    Ok(())
}

fn validate_taxonomy(candidate: [u8; 6]) -> Result<(), CfiError> {
    let [c_cat, c_grp, a1, a2, a3, a4] = candidate;

    let category = find_category(c_cat).ok_or(CfiError::UnknownCategory {
        code: char::from(c_cat),
    })?;

    let group = find_group(category, c_grp).ok_or(CfiError::UnknownGroup {
        category: char::from(c_cat),
        code: char::from(c_grp),
    })?;

    let attrs = [a1, a2, a3, a4];
    for (i, &code) in attrs.iter().enumerate() {
        let mask = group.attrs.get(i).copied().unwrap_or_default();
        if !attr_allows(mask, code) {
            let index = u8::try_from(i).unwrap_or_default().saturating_add(1);
            return Err(CfiError::InvalidAttribute {
                category: char::from(c_cat),
                group: char::from(c_grp),
                index,
                code: char::from(code),
            });
        }
    }

    Ok(())
}

fn find_category(code: u8) -> Option<&'static CfiCategoryEntry> {
    let index = CFI_CATEGORIES
        .binary_search_by_key(&code, |c| c.code)
        .ok()?;
    CFI_CATEGORIES.get(index)
}

fn find_group(category: &'static CfiCategoryEntry, code: u8) -> Option<&'static CfiGroupEntry> {
    let index = category
        .groups
        .binary_search_by_key(&code, |g| g.code)
        .ok()?;
    category.groups.get(index)
}

#[inline]
fn attr_allows(mask: u32, code: u8) -> bool {
    let shift = u32::from(code.saturating_sub(b'A'));
    mask.checked_shr(shift).unwrap_or_default() & 1 == 1
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // ---- normalization (`normalize`) ----

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize(""), Err(CfiError::Empty));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(
            normalize("  ESVUFR \t\n").unwrap(),
            normalize("ESVUFR").unwrap()
        );
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(normalize("esvufr").unwrap(), *b"ESVUFR");
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(normalize("ESVU"), Err(CfiError::InvalidLength { found: 4 }));
    }

    #[test]
    fn whitespace_only_is_a_length_error() {
        assert_eq!(normalize("   "), Err(CfiError::InvalidLength { found: 0 }));
    }

    #[test]
    fn keeps_interior_characters_for_validation() {
        assert_eq!(normalize("ES VFR").unwrap(), *b"ES VFR");
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("ESVUF£");
        assert!(matches!(
            err,
            Err(CfiError::InvalidCharacter {
                character: '£', ..
            })
        ));
    }

    #[test]
    fn trims_non_ascii_whitespace() {
        assert_eq!(
            normalize("\u{00A0}ESVUFR\u{00A0}").unwrap(),
            normalize("ESVUFR").unwrap()
        );
    }

    // ---- validation (`validate` and friends) ----

    #[test]
    fn accepts_known_valid_cfis() {
        for s in [
            *b"ESVUFR", // equity / common share, voting, free, fully paid, registered
            *b"ESVTOB", // equity / common share, another valid attribute combination
            *b"DBFTFB", // debt / bond
            *b"MCATXB", // miscellaneous-form / currencies
            *b"OCASNS", // listed option / call
        ] {
            assert!(validate(s).is_ok());
        }
    }

    #[test]
    fn rejects_unknown_category() {
        assert_eq!(
            validate(*b"QSVUFR"),
            Err(CfiError::UnknownCategory { code: 'Q' })
        );
    }

    #[test]
    fn rejects_unknown_group() {
        assert_eq!(
            validate(*b"EZVUFR"),
            Err(CfiError::UnknownGroup {
                category: 'E',
                code: 'Z',
            })
        );
    }

    #[test]
    fn rejects_invalid_attribute() {
        assert_eq!(
            validate(*b"ESXUFR"),
            Err(CfiError::InvalidAttribute {
                category: 'E',
                group: 'S',
                index: 1,
                code: 'X',
            })
        );
    }

    #[test]
    fn rejects_lowercase_as_character_class() {
        assert_eq!(
            validate(*b"esvufr"),
            Err(CfiError::InvalidCharacter {
                character: 'e',
                position: 1,
            })
        );
    }

    #[test]
    fn rejects_digit_as_character_class() {
        assert_eq!(
            validate(*b"ESVUF1"),
            Err(CfiError::InvalidCharacter {
                character: '1',
                position: 6,
            })
        );
    }

    // ---- constructors (`parse`, `from_bytes`) ----

    #[test]
    fn from_bytes_rejects_invalid_attribute_without_normalizing() {
        assert_eq!(
            Cfi::from_bytes(*b"ESZUFR"),
            Err(CfiError::InvalidAttribute {
                category: 'E',
                group: 'S',
                index: 1,
                code: 'Z',
            })
        );
    }

    // ---- accessors ----

    #[test]
    fn accessors_return_expected_segments() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        assert_eq!(cfi.category(), 'E');
        assert_eq!(cfi.group(), 'S');
        assert_eq!(cfi.attributes(), ['V', 'U', 'F', 'R']);
        assert_eq!(cfi.as_bytes(), b"ESVUFR");
        assert_eq!(cfi.as_str(), "ESVUFR");
    }

    // ---- trait impls: FromStr / TryFrom ----

    #[test]
    fn from_str_parses_via_the_parse_method() {
        let cfi: Cfi = "esvufr".parse().unwrap();
        assert_eq!(cfi.as_str(), "ESVUFR");
    }

    #[test]
    fn from_str_propagates_parse_errors() {
        let err = "ESVU".parse::<Cfi>();
        assert_eq!(err, Err(CfiError::InvalidLength { found: 4 }));
    }

    #[test]
    fn try_from_str_delegates_to_parse() {
        assert_eq!(
            Cfi::try_from("esvufr").unwrap(),
            Cfi::parse("ESVUFR").unwrap()
        );
    }

    #[test]
    fn try_from_array_delegates_to_from_bytes() {
        assert_eq!(
            Cfi::try_from(*b"ESVUFR").unwrap(),
            Cfi::from_bytes(*b"ESVUFR").unwrap()
        );
    }

    #[test]
    fn try_from_slice_accepts_correct_length() {
        let cfi = Cfi::try_from(b"ESVUFR".as_slice()).unwrap();
        assert_eq!(cfi.as_str(), "ESVUFR");
    }

    #[test]
    fn try_from_slice_rejects_wrong_length() {
        let err = Cfi::try_from(b"ESVU".as_slice());
        assert_eq!(err, Err(CfiError::InvalidLength { found: 4 }));
    }

    // ---- equality against `str` / `&str` ----

    #[test]
    fn equality_is_symmetric_with_str_slices() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        let other = "ESVUFR";
        assert_eq!(cfi, other);
        assert_eq!(other, cfi);
    }

    // ---- AsRef ----

    #[test]
    fn as_ref_impls_match_accessors() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        let bytes_ref: &[u8] = cfi.as_ref();
        assert_eq!(bytes_ref, cfi.as_bytes());
        let str_ref: &str = cfi.as_ref();
        assert_eq!(str_ref, cfi.as_str());
    }

    #[test]
    fn ordering_follows_byte_order() {
        let d = Cfi::parse("DBFTFB").unwrap();
        let e = Cfi::parse("ESVUFR").unwrap();
        assert!(d < e);
    }

    #[test]
    fn hash_supports_set_membership() {
        let mut set = HashSet::new();
        set.insert(Cfi::parse("ESVUFR").unwrap());
        set.insert(Cfi::parse("DBFTFB").unwrap());
        set.insert(Cfi::parse("ESVUFR").unwrap());
        assert_eq!(set.len(), 2);
    }

    // ---- Display / Debug ----

    #[test]
    fn display_is_the_canonical_string() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        assert_eq!(cfi.to_string(), "ESVUFR");
        assert_eq!(cfi.to_string(), "ESVUFR");
    }

    #[test]
    fn debug_is_readable() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        assert_eq!(format!("{cfi:?}"), "Cfi(\"ESVUFR\")");
    }

    // ---- CfiError messages ----

    #[test]
    fn error_messages_are_human_readable() {
        assert_eq!(CfiError::Empty.to_string(), "empty CFI");
        assert_eq!(
            CfiError::InvalidLength { found: 4 }.to_string(),
            "CFI must contain exactly 6 characters, found 4"
        );
        assert_eq!(
            CfiError::InvalidCharacter {
                character: '1',
                position: 6
            }
            .to_string(),
            "invalid character '1' at position 6 of 6: expected an uppercase letter (A-Z) ASCII character"
        );
        assert_eq!(
            CfiError::UnknownCategory { code: 'Q' }.to_string(),
            "unknown CFI category 'Q' at position 1"
        );
        assert_eq!(
            CfiError::UnknownGroup {
                category: 'E',
                code: 'Z'
            }
            .to_string(),
            "unknown CFI group 'Z' at position 2"
        );
        assert_eq!(
            CfiError::InvalidAttribute {
                category: 'E',
                group: 'S',
                index: 1,
                code: 'X'
            }
            .to_string(),
            "invalid attribute 'X' at position 1 of 6"
        );
    }
}
