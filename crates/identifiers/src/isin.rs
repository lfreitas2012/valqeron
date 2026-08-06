//! ISIN (International Securities Identification Number) — the ISO 6166 identifier for a fungible
//! financial security.
//!
//! This module provides the validated Rust representation ([`Isin`]) and the parsing, validation,
//! and error types that surround it. It accepts the canonical 12-character form (optionally
//! surrounded by whitespace, in any ASCII case), normalizes it, and guarantees that any constructed
//! [`Isin`] satisfies the structural rules and the Luhn check digit described below. There is no
//! partially-validated state: if you hold an [`Isin`], it is valid.
//!
//! # What this type represents
//!
//! An ISIN has 12 characters, split into three segments:
//!
//! | Positions | Length | Segment       | Meaning                                                           |
//! |-----------|--------|---------------|-------------------------------------------------------------------|
//! | 1–2       | 2      | Country code  | ISO 3166-1 alpha-2 prefix of the issuing national numbering agency |
//! | 3–11      | 9      | NSIN          | National Securities Identifying Number, alphanumeric              |
//! | 12        | 1      | Check digit   | Luhn (modulus 10) digit computed over the first 11 characters     |
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │  CC    │           NSIN (9 chars)          │ Check (1) │
//! │  A  A  │       N  N  N  N  N  N  N  N  N    │     D     │
//! └──────────────────────────────────────────────────────┘
//! ```
//!
//! [`Isin`] stores those 12 characters as normalized uppercase ASCII and exposes borrowed accessors
//! for the country code ([`Isin::country_code`]), the NSIN ([`Isin::nsin`]), the check digit
//! ([`Isin::check_digit`]), and the whole value ([`Isin::as_str`]).
//!
//! # Validation rules
//!
//! Every fallible constructor runs the same rules, in order, and each maps to one [`IsinError`]
//! variant:
//!
//! 1. **Length** — after surrounding whitespace is trimmed, the input must contain exactly 12
//!    characters ([`IsinError::InvalidLength`]). [`Isin::parse`] rejects empty input up front
//!    ([`IsinError::Empty`]).
//! 2. **Character class** — positions 1–2 accept an uppercase letter, positions 3–11 accept a digit
//!    or an uppercase letter, and position 12 accepts only a digit ([`IsinError::InvalidCharacter`]).
//! 3. **Check digit** — position 12 must match the ISO 6166 Luhn digit computed from the first 11
//!    characters ([`IsinError::InvalidCheckDigit`]).
//!
//! The country code is validated *structurally* (two uppercase letters); this crate deliberately
//! does not check it against the live ISO 3166-1 country registry, which changes over time and is
//! out of scope for a checksum-oriented value type.
//!
//! # Design notes
//!
//! - **No invalid state is representable.** [`Isin`]'s only field is private; the only ways to
//!   obtain one — [`Isin::parse`], [`Isin::new`], [`Isin::from_bytes`], [`FromStr`], and
//!   [`TryFrom<&str>`] — all run full validation. There is no unchecked constructor.
//! - **Zero allocation, `Copy`, `no_std`-friendly.** [`Isin`] is a 12-byte value type wrapping
//!   `[u8; 12]`. Parsing, validating, and every accessor operate on the stack.
//! - **Ordering and hashing are byte-wise.** [`Isin`] derives [`Ord`] and [`Hash`] directly over
//!   its ASCII bytes, which matches [`str`] ordering on [`Isin::as_str`]. This is lexicographic
//!   string order, not any notion of issuance order.
//! - **Safe to use as a map/set key.** [`Isin`] implements [`Eq`] and [`Hash`] consistently with
//!   [`PartialEq`], so it works as a `HashMap`/`HashSet` or `BTreeMap`/`BTreeSet` key out of the box.
//!
//! # Feature flags
//!
//! This module's optional integrations are off by default and purely additive — enabling one never
//! changes the behavior of [`Isin::parse`] or the validation rules above:
//!
//! - **`serde`** — (de)serializes [`Isin`] as its 12-character string (e.g. `"US0378331005"`).
//!   Deserialization re-runs full validation, so an untrusted payload can never produce an invalid
//!   [`Isin`].
//! - **`schemars`** — implements `JsonSchema` for [`Isin`], describing it as a pattern-constrained
//!   string (`^[A-Z]{2}[A-Z0-9]{9}[0-9]$`). Implies `serde`.
//! - **`arbitrary`** — implements `Arbitrary` for [`Isin`], generating structurally valid,
//!   checksum-correct values for fuzz targets.
//! - **`proptest`** — exposes reusable `proptest` strategies (`ftracker_identifiers::isin::proptest`,
//!   when this feature is enabled) for generating checksum-valid [`Isin`] values.
//!
//! # Error handling
//!
//! Every fallible constructor returns [`IsinError`], which is `Clone + PartialEq + Eq` and
//! implements [`core::error::Error`] and [`core::fmt::Display`], so it composes with `?` and with
//! error-aggregation crates alike:
//!
//! ```
//! use ftracker_identifiers::{Isin, IsinError};
//!
//! match Isin::parse("US0378331006") {
//!     Ok(isin) => println!("valid: {isin}"),
//!     Err(IsinError::InvalidCheckDigit { expected, found }) => {
//!         println!("checksum mismatch: expected {expected}, found {found}");
//!     }
//!     Err(other) => println!("rejected: {other}"),
//! }
//! ```
//!
//! # Examples
//!
//! ```
//! use ftracker_identifiers::Isin;
//!
//! let apple = Isin::parse("US0378331005").unwrap();
//! assert_eq!(apple.country_code(), "US");
//! assert_eq!(apple.nsin(), "037833100");
//! assert_eq!(apple.check_digit(), 5);
//! assert_eq!(apple.as_str(), "US0378331005");
//! ```
//!
//! Sorting and deduplicating a batch of ISINs, e.g. after importing them from a spreadsheet:
//!
//! ```
//! use ftracker_identifiers::Isin;
//!
//! let mut isins: Vec<Isin> = ["US0231351067", "US0378331005", "US0378331005"]
//!     .into_iter()
//!     .map(|s| Isin::parse(s).unwrap())
//!     .collect();
//! isins.sort();
//! isins.dedup();
//! assert_eq!(isins.len(), 2);
//! ```

