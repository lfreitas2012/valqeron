use crate::identifiers::country_code::CountryCode;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::{FromStr, from_utf8_unchecked};
use std::string::FromUtf8Error;
use thiserror::Error;
use valqeron_macros::generate_mic_table;

#[derive(Debug)]
struct MicEntry {
    pub code: [u8; 4],
    pub operating: u16,
    pub country: [u8; 2],
    pub active: bool,
}

generate_mic_table!("data/mic.csv");

#[derive(Clone, Copy)]
#[must_use = "a parsed Mic should be used; discarding it wastes the validation work"]
pub struct Mic {
    bytes: [u8; 4],
    entry: &'static MicEntry,
}

impl Mic {
    /// Parses a MIC from user-facing input: trims whitespace and uppercases letters before
    /// validating against the ISO 10383 registry.
    ///
    /// # Errors
    ///
    /// Returns [`MicError`] if the input is empty, is not exactly four characters long, contains a
    /// non-ASCII-alphanumeric character, or is well-formed but not a registered MIC.
    pub fn parse(input: &str) -> Result<Self, MicError> {
        let candidate = normalize(input)?;
        Self::from_bytes(candidate)
    }

    /// Alias for [`Mic::parse`].
    ///
    /// # Errors
    ///
    /// See [`Mic::parse`].
    #[inline]
    pub fn new(input: &str) -> Result<Self, MicError> {
        Self::parse(input)
    }

    /// Builds a `Mic` from four raw bytes, validating them against the
    /// ISO 10383 registry.
    ///
    /// # Errors
    ///
    /// Returns [`MicError`] if the bytes are not uppercase ASCII
    /// letters/digits, or are well-formed but not a registered MIC.
    pub fn from_bytes(bytes: [u8; 4]) -> Result<Self, MicError> {
        let entry = validate(bytes)?;
        Ok(Mic { bytes, entry })
    }

    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 4] {
        &self.bytes
    }

    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `Mic::from_bytes` guarantees every byte is an uppercase ASCII letter or digit.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.entry.active
    }

    #[must_use]
    pub fn is_operating(&self) -> bool {
        self.operating_mic() == *self
    }

    #[must_use]
    pub fn is_segment(&self) -> bool {
        !self.is_operating()
    }

    pub fn operating_mic(&self) -> Mic {
        let operating_entry = MIC_ENTRIES
            .get(usize::from(self.entry.operating))
            .unwrap_or(self.entry);
        Mic {
            bytes: operating_entry.code,
            entry: operating_entry,
        }
    }

    #[must_use]
    pub fn country_code(&self) -> Option<CountryCode> {
        CountryCode::from_bytes(self.entry.country).ok()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MicError {
    #[error("MIC cannot be empty")]
    Empty,

    #[error("MIC must be exactly four characters long; found {found}")]
    InvalidLength { found: usize },

    #[error(
        "MIC must only contain uppercase ASCII letters and decimal digits; found {character} at position {position}"
    )]
    InvalidCharacter { character: char, position: u8 },

    #[error("MIC {code} is not registered in ISO 10383")]
    Unregistered { code: String },

    #[error(transparent)]
    InvalidUtf8(#[from] FromUtf8Error),
}

