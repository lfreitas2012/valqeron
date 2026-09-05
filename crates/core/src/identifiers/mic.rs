//! ISO 10383 market identifier codes (MIC): the four-character codes that identify exchanges,
//! trading platforms, and other organized markets.
//!
//! This module provides the validated Rust representation ([`Mic`]) together with the parsing,
//! validation, and error types that surround it. It accepts the canonical four-character form
//! (optionally surrounded by whitespace, in any ASCII case), normalizes it, and guarantees that
//! any constructed [`Mic`] is a code registered in ISO 10383. There is no partially validated
//! state. If you hold a [`Mic`], it is registered.
//!
//! # What this type represents
//!
//! A MIC is four uppercase ASCII letters or digits, for example, `XNYS`, `XLON`, or `360T`. The
//! registry distinguishes two kinds of code: an *operating MIC* identifies the entity operating a
//! market (for example, `XNYS`, the New York Stock Exchange), and a *segment MIC* identifies a
//! specific market segment run by that operator (for example, `ARCX`, NYSE Arca, operated under
//! `XNYS`). A few special purpose codes (`XOFF`, `XXXX`, `BILT`) describe off-exchange or
//! unlisted activity rather than a real market.
//!
//! [`Mic`] stores the four characters as normalized uppercase ASCII and exposes borrowed accessors
//! for the raw bytes ([`Mic::as_bytes`]) and for the whole value ([`Mic::as_str`]). The registered
//! facts about the code are available through lookup accessors: [`Mic::is_active`],
//! [`Mic::is_operating`], [`Mic::is_segment`], [`Mic::operating_mic`], and [`Mic::country_code`].
//!
//! # Validation rules
//!
//! A MIC has no check digit. It is valid exactly when it is a code registered in ISO 10383. This
//! crate embeds the registry as a sorted table generated from `data/mic.csv`. Every fallible
//! constructor runs the same rules, in order, and each maps to one [`MicError`] variant:
//!
//! 1. Length: after the surrounding whitespace is trimmed, the input must contain exactly four
//!    characters ([`MicError::InvalidLength`]). [`Mic::parse`] rejects empty input first
//!    ([`MicError::Empty`]).
//! 2. Character class: every position must be an uppercase ASCII letter or a decimal digit
//!    ([`MicError::InvalidCharacter`]).
//! 3. Membership: the four characters together must name a registered code
//!    ([`MicError::Unregistered`]).
//!
//! Registration, not lifecycle state, decides validity: codes the registry lists as expired are
//! accepted because they identify markets that existed and still appear in historical reference
//! data. Use [`Mic::is_active`] to enforce "currently active" as a policy where that matters.
//!
//! # The registry is a snapshot
//!
//! ISO 10383 is a living registry: new codes are added and existing codes expire with roughly
//! monthly publications. The embedded table is the publication committed at `data/mic.csv`, so
//! membership and the registered facts are as of that snapshot. Refresh it with `just mic-update`
//! (which fetches the latest official CSV and regenerates the table). A code registered after the
//! committed snapshot is rejected as [`MicError::Unregistered`] until the snapshot is refreshed.
//! Expired codes are never removed from the registry and never reused, so refreshing the snapshot
//! only ever grows the accepted set.
//!
//! # Design notes
//!
//! * No invalid state is representable. The only field of [`Mic`] is private. Every way to obtain
//!   one ([`Mic::parse`], [`Mic::new`], [`Mic::from_bytes`], [`FromStr`], and [`TryFrom<&str>`])
//!   runs full validation. There is no unchecked constructor.
//! * It is zero allocation and `Copy`. [`Mic`] is a four-byte value that wraps `[u8; 4]`. Parsing,
//!   validation, and every accessor operate on the stack. The membership test and the lookup
//!   accessors are a binary search over the embedded table.
//! * Ordering and hashing operate over the raw ASCII bytes. This matches [`str`] ordering on
//!   [`Mic::as_str`], which is lexicographic and carries no market meaning.
//! * It is safe to use as a map or set key. [`Mic`] implements [`Eq`] and [`Hash`] consistently
//!   with [`PartialEq`], so it works as a `HashMap` or `HashSet` key, and as a `BTreeMap` or
//!   `BTreeSet` key, out of the box.
//!
//! # Feature flags
//!
//! The optional integrations are off by default and purely additive. Enabling one never changes
//! the behavior of [`Mic::parse`] or the validation rules above:
//!
//! * `serde`: (de)serializes [`Mic`] as its four-character string, for example, `"XNYS"`.
//!   Deserialization re-runs full validation, so an untrusted payload can never produce an invalid
//!   [`Mic`].
//! * `schemars`: implements `JsonSchema` for [`Mic`], describing it as a pattern-constrained
//!   string (`^[A-Z0-9]{4}$`). The pattern is structural. It cannot express which codes are
//!   registered, so validity is enforced on deserialization. Implies `serde`.
//! * `arbitrary`: implements `Arbitrary` for [`Mic`], generating registered codes for fuzz
//!   targets.
//! * `proptest`: exposes reusable `proptest` strategies (`valqeron_identifiers::mic::proptest`,
//!   when this feature is enabled) for generating valid [`Mic`] values.
//!
//! # Error handling
//!
//! Every fallible constructor returns [`MicError`], which is `Clone + PartialEq + Eq` and
//! implements [`core::error::Error`] and [`core::fmt::Display`], so it composes with `?` and with
//! error aggregation crates alike:
//!
//! ```
//! use valqeron_identifiers::{Mic, MicError};
//!
//! match Mic::parse("ZZZZ") {
//!     Ok(mic) => println!("valid: {mic}"),
//!     Err(MicError::Unregistered { code }) => {
//!         println!("not registered: {}{}{}{}", code[0], code[1], code[2], code[3]);
//!     }
//!     Err(other) => println!("rejected: {other}"),
//! }
//! ```
//!
//! # Examples
//!
//! ```
//! use valqeron_identifiers::Mic;
//!
//! let mic = Mic::parse("xnys").unwrap(); // lowercase is folded automatically
//! assert_eq!(mic.as_str(), "XNYS");
//! assert!(mic.is_active());
//! assert!(mic.is_operating());
//! ```
//!
//! Walking from a segment to the market that operates it, and to the country it trades in:
//!
//! ```
//! use valqeron_identifiers::Mic;
//!
//! let arca = Mic::parse("ARCX").unwrap(); // NYSE Arca
//! assert!(arca.is_segment());
//! assert_eq!(arca.operating_mic().as_str(), "XNYS");
//! assert_eq!(arca.country_code().map(|c| *c.as_bytes()), Some(*b"US"));
//! ```

