//! ISO 3166-1 alpha-2 country codes: the two letter codes that identify countries, dependent
//! territories, and special areas of geographical interest.
//!
//! This module provides the validated Rust representation ([`CountryCode`]) together with the
//! parsing, validation, and error types that surround it. It accepts the canonical two letter form
//! (optionally surrounded by whitespace, in any ASCII case), normalizes it, and guarantees that any
//! constructed [`CountryCode`] is a code that ISO 3166-1 officially assigns. There is no partially
//! validated state. If you hold a [`CountryCode`], it is valid.
//!
//! # What this type represents
//!
//! A country code is two uppercase ASCII letters, for example `US`, `BR`, or `GB`. The code
//! identifies a country or territory. This crate stores only the code itself. It does not carry the
//! country name, the alpha-3 code, or the numeric code, and it does not model subdivisions.
//!
//! [`CountryCode`] stores the two characters as normalized uppercase ASCII. It exposes borrowed
//! accessors for the raw bytes ([`CountryCode::as_bytes`]) and for the whole value
//! ([`CountryCode::as_str`]).
//!
//! # Validation rules
//!
//! A country code has no check digit. It is valid exactly when it is one of the codes ISO 3166-1
//! officially assigns. This crate embeds that set as a compile time bitmap. Every fallible
//! constructor runs the same rules, in order, and each maps to one [`CountryCodeError`] variant:
//!
//! 1. Length: after surrounding whitespace is trimmed, the input must contain exactly two
//!    characters ([`CountryCodeError::InvalidLength`]). [`CountryCode::parse`] rejects empty input
//!    first ([`CountryCodeError::Empty`]).
//! 2. Character class: both positions must be an uppercase ASCII letter
//!    ([`CountryCodeError::InvalidCharacter`]).
//! 3. Assignment: the two letters together must be an officially assigned code
//!    ([`CountryCodeError::Unassigned`]).
//!
//! Only the assigned codes are recognized. Reserved codes such as `EU` and `UK`, and the user
//! assigned ranges, are not accepted. Codes that were once used and later withdrawn are not
//! accepted either.
//!
//! # Design notes
//!
//! * No invalid state is representable. The only field of [`CountryCode`] is private. Every way to
//!   obtain one ([`CountryCode::parse`], [`CountryCode::new`], [`CountryCode::from_bytes`],
//!   [`FromStr`], and [`TryFrom<&str>`]) runs full validation. There is no unchecked constructor.
//! * It is zero allocation and `Copy`. [`CountryCode`] is a two byte value that wraps `[u8; 2]`. It
//!   works in `no_std` environments. Parsing, validation, and every accessor operate on the stack.
//!   The assignment check computes one array index and tests one bit.
//! * Ordering and hashing operate over the raw ASCII bytes. This matches [`str`] ordering on
//!   [`CountryCode::as_str`], which is lexicographic and carries no geographic meaning.
//! * It is safe to use as a map or set key. [`CountryCode`] implements [`Eq`] and [`Hash`]
//!   consistently with [`PartialEq`], so it works as a `HashMap` or `HashSet` key, and as a
//!   `BTreeMap` or `BTreeSet` key, out of the box.
//!
//! # Feature flags
//!
//! The optional integrations are off by default and purely additive. Enabling one never changes the
//! behavior of [`CountryCode::parse`] or the validation rules above:
//!
//! * `serde`: (de)serializes [`CountryCode`] as its two letter string, for example `"US"`.
//!   Deserialization re-runs full validation, so an untrusted payload can never produce an invalid
//!   [`CountryCode`].
//! * `schemars`: implements `JsonSchema` for [`CountryCode`], describing it as a pattern
//!   constrained string (`^[A-Z]{2}$`). The pattern is structural. It cannot express which two
//!   letter codes are assigned, so validity is enforced on deserialization. Implies `serde`.
//! * `arbitrary`: implements `Arbitrary` for [`CountryCode`], generating officially assigned codes
//!   for fuzz targets.
//! * `proptest`: exposes reusable `proptest` strategies (`ftracker_identifiers::country::proptest`,
//!   when this feature is enabled) for generating valid [`CountryCode`] values.
//!
//! # Error handling
//!
//! Every fallible constructor returns [`CountryCodeError`], which is `Clone + PartialEq + Eq` and
//! implements [`core::error::Error`] and [`core::fmt::Display`], so it composes with `?` and with
//! error aggregation crates alike:
//!
//! ```
//! use ftracker_identifiers::{CountryCode, CountryCodeError};
//!
//! match CountryCode::parse("ZZ") {
//!     Ok(code) => println!("valid: {code}"),
//!     Err(CountryCodeError::Unassigned { code }) => {
//!         println!("not assigned: {}{}", code[0], code[1]);
//!     }
//!     Err(other) => println!("rejected: {other}"),
//! }
//! ```
//!
//! # Examples
//!
//! ```
//! use ftracker_identifiers::CountryCode;
//!
//! let code = CountryCode::parse("us").unwrap(); // lowercase is folded automatically
//! assert_eq!(code.as_str(), "US");
//! assert_eq!(code.as_bytes(), b"US");
//! ```
//!
//! Sorting and deduplicating a batch of codes, for example after importing them from a spreadsheet:
//!
//! ```
//! use ftracker_identifiers::CountryCode;
//!
//! let mut codes: Vec<CountryCode> = ["US", "BR", "US"]
//!     .into_iter()
//!     .map(|s| CountryCode::parse(s).unwrap())
//!     .collect();
//! codes.sort();
//! codes.dedup();
//! assert_eq!(codes.len(), 2);
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