mod error;
mod fmt;
mod parser;
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

pub use error::IsinError;

use core::convert::TryFrom;
use core::str::{FromStr, from_utf8_unchecked};

/// A validated ISIN (International Securities Identification Number, ISO 6166).
///
/// `Isin` is a 12-byte, `Copy`, allocation-free value object. Once constructed, it is guaranteed to
/// satisfy the structural rules and Luhn check digit required by ISO 6166 — there is no way to
/// obtain an `Isin` that hasn't passed validation.
///
/// Internally, the identifier is stored as raw uppercase ASCII bytes (`'0'..='9'` or `'A'..='Z'`).
///
/// # Constructing an `Isin`
///
/// | Constructor                     | Accepts                                             |
/// |----------------------------------|-----------------------------------------------------|
/// | [`Isin::parse`] / [`Isin::new`]  | 12-character strings, any ASCII case, trimmed        |
/// | [`Isin::from_bytes`]             | Exactly 12 pre-normalized uppercase ASCII bytes      |
/// | [`FromStr`] / [`TryFrom<&str>`]  | Same as `parse`, for use in generic code            |
///
/// All of them run the same validation and return [`IsinError`] on failure. See the [module-level
/// documentation](self) for the segment layout and design rationale.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed Isin should be used; discarding it wastes the validation work"]
pub struct Isin {
    bytes: [u8; 12],
}

