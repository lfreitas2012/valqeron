//! CFI (Classification of Financial Instruments), the ISO 10962 six-letter code that classifies a
//! financial instrument by category, group, and four attributes.
//!
//! This module provides the validated Rust representation ([`Cfi`]) and the parsing, validation,
//! and error types that surround it. It accepts the canonical 6-character form (optionally
//! surrounded by whitespace, in any ASCII case), normalizes it, and guarantees that any constructed
//! [`Cfi`] describes a combination actually defined by ISO 10962. There is no partially validated
//! state: if you hold a [`Cfi`], it is valid.
//!
//! # What this type represents
//!
//! A CFI has 6 characters, all uppercase letters, split into three parts:
//!
//! | Positions | Length | Segment | Meaning |
//! |-----------|--------|------------|------------------------------------------------------------------|
//! | 1 | 1 | Category | The broadest class of instrument (e.g. `E` = equities) |
//! | 2 | 1 | Group | A subdivision within the category (meaning depends on the category) |
//! | 3–6 | 4 | Attributes | Four attribute codes whose meaning depends on the category and group |
//!
//! ```text
//! ┌────────────────────────────────────────┐
//! │ Cat │ Grp │  Attribute 1..4 (4 chars)  │
//! │  E  │  S  │   V     U     F     R      │
//! └────────────────────────────────────────┘
//! ```
//!
//! [`Cfi`] stores those 6 characters as normalized uppercase ASCII and exposes borrowed/`char`
//! accessors for the category ([`Cfi::category`]), the group ([`Cfi::group`]), the four attributes
//! ([`Cfi::attributes`]), and the whole value ([`Cfi::as_str`]).
//!
//! # Validation rules — taxonomy, not checksum
//!
//! Unlike [`Cnpj`](crate::Cnpj) (Módulo 11) or [`Isin`](crate::Isin) (Luhn), a CFI carries no check
//! digit. Its validity is defined entirely by the ISO 10962 code taxonomy, which this crate embeds
//! as a generated, lookup table. Every fallible constructor runs the same rules, in order,
//! and each maps to one [`CfiError`] variant:
//!
//! 1. **Length**: after the surrounding whitespace is trimmed, the input must contain exactly 6
//!    characters ([`CfiError::InvalidLength`]). [`Cfi::parse`] rejects empty input up front
//!    ([`CfiError::Empty`]).
//! 2. **Character class**: every position must be an ASCII letter; ASCII lowercase letters are
//!    folded to uppercase before taxonomy validation.
//! 3. **Category**: position 1 must be a category defined by ISO 10962
//!    ([`CfiError::UnknownCategory`]).
//! 4. **Group**: position 2 must be a group defined for that category ([`CfiError::UnknownGroup`]).
//! 5. **Attributes**: each of positions 3–6 must be a code the standard permits for the resolved
//!    category and group at that attribute position ([`CfiError::InvalidAttribute`]).
//!
//! Only the classification *codes* are embedded, not ISO's descriptive text, so this crate can
//! tell you a CFI is well-formed and which position is wrong, but it does not resolve the codes to
//! their human-readable meanings.
//!
//! # Design notes
//!
//! - **No invalid state is representable.** [`Cfi`]'s only field is private. There is no unchecked
//!   constructor. Every public constructor and conversion, including byte-array and byte-slice
//!   conversions, runs full validation.
//! - **Zero allocation, `Copy`, allocation-free.** [`Cfi`] is a 6-byte value type wrapping
//!   `[u8; 6]`. Parsing, validating, and every accessor operate on the stack; the taxonomy lookup
//!   is a couple of binary searches and bitmask tests over a `static` table.
//! - **Ordering and hashing are byte-wise.** [`Cfi`] derives [`Ord`] and [`Hash`] directly over its
//!   ASCII bytes, matching [`str`] ordering on [`Cfi::as_str`]. This is a lexicographic string order,
//!   with no taxonomic meaning.
//! - **Safe to use as a map/set key.** [`Cfi`] implements [`Eq`] and [`Hash`] consistently with
//!   [`PartialEq`], so it works as a `HashMap`/`HashSet` or `BTreeMap`/`BTreeSet` key out of the box.
//!
//! # Feature flags
//!
//! This module's optional integrations are off by default and purely additive, enabling one never
//! changes the behavior of [`Cfi::parse`] or the validation rules above:
//!
//! - **`serde`**: (de)serializes [`Cfi`] as its 6-character string (e.g. `"ESVUFR"`).
//!   Deserialization re-runs full validation, so an untrusted payload can never produce an invalid
//!   [`Cfi`].
//! - **`schemars`**: implements `JsonSchema` for [`Cfi`], describing it as a pattern-constrained
//!   string (`^[A-Z]{6}$`). The pattern is structural only; it cannot express which combinations are
//!   taxonomically valid. Implies `serde`.
//! - **`arbitrary`**: implements `Arbitrary` for [`Cfi`], generating taxonomically valid values for
//!   fuzz targets by walking the embedded table.
//! - **`proptest`**: exposes reusable `proptest` strategies (`valqeron_identifiers::cfi::proptest`,
//!   when this feature is enabled) for generating valid [`Cfi`] values.
//!
//! # Error handling
//!
//! Every fallible constructor returns [`CfiError`], which is `Clone + PartialEq + Eq` and implements
//! [`core::error::Error`] and [`core::fmt::Display`], so it composes with `?` and with
//! error-aggregation crates alike:
//!
//! ```
//! use valqeron_identifiers::{Cfi, CfiError};
//!
//! match Cfi::parse("ESZUFR") {
//!     Ok(cfi) => println!("valid: {cfi}"),
//!     Err(CfiError::InvalidAttribute { index, code, .. }) => {
//!         println!("attribute {index} rejected: {code}");
//!     }
//!     Err(other) => println!("rejected: {other}"),
//! }
//! ```
//!
//! # Examples
//!
//! ```
//! use valqeron_identifiers::Cfi;
//!
//! let cfi = Cfi::parse("ESVUFR").unwrap();
//! assert_eq!(cfi.category(), 'E');
//! assert_eq!(cfi.group(), 'S');
//! assert_eq!(cfi.attributes(), ['V', 'U', 'F', 'R']);
//! assert_eq!(cfi.as_str(), "ESVUFR");
//! ```
//!
//! Sorting and deduplicating a batch of CFIs, e.g., after importing them from a spreadsheet:
//!
//! ```
//! use valqeron_identifiers::Cfi;
//!
//! let mut cfis: Vec<Cfi> = ["ESVUFR", "DBFTFB", "ESVUFR"]
//!     .into_iter()
//!     .map(|s| Cfi::parse(s).unwrap())
//!     .collect();
//! cfis.sort();
//! cfis.dedup();
//! assert_eq!(cfis.len(), 2);
//! ```