impl FromStr for Mic {
    type Err = MicError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Mic {
    type Error = MicError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 4]> for Mic {
    type Error = MicError;

    fn try_from(value: [u8; 4]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for Mic {
    type Error = MicError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 4] = value
            .try_into()
            .map_err(|_| MicError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
    }
}

impl PartialEq for Mic {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for Mic {}

impl PartialOrd for Mic {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Mic {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

impl Hash for Mic {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
    }
}

impl PartialEq<str> for Mic {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Mic {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Mic> for str {
    fn eq(&self, other: &Mic) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Mic> for &str {
    fn eq(&self, other: &Mic) -> bool {
        *self == other.as_str()
    }
}

impl AsRef<[u8]> for Mic {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for Mic {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Mic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Mic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Mic").field(&self.as_str()).finish()
    }
}

impl Serialize for Mic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Mic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        Mic::parse(s).map_err(serde::de::Error::custom)
    }
}

fn validate(candidate: [u8; 4]) -> Result<&'static MicEntry, MicError> {
    validate_character_classes(candidate)?;
    validate_membership(candidate)
}

fn validate_character_classes(candidate: [u8; 4]) -> Result<(), MicError> {
    for (&byte, position) in candidate.iter().zip(1u8..) {
        if !byte.is_ascii_uppercase() && !byte.is_ascii_digit() {
            return Err(MicError::InvalidCharacter {
                character: char::from(byte),
                position,
            });
        }
    }
    Ok(())
}

fn validate_membership(candidate: [u8; 4]) -> Result<&'static MicEntry, MicError> {
    if let Some(entry) = find(candidate) {
        Ok(entry)
    } else {
        let code = String::from_utf8(candidate.to_vec())?;
        Err(MicError::Unregistered { code })
    }
}

#[inline]
fn find(candidate: [u8; 4]) -> Option<&'static MicEntry> {
    MIC_ENTRIES
        .binary_search_by_key(&candidate, |entry| entry.code)
        .ok()
        .and_then(|index| MIC_ENTRIES.get(index))
}

fn normalize(input: &str) -> Result<[u8; 4], MicError> {
    if input.is_empty() {
        return Err(MicError::Empty);
    }

    let trimmed = input.trim();
    let found = trimmed.chars().count();
    if found != 4 {
        return Err(MicError::InvalidLength { found });
    }

    let mut buf = [0u8; 4];
    for ((slot, ch), position) in buf.iter_mut().zip(trimmed.chars()).zip(1u8..) {
        if !ch.is_ascii() {
            return Err(MicError::InvalidCharacter {
                character: ch,
                position,
            });
        }
        let upper = ch.to_ascii_uppercase();
        *slot = u8::try_from(upper).map_err(|_| MicError::InvalidCharacter {
            character: ch,
            position,
        })?;
    }

    Ok(buf)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn candidate(s: &str) -> [u8; 4] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 4];
        out.copy_from_slice(bytes);
        out
    }

    mod parsing {
        use super::*;

        #[test]
        fn rejects_empty() {
            assert_eq!(normalize(""), Err(MicError::Empty));
        }

        #[test]
        fn trims_surrounding_whitespace() {
            assert_eq!(normalize("  XNYS "), normalize("XNYS"));
        }

        #[test]
        fn uppercases_letters() {
            assert_eq!(normalize("xnys").unwrap(), *b"XNYS");
        }

        #[test]
        fn keeps_digits() {
            assert_eq!(normalize("360t").unwrap(), *b"360T");
        }

        #[test]
        fn rejects_wrong_length() {
            assert_eq!(normalize("XNY"), Err(MicError::InvalidLength { found: 3 }));
            assert_eq!(
                normalize("XNYSE"),
                Err(MicError::InvalidLength { found: 5 })
            );
        }

        #[test]
        fn whitespace_only_is_a_length_error() {
            assert_eq!(normalize("   "), Err(MicError::InvalidLength { found: 0 }));
        }

        #[test]
        fn keeps_non_alphanumeric_characters_for_validation() {
            // A separator not surrounding whitespace survives normalization (the count is
            // still four) and is left for validation to reject as an invalid character.
            assert_eq!(normalize("XN.S").unwrap(), *b"XN.S");
        }

        #[test]
        fn rejects_non_ascii() {
            let err = normalize("XNY£").unwrap_err();
            assert!(matches!(
                err,
                MicError::InvalidCharacter {
                    character: '£', ..
                }
            ));
        }

        #[test]
        fn parse_and_from_str_agree() {
            let via_parse = Mic::parse("XNYS").unwrap();
            let via_from_str: Mic = "XNYS".parse().unwrap();
            assert_eq!(via_parse, via_from_str);
        }

        #[test]
        fn new_is_an_alias_for_parse() {
            assert_eq!(Mic::new("XNYS"), Mic::parse("XNYS"));
        }
    }

    mod validation {
        use super::*;

        #[test]
        fn accepts_known_registered_codes() {
            for s in ["XNYS", "XLON", "BVMF", "360T", "XOFF", "XXXX", "ALDP"] {
                assert!(validate(candidate(s)).is_ok(), "{s} should be valid");
            }
        }

        #[test]
        fn accepts_expired_codes() {
            // `ALDP` is expired but registered; membership is about the registry,
            // not the lifecycle state.
            let entry = find(candidate("ALDP")).unwrap();
            assert!(!entry.active);
            assert!(validate(candidate("ALDP")).is_ok());
        }

        #[test]
        fn rejects_unregistered_but_well_formed() {
            let err = validate(candidate("ZZZZ")).unwrap_err();
            assert_eq!(
                err,
                MicError::Unregistered {
                    code: String::from("ZZZZ"),
                }
            );
        }