impl Isin {
    /// Parses an ISIN from a string.
    ///
    /// The parser trims surrounding whitespace and folds ASCII letters to uppercase before
    /// validation. This is the primary constructor; [`Isin::new`], [`FromStr`], and
    /// [`TryFrom<&str>`] all delegate to it.
    ///
    /// # Errors
    ///
    /// Returns [`IsinError`] if the input is empty, does not contain exactly 12 characters after
    /// trimming, contains a character invalid for its position, or fails the Luhn check digit.
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::Isin;
    ///
    /// assert!(Isin::parse("US0378331005").is_ok());
    /// assert!(Isin::parse("us0378331005").is_ok()); // lowercase is folded automatically
    /// assert!(Isin::parse(" US0378331005 ").is_ok()); // surrounding whitespace is trimmed
    /// assert!(Isin::parse("US0378331006").is_err()); // wrong check digit
    /// ```
    pub fn parse(input: &str) -> Result<Self, IsinError> {
        let candidate = parser::normalize(input)?;
        Self::from_bytes(candidate)
    }

    /// Alias for [`Isin::parse`].
    ///
    /// # Errors
    ///
    /// See [`Isin::parse`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::Isin;
    ///
    /// assert_eq!(Isin::new("US0378331005"), Isin::parse("US0378331005"));
    /// ```
    #[inline]
    pub fn new(input: &str) -> Result<Self, IsinError> {
        Self::parse(input)
    }

    /// Constructs an `Isin` directly from 12 raw ASCII bytes.
    ///
    /// Each byte must already be uppercase and valid for its position (two letters, nine
    /// alphanumerics, one digit). Use [`Isin::parse`] if the input might contain surrounding
    /// whitespace or lowercase letters.
    ///
    /// # Errors
    ///
    /// Returns [`IsinError`] under the same conditions as [`Isin::parse`], except that length is
    /// guaranteed by the `[u8; 12]` type itself: [`IsinError::InvalidLength`] cannot occur here.
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::Isin;
    ///
    /// let isin = Isin::from_bytes(*b"US0378331005").unwrap();
    /// assert_eq!(isin.as_str(), "US0378331005");
    ///
    /// // A malformed checksum is rejected just like it would be through `parse`.
    /// assert!(Isin::from_bytes(*b"US0378331006").is_err());
    /// ```
    pub fn from_bytes(bytes: [u8; 12]) -> Result<Self, IsinError> {
        validation::validate(&bytes)?;
        Ok(Isin { bytes })
    }

