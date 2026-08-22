use crate::identifiers::CountryCode;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::{FromStr, from_utf8_unchecked};
use std::string::FromUtf8Error;
use thiserror::Error;
use valqeron_macros::generate_mic_table;

struct MicEntry {
    pub code: [u8; 4],
    pub operating: u16,
    pub country: [u8; 2],
    pub active: bool,
}

generate_mic_table!("data/mic.csv");

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed Mic should be used; discarding it wastes the validation work"]
pub struct Mic {
    bytes: [u8; 4],
}

impl Mic {
    pub fn parse(input: &str) -> Result<Self, MicError> {
        let candidate = normalize(input)?;
        Self::from_bytes(candidate)
    }

    /// Alias for [`Mic::parse`].
    ///
    /// # Errors
    ///
    /// See [`Mic::parse`].
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Mic;
    ///
    /// assert_eq!(Mic::new("XNYS"), Mic::parse("XNYS"));
    /// ```
    #[inline]
    pub fn new(input: &str) -> Result<Self, MicError> {
        Self::parse(input)
    }

    /// Constructs a `Mic` directly from four raw ASCII bytes.
    ///
    /// Each byte must already be an uppercase letter or a digit. Use [`Mic::parse`] if the input
    /// might contain surrounding whitespace or lowercase letters.
    ///
    /// # Errors
    ///
    /// Returns [`MicError`] under the same conditions as [`Mic::parse`], except that length is
    /// guaranteed by the `[u8; 4]` type itself: [`MicError::InvalidLength`] cannot occur here.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Mic;
    ///
    /// let mic = Mic::from_bytes(*b"XNYS").unwrap();
    /// assert_eq!(mic.as_str(), "XNYS");
    ///
    /// // A well formed but unregistered code is rejected just like it would be through `parse`.
    /// assert!(Mic::from_bytes(*b"ZZZZ").is_err());
    /// ```
    pub fn from_bytes(bytes: [u8; 4]) -> Result<Self, MicError> {
        validate(&bytes)?;
        Ok(Mic { bytes })
    }