        #[test]
        fn rejects_lowercase_as_character_class() {
            let err = validate(candidate("xnys")).unwrap_err();
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
            let err = validate(candidate("XN.S")).unwrap_err();
            assert_eq!(
                err,
                MicError::InvalidCharacter {
                    character: '.',
                    position: 3,
                }
            );
        }

        #[test]
        fn try_from_slice_rejects_wrong_length() {
            let err = Mic::try_from(&b"XNY"[..]).unwrap_err();
            assert_eq!(err, MicError::InvalidLength { found: 3 });
        }

        #[test]
        fn try_from_slice_accepts_four_bytes() {
            let mic = Mic::try_from(&b"XNYS"[..]).unwrap();
            assert_eq!(mic, "XNYS");
        }

        #[test]
        fn try_from_array_matches_from_bytes() {
            assert_eq!(Mic::try_from(*b"XNYS"), Mic::from_bytes(*b"XNYS"));
        }
    }

    mod registry_lookups {
        use super::*;

        #[test]
        fn find_resolves_operating_references() {
            // `ARCX` is a segment of `XNYS`; its operating index must name that entry.
            let entry = find(candidate("ARCX")).unwrap();
            let operating = &MIC_ENTRIES[usize::from(entry.operating)];
            assert_eq!(operating.code, *b"XNYS");
        }

        #[test]
        fn operating_market_is_its_own_operating_mic() {
            let mic = Mic::parse("XNYS").unwrap();
            assert!(mic.is_operating());
            assert!(!mic.is_segment());
            assert_eq!(mic.operating_mic(), mic);
        }

        #[test]
        fn segment_market_resolves_to_its_operator() {
            let segment = Mic::parse("ARCX").unwrap();
            assert!(segment.is_segment());
            assert!(!segment.is_operating());
            assert_eq!(segment.operating_mic(), Mic::parse("XNYS").unwrap());
        }

        #[test]
        fn active_flag_reflects_the_registry() {
            assert!(Mic::parse("XNYS").unwrap().is_active());
            assert!(!Mic::parse("ALDP").unwrap().is_active());
        }

        #[test]
        fn country_code_is_none_for_the_zz_placeholder() {
            assert!(Mic::parse("XOFF").unwrap().country_code().is_none());
        }

        #[test]
        fn country_code_is_some_for_an_assigned_country() {
            assert!(Mic::parse("XNYS").unwrap().country_code().is_some());
        }
    }

    mod formatting {
        use super::*;

        #[test]
        fn display_is_the_canonical_string() {
            let mic = Mic::parse("XNYS").unwrap();
            assert_eq!(mic.to_string(), "XNYS");
        }

        #[test]
        fn debug_is_readable() {
            let mic = Mic::parse("XNYS").unwrap();
            assert_eq!(format!("{mic:?}"), "Mic(\"XNYS\")");
        }

        #[test]
        fn error_messages_are_human_readable() {
            assert_eq!(MicError::Empty.to_string(), "MIC cannot be empty");
            assert_eq!(
                MicError::InvalidLength { found: 3 }.to_string(),
                "MIC must be exactly four characters long; found 3"
            );
        }
    }

    mod comparisons {
        use super::*;

        #[test]
        fn compares_equal_to_matching_str() {
            let mic = Mic::parse("XNYS").unwrap();
            assert_eq!(mic, "XNYS");
            assert_eq!("XNYS", mic);
            assert_ne!(mic, "XNAS");
        }

        #[test]
        fn orders_the_same_as_the_underlying_bytes() {
            let arcx = Mic::parse("ARCX").unwrap();
            let xnys = Mic::parse("XNYS").unwrap();
            assert!(arcx < xnys);
        }

        #[test]
        fn hash_is_consistent_with_equality() {
            let a = Mic::parse("XNYS").unwrap();
            let b = Mic::parse("xnys").unwrap(); // normalizes to the same bytes as `a`
            let mut set = HashSet::new();
            set.insert(a);
            assert!(!set.insert(b), "equal Mics must hash to the same bucket");
        }

        #[test]
        fn as_ref_bytes_matches_as_bytes() {
            let mic = Mic::parse("XNYS").unwrap();
            assert_eq!(AsRef::<[u8]>::as_ref(&mic), mic.as_bytes());
        }
    }
}