    /// Returns the 12 raw ASCII bytes backing this ISIN (for example, `b"US0378331005"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::Isin;
    ///
    /// let isin = Isin::parse("US0378331005").unwrap();
    /// assert_eq!(isin.as_bytes(), b"US0378331005");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 12] {
        &self.bytes
    }

    /// Returns the full 12-character ISIN as a `&str`.
    ///
    /// This never allocates: the bytes are guaranteed to be valid ASCII by construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::Isin;
    ///
    /// let isin = Isin::parse("US0378331005").unwrap();
    /// assert_eq!(isin.as_str(), "US0378331005");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `Isin::from_bytes` guarantees the bytes are ASCII letters and digits only.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }

    /// Returns the two-character ISO 3166-1 alpha-2 country code (positions 1–2).
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::Isin;
    ///
    /// let isin = Isin::parse("US0378331005").unwrap();
    /// assert_eq!(isin.country_code(), "US");
    /// ```
    #[inline]
    #[must_use]
    pub fn country_code(&self) -> &str {
        &self.as_str()[0..2]
    }

    /// Returns the prefix (positions 1-2) as a validated [`CountryCode`](crate::CountryCode), or
    /// `None` when it is not an officially assigned ISO 3166-1 alpha-2 code.
    ///
    /// An [`Isin`] only validates its prefix structurally (two uppercase letters), so it can carry
    /// prefixes that ISO 6166 reserves but ISO 3166-1 does not assign. The most common are `XS`
    /// (used by international clearing systems such as Euroclear and Clearstream), `EU` (European
    /// Union supranational issues), and `QS`. For those, this returns `None` even though the
    /// [`Isin`] itself is valid. Use [`Isin::country_code`] when you want the raw two letter prefix
    /// regardless of assignment.
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::{Isin, CountryCode};
    ///
    /// let apple = Isin::parse("US0378331005").unwrap();
    /// assert_eq!(apple.country(), Some(CountryCode::parse("US").unwrap()));
    /// ```
    #[inline]
    #[must_use]
    pub fn country(&self) -> Option<crate::CountryCode> {
        crate::CountryCode::from_bytes([self.bytes[0], self.bytes[1]]).ok()
    }

    /// Returns the nine-character National Securities Identifying Number (positions 3–11).
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::Isin;
    ///
    /// let isin = Isin::parse("US0378331005").unwrap();
    /// assert_eq!(isin.nsin(), "037833100");
    /// ```
    #[inline]
    #[must_use]
    pub fn nsin(&self) -> &str {
        &self.as_str()[2..11]
    }

    /// Returns the Luhn check digit (position 12) as a numeric value.
    ///
    /// For a valid `Isin`, this always equals [`Isin::computed_check_digit`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::Isin;
    ///
    /// let isin = Isin::parse("US0378331005").unwrap();
    /// assert_eq!(isin.check_digit(), 5);
    /// ```
    #[inline]
    #[must_use]
    pub fn check_digit(&self) -> u8 {
        self.bytes[11] - b'0'
    }

    /// Recomputes the check digit that the ISO 6166 Luhn algorithm produces from the first 11
    /// characters of this value.
    ///
    /// For a valid `Isin` this always matches [`Isin::check_digit`]; the method exists so callers
    /// can reproduce the algorithm's output without a separate crate.
    ///
    /// # Examples
    ///
    /// ```
    /// use ftracker_identifiers::Isin;
    ///
    /// let isin = Isin::parse("US0378331005").unwrap();
    /// assert_eq!(isin.computed_check_digit(), isin.check_digit());
    /// ```
    #[inline]
    #[must_use]
    pub fn computed_check_digit(&self) -> u8 {
        validation::compute_check_digit(&self.bytes[..11])
    }
}

impl FromStr for Isin {
    type Err = IsinError;

    /// Delegates to [`Isin::parse`], enabling `input.parse::<Isin>()` and use in generic code
    /// bounded by [`FromStr`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Isin {
    type Error = IsinError;

    /// Delegates to [`Isin::parse`], enabling `Isin::try_from(input)` and use in generic code
    /// bounded by [`TryFrom<&str>`].
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 12]> for Isin {
    type Error = IsinError;

    /// Delegates to [`Isin::from_bytes`]. The bytes must already be pre normalized uppercase ASCII.
    fn try_from(value: [u8; 12]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for Isin {
    type Error = IsinError;

    /// Validates a byte slice as an ISIN. The slice must be exactly 12 pre normalized uppercase
    /// ASCII bytes; any other length yields [`IsinError::InvalidLength`]. Once the length is
    /// confirmed, this behaves like [`Isin::from_bytes`].
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 12] = value
            .try_into()
            .map_err(|_| IsinError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
    }
}

impl PartialEq<str> for Isin {
    /// Compares against a string slice by its canonical 12 character representation.
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Isin {
    /// Compares against a string slice by its canonical 12 character representation.
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Isin> for str {
    fn eq(&self, other: &Isin) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Isin> for &str {
    fn eq(&self, other: &Isin) -> bool {
        *self == other.as_str()
    }
}

impl AsRef<[u8]> for Isin {
    /// Equivalent to [`Isin::as_bytes`], borrowed as a slice.
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for Isin {
    /// Equivalent to [`Isin::as_str`].
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