use crate::CountryCode;
use arbitrary::{Arbitrary, Unstructured};
use proptest::prelude::Strategy;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::borrow::Cow;
use std::fmt;
use std::str::{FromStr, from_utf8_unchecked};
use valqeron_macros::generate_mic_table;

// Procedural Macros for MIC Table Generation
struct MicEntry {
    pub code: [u8; 4],
    pub operating: u16,
    pub country: [u8; 2],
    pub active: bool,
}

generate_mic_table!("data/mic.csv");

/// A validated ISO 10383 market identifier code.
///
/// `Mic` is a four-byte, `Copy`, allocation-free value object. Once constructed, it is guaranteed
/// to be a code registered in ISO 10383 (active or expired). There is no way to get a `Mic` that
/// has not passed validation.
///
/// Internally, the code is stored as four raw uppercase ASCII letters or digits (`'A'...='Z'`,
/// `'0'...='9'`).
///
/// # Constructing a `Mic`
///
/// * [`Mic::parse`] and [`Mic::new`] accept four character strings, in any ASCII case, trimmed of
///   surrounding whitespace.
/// * [`Mic::from_bytes`] accepts exactly four pre-normalized uppercase ASCII bytes.
/// * [`FromStr`] and [`TryFrom<&str>`] behave like `parse`, for use in generic code.
///
/// All of them run the same validation and return [`MicError`] on failure. See the
/// [module level documentation](self) for the validation rules and design rationale.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed Mic should be used; discarding it wastes the validation work"]
pub struct Mic {
    bytes: [u8; 4],
}

impl Mic {
    /// Parses a market identifier code from a string.
    ///
    /// The parser trims surrounding whitespace and folds ASCII letters to uppercase before
    /// validation. This is the primary constructor. [`Mic::new`], [`FromStr`], and
    /// [`TryFrom<&str>`] all delegate to it.
    ///
    /// # Errors
    ///
    /// Returns [`MicError`] if the input is empty, does not contain exactly four characters after
    /// trimming, contains a character other than a letter or a digit, or names a code that is not
    /// registered in ISO 10383.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Mic;
    ///
    /// assert!(Mic::parse("XNYS").is_ok());
    /// assert!(Mic::parse("xnys").is_ok()); // lowercase is folded automatically
    /// assert!(Mic::parse(" 360T ").is_ok()); // surrounding whitespace is trimmed
    /// assert!(Mic::parse("ALDP").is_ok()); // expired codes stay registered
    /// assert!(Mic::parse("ZZZZ").is_err()); // well formed but not registered
    /// ```
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

/// Runs every validation rule against a normalized candidate, cheapest first:
///
/// 1. Character class: every position must be an uppercase ASCII letter or a decimal digit.
/// 2. Membership: the four characters together must name a code registered in ISO 10383.
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
    MIC_ENTRIES
        .binary_search_by_key(candidate, |entry| entry.code)
        .ok()
        .map(|index| &MIC_ENTRIES[index])
}