use arbitrary::{Arbitrary, Unstructured};
use core::convert::TryFrom;
use core::str::{FromStr, from_utf8_unchecked};
use proptest::arbitrary::any;
use proptest::prelude::{Just, Strategy, prop};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::borrow::Cow;
use std::fmt;
use valqeron_macros::generate_cfi_table;

// Procedural Macros for CFI Table Generation
struct CfiGroupEntry {
    pub code: u8,
    pub attrs: [u32; 4],
}

struct CfiCategoryEntry {
    pub code: u8,
    pub groups: &'static [CfiGroupEntry],
}

generate_cfi_table!("data/cfi.json");

/// A validated CFI (Classification of Financial Instruments, ISO 10962).
///
/// `Cfi` is a 6-byte, `Copy`, allocation-free value object. Once constructed, it is guaranteed to
/// describe a category, group, and four attribute codes defined by ISO 10962 — there is no way to
/// get a `Cfi` that hasn't passed validation.
///
/// Internally, the identifier is stored as raw uppercase ASCII letters (`'A'...='Z'`).
///
/// # Constructing a `Cfi`
///
/// | Constructor                    | Accepts                                             |
/// |---------------------------------|-----------------------------------------------------|
/// | [`Cfi::parse`] / [`Cfi::new`]   | 6-character strings, any ASCII case, trimmed         |
/// | [`Cfi::from_bytes`]             | Exactly 6 pre-normalized uppercase ASCII bytes       |
/// | [`FromStr`] / [`TryFrom<&str>`] | Same as `parse`, for use in generic code            |
///
/// All of them run the same validation and return [`CfiError`] on failure.
/// See the [module-level documentation](self) for the segment layout and design rationale.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed Cfi should be used; discarding it wastes the validation work"]
pub struct Cfi {
    bytes: [u8; 6],
}