#[cfg(test)]
mod tests;

pub use error::CountryCodeError;

use core::convert::TryFrom;
use core::str::{FromStr, from_utf8_unchecked};

/// A validated ISO 3166-1 alpha-2 country code.
///
/// `CountryCode` is a two byte, `Copy`, allocation free value object. Once constructed, it is
/// guaranteed to be a code that ISO 3166-1 officially assigns. There is no way to get a
/// `CountryCode` that has not passed validation.
///
/// Internally, the code is stored as two raw uppercase ASCII letters (`'A'..='Z'`).
///
/// # Constructing a `CountryCode`
///
/// * [`CountryCode::parse`] and [`CountryCode::new`] accept two character strings, in any ASCII
///   case, trimmed of surrounding whitespace.
/// * [`CountryCode::from_bytes`] accepts exactly two pre normalized uppercase ASCII bytes.
/// * [`FromStr`] and [`TryFrom<&str>`] behave like `parse`, for use in generic code.
///
/// All of them run the same validation and return [`CountryCodeError`] on failure. See the
/// [module level documentation](self) for the validation rules and design rationale.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed CountryCode should be used; discarding it wastes the validation work"]
pub struct CountryCode {
    bytes: [u8; 2],
}

impl CountryCode {
    /// Parses a country code from a string.
    ///
    /// The parser trims surrounding whitespace and folds ASCII letters to uppercase before
    /// validation. This is the primary constructor. [`CountryCode::new`], [`FromStr`], and
    /// [`TryFrom<&str>`] all delegate to it.
    ///
    /// # Errors
    ///
    /// Returns [`CountryCodeError`] if the input is empty, does not contain exactly two characters
    /// after trimming, contains a non letter character, or names a code that ISO 3166-1 does not
    /// officially assign.
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::CountryCode;
    ///
    /// assert!(CountryCode::parse("US").is_ok());
    /// assert!(CountryCode::parse("us").is_ok()); // lowercase is folded automatically
    /// assert!(CountryCode::parse(" BR ").is_ok()); // surrounding whitespace is trimmed
    /// assert!(CountryCode::parse("ZZ").is_err()); // well formed but not assigned
    /// ```
    pub fn parse(input: &str) -> Result<Self, CountryCodeError> {
        let candidate = parser::normalize(input)?;
        Self::from_bytes(candidate)
    }