    /// Returns the four raw ASCII bytes backing this code (for example, `b"XNYS"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Mic;
    ///
    /// let mic = Mic::parse("XNYS").unwrap();
    /// assert_eq!(mic.as_bytes(), b"XNYS");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 4] {
        &self.bytes
    }

    /// Returns the four-character market identifier code as a `&str`.
    ///
    /// This never allocates. The bytes are guaranteed to be valid ASCII by construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Mic;
    ///
    /// let mic = Mic::parse("XNYS").unwrap();
    /// assert_eq!(mic.as_str(), "XNYS");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `Mic::from_bytes` guarantees every byte is an uppercase ASCII letter or digit.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }

    /// Returns `true` when the registry snapshot lists this code as active, `false` when it has
    /// expired.
    ///
    /// Expired codes still parse because they identify markets that existed and remain registered
    /// forever. Whether an expired code is acceptable is a policy decision for the caller, and
    /// this accessor is the hook for it.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Mic;
    ///
    /// assert!(Mic::parse("XNYS").unwrap().is_active());
    /// assert!(!Mic::parse("ALDP").unwrap().is_active()); // NYSE Alternext Dark, expired
    /// ```
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.entry().active
    }

    /// Returns `true` when this code is an operating MIC: the code of the entity operating a
    /// market, rather than one of its segments.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Mic;
    ///
    /// assert!(Mic::parse("XNYS").unwrap().is_operating());
    /// assert!(!Mic::parse("ARCX").unwrap().is_operating()); // NYSE Arca is a segment
    /// ```
    #[must_use]
    pub fn is_operating(&self) -> bool {
        self.operating_mic() == *self
    }

    /// Returns `true` when this code is a segment MIC: a specific market segment that belongs to
    /// an operating MIC.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Mic;
    ///
    /// assert!(Mic::parse("ARCX").unwrap().is_segment());
    /// assert!(!Mic::parse("XNYS").unwrap().is_segment());
    /// ```
    #[must_use]
    pub fn is_segment(&self) -> bool {
        !self.is_operating()
    }

    /// Returns the operating MIC this code belongs to, exactly as the registry publishes it. An
    /// operating MIC returns itself.
    ///
    /// The registry keeps the operating MIC a segment had at the time on expired rows, so for a
    /// handful of expired segments the returned code names a market that was later re-parented and
    /// is itself a segment today. References always resolve, so repeated calls reach an operating
    /// MIC in a bounded number of steps, but a single call is not guaranteed to return one when
    /// `self` has expired.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Mic;
    ///
    /// let arca = Mic::parse("ARCX").unwrap();
    /// assert_eq!(arca.operating_mic().as_str(), "XNYS");
    ///
    /// let nyse = Mic::parse("XNYS").unwrap();
    /// assert_eq!(nyse.operating_mic(), nyse);
    /// ```
    pub fn operating_mic(&self) -> Mic {
        let entry = self.entry();
        Mic {
            bytes: MIC_ENTRIES[usize::from(entry.operating)].code,
        }
    }

    /// Returns the ISO 3166-1 country the registry lists for this market, or `None` for the
    /// off-exchange pseudo-MICs (`XOFF`, `XXXX`, `BILT`), which the registry files under the `ZZ`
    /// placeholder instead of a country.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Mic;
    ///
    /// let nyse = Mic::parse("XNYS").unwrap();
    /// assert_eq!(nyse.country_code().map(|c| *c.as_bytes()), Some(*b"US"));
    ///
    /// let off_exchange = Mic::parse("XOFF").unwrap();
    /// assert_eq!(off_exchange.country_code(), None);
    /// ```
    #[must_use]
    pub fn country_code(&self) -> Option<CountryCode> {
        // The generator proves every non-`ZZ` country in the table is an assigned ISO 3166-1
        // code, so this only yields `None` for the `ZZ` placeholder.
        CountryCode::from_bytes(self.entry().country).ok()
    }

    /// Looks up the registry entry backing this code.
    fn entry(self) -> &'static MicEntry {
        find(&self.bytes)
            .expect("a constructed Mic always names an entry in the embedded registry table")
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

    /// Delegates to [`Mic::parse`], enabling `input.parse::<Mic>()` and use in generic code
    /// bounded by [`FromStr`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Mic {
    type Error = MicError;

    /// Delegates to [`Mic::parse`], enabling `Mic::try_from(input)` and use in generic code
    /// bounded by [`TryFrom<&str>`].
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 4]> for Mic {
    type Error = MicError;

    /// Delegates to [`Mic::from_bytes`]. The four bytes must already be pre normalized uppercase
    /// ASCII letters or digits.
    fn try_from(value: [u8; 4]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for Mic {
    type Error = MicError;

    /// Validates a byte slice as a market identifier code. The slice must be exactly four pre
    /// normalized uppercase ASCII bytes; any other length yields [`MicError::InvalidLength`]. Once
    /// the length is confirmed, this behaves like [`Mic::from_bytes`].
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 4] = value
            .try_into()
            .map_err(|_| MicError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
    }
}

impl PartialEq<str> for Mic {
    /// Compares against a string slice by its canonical four character representation.
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Mic {
    /// Compares against a string slice by its canonical four character representation.
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
    /// Equivalent to [`Mic::as_bytes`], borrowed as a slice.
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for Mic {
    /// Equivalent to [`Mic::as_str`].
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

fn validate(candidate: &[u8; 4]) -> Result<(), MicError> {
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
        let code = String::from_utf8(candidate.to_vec())?;
        Err(MicError::Unregistered { code })
    }
}

#[inline]
fn find(candidate: &[u8; 4]) -> Option<&'static MicEntry> {
    MIC_ENTRIES
        .binary_search_by_key(candidate, |entry| entry.code)
        .ok()
        .map(|index| &MIC_ENTRIES[index])
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
    for (i, ch) in trimmed.chars().enumerate() {
        if !ch.is_ascii() {
            return Err(MicError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
            });
        }
        buf[i] = ch.to_ascii_uppercase() as u8;
    }

    Ok(buf)
}

#[cfg(test)]
mod tests_formating {
    use crate::identifiers::Mic;
    use std::format;
    use std::string::ToString;

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
}

#[cfg(test)]
mod tests_parser {
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
}

#[cfg(test)]
mod tests_validation {
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
                code: String::from("ZZZZ"),
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
        let operating = &MIC_ENTRIES[usize::from(entry.operating)];
        assert_eq!(operating.code, *b"XNYS");
        assert_eq!(usize::from(operating.operating), {
            MIC_ENTRIES
                .binary_search_by_key(b"XNYS", |e| e.code)
                .unwrap()
        });
    }
}