impl Cfi {
    /// Parses a CFI from a string.
    ///
    /// The parser trims surrounding whitespace and folds ASCII letters to uppercase before
    /// validation. This is the primary constructor; [`Cfi::new`], [`FromStr`], and
    /// [`TryFrom<&str>`] all delegate to it.
    ///
    /// # Errors
    ///
    /// Returns [`CfiError`] if the input is empty, does not contain exactly 6 characters after
    /// trimming, contains a non-letter character, or names a category, group, or attribute code
    /// that ISO 10962 does not define.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cfi;
    ///
    /// assert!(Cfi::parse("ESVUFR").is_ok());
    /// assert!(Cfi::parse("esvufr").is_ok()); // lowercase is folded automatically
    /// assert!(Cfi::parse(" ESVUFR ").is_ok()); // surrounding whitespace is trimmed
    /// assert!(Cfi::parse("EZVUFR").is_err()); // 'Z' is not a group of category 'E'
    /// ```
    pub fn parse(input: &str) -> Result<Self, CfiError> {
        let candidate = normalize(input)?;
        Self::from_bytes(candidate)
    }

    /// Alias for [`Cfi::parse`].
    ///
    /// # Errors
    ///
    /// See [`Cfi::parse`].
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cfi;
    ///
    /// assert_eq!(Cfi::new("ESVUFR"), Cfi::parse("ESVUFR"));
    /// ```
    #[inline]
    pub fn new(input: &str) -> Result<Self, CfiError> {
        Self::parse(input)
    }

    /// Constructs a `Cfi` directly from 6 raw ASCII bytes.
    ///
    /// Each byte must already be an uppercase letter valid for its position. Use [`Cfi::parse`] if
    /// the input might contain surrounding whitespace or lowercase letters.
    ///
    /// # Errors
    ///
    /// Returns [`CfiError`] under the same conditions as [`Cfi::parse`], except that length is
    /// guaranteed by the `[u8; 6]` type itself: [`CfiError::InvalidLength`] cannot occur here.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cfi;
    ///
    /// let cfi = Cfi::from_bytes(*b"ESVUFR").unwrap();
    /// assert_eq!(cfi.as_str(), "ESVUFR");
    ///
    /// // An undefined attribute code is rejected just like it would be through `parse`.
    /// assert!(Cfi::from_bytes(*b"ESZUFR").is_err());
    /// ```
    pub fn from_bytes(bytes: [u8; 6]) -> Result<Self, CfiError> {
        validate(&bytes)?;
        Ok(Cfi { bytes })
    }

    /// Returns the 6 raw ASCII bytes backing this CFI (for example, `b"ESVUFR"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cfi;
    ///
    /// let cfi = Cfi::parse("ESVUFR").unwrap();
    /// assert_eq!(cfi.as_bytes(), b"ESVUFR");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 6] {
        &self.bytes
    }

    /// Returns the full 6-character CFI as a `&str`.
    ///
    /// This never allocates: the bytes are guaranteed to be valid ASCII by construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cfi;
    ///
    /// let cfi = Cfi::parse("ESVUFR").unwrap();
    /// assert_eq!(cfi.as_str(), "ESVUFR");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `Cfi::from_bytes` guarantees every byte is an uppercase ASCII letter.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }

    /// Returns the category code (position 1).
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cfi;
    ///
    /// let cfi = Cfi::parse("ESVUFR").unwrap();
    /// assert_eq!(cfi.category(), 'E');
    /// ```
    #[inline]
    #[must_use]
    pub fn category(&self) -> char {
        self.bytes[0] as char
    }

    /// Returns the group code (position 2).
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cfi;
    ///
    /// let cfi = Cfi::parse("ESVUFR").unwrap();
    /// assert_eq!(cfi.group(), 'S');
    /// ```
    #[inline]
    #[must_use]
    pub fn group(&self) -> char {
        self.bytes[1] as char
    }

    /// Returns the four attribute codes (positions 3–6), in order.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cfi;
    ///
    /// let cfi = Cfi::parse("ESVUFR").unwrap();
    /// assert_eq!(cfi.attributes(), ['V', 'U', 'F', 'R']);
    /// ```
    #[inline]
    #[must_use]
    pub fn attributes(&self) -> [char; 4] {
        [
            self.bytes[2] as char,
            self.bytes[3] as char,
            self.bytes[4] as char,
            self.bytes[5] as char,
        ]
    }
}