    /// Alias for [`CountryCode::parse`].
    ///
    /// # Errors
    ///
    /// See [`CountryCode::parse`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::CountryCode;
    ///
    /// assert_eq!(CountryCode::new("US"), CountryCode::parse("US"));
    /// ```
    #[inline]
    pub fn new(input: &str) -> Result<Self, CountryCodeError> {
        Self::parse(input)
    }

    /// Constructs a `CountryCode` directly from two raw ASCII bytes.
    ///
    /// Each byte must already be an uppercase letter. Use [`CountryCode::parse`] if the input might
    /// contain surrounding whitespace or lowercase letters.
    ///
    /// # Errors
    ///
    /// Returns [`CountryCodeError`] under the same conditions as [`CountryCode::parse`], except that
    /// length is guaranteed by the `[u8; 2]` type itself: [`CountryCodeError::InvalidLength`] cannot
    /// occur here.
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::CountryCode;
    ///
    /// let code = CountryCode::from_bytes(*b"US").unwrap();
    /// assert_eq!(code.as_str(), "US");
    ///
    /// // A well formed but unassigned code is rejected just like it would be through `parse`.
    /// assert!(CountryCode::from_bytes(*b"ZZ").is_err());
    /// ```
    pub fn from_bytes(bytes: [u8; 2]) -> Result<Self, CountryCodeError> {
        validation::validate(&bytes)?;
        Ok(CountryCode { bytes })
    }

    /// Returns the two raw ASCII bytes backing this code (for example, `b"US"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::CountryCode;
    ///
    /// let code = CountryCode::parse("US").unwrap();
    /// assert_eq!(code.as_bytes(), b"US");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 2] {
        &self.bytes
    }

    /// Returns the two character country code as a `&str`.
    ///
    /// This never allocates. The bytes are guaranteed to be valid ASCII by construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::CountryCode;
    ///
    /// let code = CountryCode::parse("US").unwrap();
    /// assert_eq!(code.as_str(), "US");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `CountryCode::from_bytes` guarantees both bytes are uppercase ASCII letters.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }
}

impl FromStr for CountryCode {
    type Err = CountryCodeError;

    /// Delegates to [`CountryCode::parse`], enabling `input.parse::<CountryCode>()` and use in
    /// generic code bounded by [`FromStr`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for CountryCode {
    type Error = CountryCodeError;

    /// Delegates to [`CountryCode::parse`], enabling `CountryCode::try_from(input)` and use in
    /// generic code bounded by [`TryFrom<&str>`].
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 2]> for CountryCode {
    type Error = CountryCodeError;

    /// Delegates to [`CountryCode::from_bytes`]. The two bytes must already be pre normalized
    /// uppercase ASCII letters.
    fn try_from(value: [u8; 2]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for CountryCode {
    type Error = CountryCodeError;

    /// Validates a byte slice as a country code. The slice must be exactly two pre normalized
    /// uppercase ASCII bytes; any other length yields [`CountryCodeError::InvalidLength`]. Once the
    /// length is confirmed, this behaves like [`CountryCode::from_bytes`].
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 2] = value
            .try_into()
            .map_err(|_| CountryCodeError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
    }
}

impl PartialEq<str> for CountryCode {
    /// Compares against a string slice by its canonical two letter representation.
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for CountryCode {
    /// Compares against a string slice by its canonical two letter representation.
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<CountryCode> for str {
    fn eq(&self, other: &CountryCode) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<CountryCode> for &str {
    fn eq(&self, other: &CountryCode) -> bool {
        *self == other.as_str()
    }
}

impl AsRef<[u8]> for CountryCode {
    /// Equivalent to [`CountryCode::as_bytes`], borrowed as a slice.
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for CountryCode {
    /// Equivalent to [`CountryCode::as_str`].
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
