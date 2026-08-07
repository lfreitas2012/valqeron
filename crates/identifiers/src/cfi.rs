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
//! 2. **Character class**: every position must be an uppercase ASCII letter
//!    ([`CfiError::InvalidCharacter`]).
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
//! - **No invalid state is representable.** [`Cfi`]'s only field is private; the only ways to
//!   get one — [`Cfi::parse`], [`Cfi::new`], [`Cfi::from_bytes`], [`FromStr`], and
//!   [`TryFrom<&str>`], all run full validation. There is no unchecked constructor.
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

mod error;
mod fmt;
mod parser;
mod table;
mod validation;

#[cfg(feature = "serde")]
mod serde;

#[cfg(feature = "schemars")]
mod schema;

#[cfg(feature = "arbitrary")]
mod arbitrary;

#[cfg(any(test, feature = "proptest"))]
pub mod proptest;

pub use error::CfiError;

use core::convert::TryFrom;
use core::str::{FromStr, from_utf8_unchecked};

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
        let candidate = parser::normalize(input)?;
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
        validation::validate(&bytes)?;
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