impl FromStr for Cfi {
    type Err = CfiError;

    /// Delegates to [`Cfi::parse`], enabling `input.parse::<Cfi>()` and use in generic code bounded
    /// by [`FromStr`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Cfi {
    type Error = CfiError;

    /// Delegates to [`Cfi::parse`], enabling `Cfi::try_from(input)` and use in generic code bounded
    /// by [`TryFrom<&str>`].
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 6]> for Cfi {
    type Error = CfiError;

    /// Delegates to [`Cfi::from_bytes`]. The bytes must already be pre normalized uppercase ASCII
    /// letters.
    fn try_from(value: [u8; 6]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for Cfi {
    type Error = CfiError;

    /// Validates a byte slice as a CFI. The slice must be exactly 6 pre normalized uppercase ASCII
    /// bytes; any other length yields [`CfiError::InvalidLength`]. Once the length is confirmed,
    /// this behaves like [`Cfi::from_bytes`].
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 6] = value
            .try_into()
            .map_err(|_| CfiError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
    }
}

impl PartialEq<str> for Cfi {
    /// Compares against a string slice by its canonical 6 character representation.
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Cfi {
    /// Compares against a string slice by its canonical 6 character representation.
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
    /// Equivalent to [`Cfi::as_bytes`], borrowed as a slice.
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for Cfi {
    /// Equivalent to [`Cfi::as_str`].
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Cfi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Cfi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Cfi").field(&self.as_str()).finish()
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
            return Err(CfiError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
            });
        }
        buf[i] = ch.to_ascii_uppercase() as u8;
    }

    Ok(buf)
}

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

/// A strategy producing taxonomically valid [`Cfi`] values by walking the embedded ISO 10962 table:
/// it picks a category, then a group within it, then a permitted letter for each of the four
/// attribute positions.
pub fn valid_cfi() -> impl Strategy<Value = Cfi> {
    (0..CFI_CATEGORIES.len())
        .prop_flat_map(|category_index| {
            let group_count = CFI_CATEGORIES[category_index].groups.len();
            (Just(category_index), 0..group_count)
        })
        .prop_flat_map(|(category_index, group_index)| {
            (
                Just(category_index),
                Just(group_index),
                prop::array::uniform4(any::<usize>()),
            )
        })
        .prop_map(|(category_index, group_index, selectors)| {
            let category = &CFI_CATEGORIES[category_index];
            let group = &category.groups[group_index];

            let mut bytes = [0u8; 6];
            bytes[0] = category.code;
            bytes[1] = group.code;
            for (i, selector) in selectors.iter().enumerate() {
                bytes[2 + i] = nth_letter(group.attrs[i], *selector);
            }

            Cfi::from_bytes(bytes)
                .expect("generated candidate is taxonomically valid by construction")
        })
}

/// A strategy producing a valid [`Cfi`] rendered as its canonical 6-character `String`, useful for
/// round-trip-through-parsing property tests.
pub fn valid_cfi_string() -> impl Strategy<Value = String> {
    valid_cfi().prop_map(|cfi| cfi.as_str().to_string())
}

/// Runs every validation rule against a normalized candidate, cheapest first:
/// 1. Character class — all six positions must be uppercase ASCII letters.
/// 2. Taxonomy — the category, group, and four attribute codes must exist in ISO 10962.
fn validate(candidate: &[u8; 6]) -> Result<(), CfiError> {
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
fn find_category(code: u8) -> Option<&'static CfiCategoryEntry> {
    let index = CFI_CATEGORIES
        .binary_search_by_key(&code, |c| c.code)
        .ok()?;
    Some(&CFI_CATEGORIES[index])
}

