//! CFI (Classification of Financial Instruments), the ISO 10962 six-letter code that classifies a
//! financial instrument by category, group, and four attributes.
//!
//! This crate provides the validated Rust representation ([`Cfi`]) and the parsing, validation,
//! and error types that surround it. It accepts the canonical 6-character form (optionally
//! surrounded by whitespace, in any ASCII case), normalizes it, and guarantees that any constructed
//! [`Cfi`] describes a combination actually defined by ISO 10962.
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
//! # Validation rules
//!
//! A CFI validity is defined entirely by the ISO 10962 code taxonomy, which this crate embeds
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
//!
//! # Error handling
//!
//! Every fallible constructor returns [`CfiError`], which is `Clone + PartialEq + Eq` and implements
//! [`core::error::Error`] and [`Display`], so it composes with `?` and with error-aggregation
//! alike:
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

use core::convert::TryFrom;
use core::str::{FromStr, from_utf8_unchecked};
use fmt::{Debug, Display};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use valqeron_macros::generate_cfi_table;

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
/// | Constructor | Accepts |
/// |---------------------------------|-----------------------------------------------------|
/// | [`Cfi::parse`] / [`Cfi::new`] | 6-character strings, any ASCII case, trimmed |
/// | [`Cfi::from_bytes`] | Exactly 6 pre-normalized uppercase ASCII bytes |
/// | [`FromStr`] / [`TryFrom<&str>`] | Same as `parse`, for use in generic code            |
///
/// All of them run the same validation and return [`CfiError`] on failure.
/// See the [module-level documentation](self) for the segment layout and design rationale.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[must_use = "A parsed CFI must be used."]
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

generate_cfi_table!("data/cfi.json");

/// Represents an entry in a "CfiGroup".
struct CfiGroupEntry {
    pub code: u8,
    pub attrs: [u32; 4],
}

/// Represents a category entry in the CFI (Classification of Financial Instruments) structure.
struct CfiCategoryEntry {
    pub code: u8,
    pub groups: &'static [CfiGroupEntry],
}

#[derive(Debug, Error)]
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

/// Validates a candidate CFI (Common Format Identifier) code.
///
/// # Parameters
/// - `candidate`: A reference to an array of 6 bytes representing the CFI code to validate.
///
/// # Returns
/// - `Ok(())`: If the candidate code passes all validation checks.
/// - `Err(CfiError)`: If the candidate code fails any of the validation checks.
///
/// # Errors
/// This function may return an error in the following cases:
/// - `CfiError::InvalidCharacter`: if any of the candidate bytes are not uppercase ASCII letters.
/// - `CfiError::UnknownCategory`: if the candidate code's category code is not recognized.
/// - `CfiError::UnknownGroup`: if the candidate code's group code is not recognized.
/// - `CfiError::InvalidAttribute`: if any of the candidate code's attribute codes are not
/// recognized.
///
/// # Function Details
/// - Calls `validate_character_classes` to ensure the candidate adheres to defined character class
/// rules.
/// - Calls `validate_taxonomy` to verify the taxonomy or structure of the candidate.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Builds a `[u8; 6]` from a 6-byte ASCII string literal, for feeding directly into
    /// `validate`/`from_bytes` without going through `normalize`.
    fn candidate(s: &str) -> [u8; 6] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 6];
        out.copy_from_slice(bytes);
        out
    }

    // ---- normalization (`normalize`) ----

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
        // `validate` to reject as a non-letter character.
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

    // ---- validation (`validate` and friends) ----

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

    // ---- constructors (`parse`, `new`, `from_bytes`) ----

    #[test]
    fn new_is_an_alias_for_parse() {
        assert_eq!(Cfi::new("ESVUFR"), Cfi::parse("ESVUFR"));
    }

    #[test]
    fn from_bytes_rejects_invalid_attribute_without_normalizing() {
        // from_bytes skips normalize, so it never produces InvalidLength — but taxonomy
        // validation still runs.
        assert_eq!(
            Cfi::from_bytes(candidate("ESZUFR")).unwrap_err(),
            CfiError::InvalidAttribute {
                category: 'E',
                group: 'S',
                index: 1,
                code: 'Z',
            }
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
        let err = "ESVU".parse::<Cfi>().unwrap_err();
        assert_eq!(err, CfiError::InvalidLength { found: 4 });
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
        let cfi = Cfi::try_from(&b"ESVUFR"[..]).unwrap();
        assert_eq!(cfi.as_str(), "ESVUFR");
    }

    #[test]
    fn try_from_slice_rejects_wrong_length() {
        let err = Cfi::try_from(&b"ESVU"[..]).unwrap_err();
        assert_eq!(err, CfiError::InvalidLength { found: 4 });
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

    // ---- Clone / Copy / Ord / Hash ----

    #[test]
    fn is_copy_and_clone() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        let copied = cfi; // Copy: `cfi` remains usable below.
        let cloned = cfi.clone();
        assert_eq!(cfi, copied);
        assert_eq!(cfi, cloned);
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
        set.insert(Cfi::parse("ESVUFR").unwrap()); // duplicate, should not grow the set
        assert_eq!(set.len(), 2);
    }

    // ---- Display / Debug ----

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

    // ---- serde ----

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