/// Normalizes `input` into a four-byte ASCII array.
///
/// The steps are:
///
/// * Empty input is rejected as [`MicError::Empty`].
/// * Leading and trailing whitespace is trimmed. Interior characters are left untouched.
/// * Remaining characters are ASCII uppercased, so a lowercase code is accepted transparently.
/// * Any non-ASCII character, or a character count other than four after trimming, is rejected.
///
/// This function does not check the character class of each position, nor whether the four
/// characters name a registered code. See [`super::validation::validate`] for that.
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

impl Serialize for Mic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct MicVisitor;

impl<'de> Visitor<'de> for MicVisitor {
    type Value = Mic;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 4-character ISO 10383 market identifier code, e.g. XNYS")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Mic::parse(v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Mic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(MicVisitor)
    }
}

impl<'a> Arbitrary<'a> for Mic {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Pick straight from the registry so every generated value is valid by construction.
        let entry = u.choose(MIC_ENTRIES)?;
        Mic::from_bytes(entry.code).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

/// A strategy producing valid [`Mic`] values by picking from the registered set.
pub fn valid_mic() -> impl Strategy<Value = Mic> {
    (0..MIC_ENTRIES.len()).prop_map(|index| {
        Mic::from_bytes(MIC_ENTRIES[index].code)
            .expect("codes in the registry table are valid by construction")
    })
}

/// A strategy producing a valid [`Mic`] rendered as its canonical four-character `String`, useful
/// for a round trip through parsing property tests.
pub fn valid_mic_string() -> impl Strategy<Value = String> {
    valid_mic().prop_map(|mic| mic.as_str().to_string())
}

impl JsonSchema for Mic {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Mic")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "iso10383-mic",
            "minLength": 4,
            "maxLength": 4,
            "pattern": "^[A-Z0-9]{4}$",
            "description": "ISO 10383 market identifier code. \
            The pattern is structural; membership in the registry is enforced on deserialization."
        })
    }
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
mod tests_arbitrary {
    use super::*;

    #[test]
    fn always_produces_registered_codes() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let mic = Mic::arbitrary(&mut u).expect("arbitrary should always succeed");
            // Re-validating via parse() proves the value round trips through the exact same checks
            // a hand-typed input would.
            assert!(Mic::parse(mic.as_str()).is_ok());
        }
    }
}

#[cfg(test)]
mod tests_proptest {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_mic_always_round_trips_through_parse(mic in valid_mic()) {
            let reparsed = Mic::parse(mic.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(mic, reparsed.unwrap());
        }

        #[test]
        fn valid_mic_string_always_parses(s in valid_mic_string()) {
            prop_assert!(Mic::parse(&s).is_ok());
        }

        #[test]
        fn operating_references_stay_inside_the_registry(mic in valid_mic()) {
            // The published operating reference of any registered code is itself registered, and
            // an operating MIC is exactly a code that references itself.
            let operating = mic.operating_mic();
            prop_assert!(Mic::parse(operating.as_str()).is_ok());
            prop_assert_eq!(mic.is_operating(), operating == mic);
        }
    }
}

#[cfg(test)]
mod tests_schemars {
    use super::super::Mic;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(Mic);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "iso10383-mic");
        assert_eq!(json["minLength"], 4);
        assert_eq!(json["maxLength"], 4);
        assert_eq!(json["pattern"], "^[A-Z0-9]{4}$");
    }
}

#[cfg(test)]
mod tests_serde {
    use super::super::Mic;

    #[test]
    fn round_trips_through_json() {
        let mic = Mic::parse("XNYS").unwrap();
        let json = serde_json::to_string(&mic).unwrap();
        assert_eq!(json, "\"XNYS\"");
        let back: Mic = serde_json::from_str(&json).unwrap();
        assert_eq!(mic, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<Mic>("\"ZZZZ\"").unwrap_err();
        assert!(err.to_string().contains("ISO 10383"));
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
        let operating = &MIC_ENTRIES[usize::from(entry.operating)];
        assert_eq!(operating.code, *b"XNYS");
        assert_eq!(usize::from(operating.operating), {
            MIC_ENTRIES
                .binary_search_by_key(b"XNYS", |e| e.code)
                .unwrap()
        });
    }
}