/// Looks up a group by its code (position 2) within a category via binary search.
fn find_group(category: &'static CfiCategoryEntry, code: u8) -> Option<&'static CfiGroupEntry> {
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
fn nth_letter(mask: u32, n: usize) -> u8 {
    let count = mask.count_ones() as usize;
    let target = n % count;
    (0u8..26)
        .filter(|bit| (mask >> bit) & 1 == 1)
        .nth(target)
        .map(|bit| b'A' + bit)
        .expect("attribute masks in the generated table are always non-zero")
}

impl Serialize for Cfi {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct CfiVisitor;

impl<'de> Visitor<'de> for CfiVisitor {
    type Value = Cfi;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 6-character CFI (ISO 10962), e.g. ESVUFR")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Cfi::parse(v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Cfi {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(CfiVisitor)
    }
}

impl JsonSchema for Cfi {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Cfi")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "cfi",
            "minLength": 6,
            "maxLength": 6,
            "pattern": "^[A-Z]{6}$",
            "description": "CFI (Classification of Financial Instruments, ISO 10962). \
            The pattern is structural; taxonomic validity is enforced on deserialization."
        })
    }
}

impl<'a> Arbitrary<'a> for Cfi {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Walk the embedded taxonomy so every generated value is valid by construction: a category,
        // a group within it, and a permitted letter for each of the four attribute positions.
        let category = u.choose(CFI_CATEGORIES)?;
        let group = u.choose(category.groups)?;

        let mut bytes = [0u8; 6];
        bytes[0] = category.code;
        bytes[1] = group.code;
        for (i, &mask) in group.attrs.iter().enumerate() {
            let selector = u.arbitrary::<u8>()? as usize;
            bytes[2 + i] = nth_letter(mask, selector);
        }

        Cfi::from_bytes(bytes).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

#[cfg(test)]
mod tests_serde {
    use crate::identifiers::cfi::Cfi;

    #[test]
    fn round_trips_through_json() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        let json = serde_json::to_string(&cfi).unwrap();
        assert_eq!(json, "\"ESVUFR\"");
        let back: Cfi = serde_json::from_str(&json).unwrap();
        assert_eq!(cfi, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<Cfi>("\"not-a-cfi\"").unwrap_err();
        assert!(err.to_string().contains("CFI"));
    }
}

#[cfg(test)]
mod tests_formating {
    use crate::identifiers::cfi::Cfi;
    use std::format;
    use std::string::ToString;

    #[test]
    fn display_is_the_canonical_string() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        assert_eq!(cfi.to_string(), "ESVUFR");
    }

    #[test]
    fn debug_is_readable() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        assert_eq!(format!("{cfi:?}"), "Cfi(\"ESVUFR\")");
    }
}

#[cfg(test)]
mod tests_parsing {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize(""), Err(CfiError::Empty));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(normalize("  ESVUFR \t\n"), normalize("ESVUFR"));
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
        // An interior space survives normalization (count is still 6) and is left for
        // `validation` to reject as a non-letter character.
        assert_eq!(normalize("ES VFR").unwrap(), *b"ES VFR");
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("ESVUF£").unwrap_err();
        assert!(matches!(
            err,
            CfiError::InvalidCharacter {
                character: '£', ..
            }
        ));
    }

    #[test]
    fn trims_non_ascii_whitespace() {
        assert_eq!(normalize("\u{00A0}ESVUFR\u{00A0}"), normalize("ESVUFR"));
    }
}

#[cfg(test)]
mod tests_proptest {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_cfi_always_round_trips_through_parse(cfi in valid_cfi()) {
            let reparsed = Cfi::parse(cfi.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(cfi, reparsed.unwrap());
        }

        #[test]
        fn valid_cfi_string_always_parses(s in valid_cfi_string()) {
            prop_assert!(Cfi::parse(&s).is_ok());
        }
    }
}

#[cfg(test)]
mod tests_validation {
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

#[cfg(test)]
mod tests_schemars {
    use crate::identifiers::cfi::Cfi;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(Cfi);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "cfi");
        assert_eq!(json["minLength"], 6);
        assert_eq!(json["maxLength"], 6);
        assert_eq!(json["pattern"], "^[A-Z]{6}$");
    }
}

#[cfg(test)]
mod tests_arbitrary {
    use super::*;

    #[test]
    fn always_produces_valid_cfis() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let cfi = Cfi::arbitrary(&mut u).expect("arbitrary should always succeed");
            // Re-validating via parse() proves the value round-trips through the exact same checks
            // a hand-typed input would.
            assert!(Cfi::parse(cfi.as_str()).is_ok());
        }
    }
}
