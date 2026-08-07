//! LEI (Legal Entity Identifier) — the ISO 17442 identifier for a legally distinct entity that
//! participates in financial transactions.
//!
//! This module provides the validated Rust representation ([`Lei`]) and the parsing, validation,
//! and error types that surround it. It accepts the canonical 20-character form (optionally
//! surrounded by whitespace, in any ASCII case), normalizes it, and guarantees that any constructed
//! [`Lei`] satisfies the structural rules and the ISO/IEC 7064 MOD 97-10 check digits described below.
//! There is no partially-validated state: if you hold a [`Lei`], it is valid.
//!
//! # What this type represents
//!
//! An LEI has 20 characters, split into three segments (ISO 17442:2020):
//!
//! | Positions | Length | Segment       | Meaning                                                           |
//! |-----------|--------|---------------|-------------------------------------------------------------------|
//! | 1–4       | 4      | LOU prefix    | Identifies the Local Operating Unit (LEI issuer), alphanumeric    |
//! | 5–18      | 14     | Entity part   | Entity-specific identifier assigned by the LOU, alphanumeric      |
//! | 19–20     | 2      | Check digits  | ISO/IEC 7064 MOD 97-10 digits computed over the first 18 chars    |
//!
//! ```text
//! ┌───────────────────────────────────────────────────────────────────┐
//! │  LOU (4)  │          Entity-specific part (14)           │ CK (2) │
//! │ A A A A   │   A  A  A  A  A  A  A  A  A  A  A  A  A  A   │  D  D  │
//! └───────────────────────────────────────────────────────────────────┘
//! ```
//!
//! [`Lei`] stores those 20 characters as normalized uppercase ASCII and exposes borrowed accessors
//! for the LOU prefix ([`Lei::lou_prefix`]), the entity-specific part ([`Lei::entity_id`]), the
//! check digits ([`Lei::check_digits`]), and the whole value ([`Lei::as_str`]).
//!
//! # Validation rules
//!
//! Every fallible constructor runs the same rules, in order, and each maps to one [`LeiError`]
//! variant:
//!
//! 1. **Length**: after surrounding whitespace is trimmed, the input must contain exactly 20
//!    characters ([`LeiError::InvalidLength`]). [`Lei::parse`] rejects empty input up front
//!    ([`LeiError::Empty`]).
//! 2. **Character class**: positions 1–18 accept a digit or an uppercase letter, and positions
//!    19–20 accept only a digit ([`LeiError::InvalidCharacter`]).
//! 3. **Check digits**: positions 19–20 must satisfy the ISO/IEC 7064 MOD 97-10 checksum computed
//!    from the first 18 characters ([`LeiError::InvalidCheckDigits`]).
//!
//! ## Validation policy (deliberate scope)
//!
//! This crate validates an LEI **structurally and by the ISO/IEC 7064 MOD 97-10 arithmetic only**.
//! One thing it deliberately does *not* do belongs to GLEIF operational policy rather than to the
//! ISO 17442 code definition:
//!
//! - **It does not require positions 5–6 to be `"00"`.** The Global LEI System currently allocates
//!   codes with `"00"` there, but that is an issuance convention, not a rule of ISO 17442, and it
//!   may change. Enforcing it would reject otherwise standard-conformant identifiers. This mirrors
//!   how [`Isin`](crate::Isin) validates its country prefix purely structurally.
//!
//! The check digits of a valid LEI are always in `02...=98`: they equal `98 - (n mod 97)`, and per
//! ISO 17442-1 the pair `00`, `01`, and `99` cannot occur. This crate enforces that by comparing
//! against the recomputed pair, so those three values are rejected like any other mismatch.
//!
//! A value that passes therefore is a structurally valid, MOD 97-10-correct LEI per ISO 17442; it
//! is **not** a claim that GLEIF has actually issued that specific code. Look the code up in the
//! Global LEI Index if you need to confirm real-world registration.
//!
//! # Design notes
//!
//! - **No invalid state is representable.** [`Lei`]'s only field is private; the only ways to obtain
//!   one is through [`Lei::parse`], [`Lei::new`], [`Lei::from_bytes`], [`FromStr`], and
//!   [`TryFrom<&str>`]; all run full validation. There is no unchecked constructor.
//! - **Zero allocation, `Copy`, allocation-free.** [`Lei`] is a 20-byte value type wrapping
//!   `[u8; 20]`. Parsing, validating, and every accessor operate on the stack.
//! - **Ordering and hashing are byte-wise.** [`Lei`] derives [`Ord`] and [`Hash`] directly over its
//!   ASCII bytes, which matches [`str`] ordering on [`Lei::as_str`]. This is lexicographic string
//!   order, not any notion of issuance order.
//! - **Safe to use as a map/set key.** [`Lei`] implements [`Eq`] and [`Hash`] consistently with
//!   [`PartialEq`], so it works as a `HashMap`/`HashSet` or `BTreeMap`/`BTreeSet` key out of the box.
//!
//! # Feature flags
//!
//! This module's optional integrations are off by default and purely additive. Enabling one never
//! changes the behavior of [`Lei::parse`] or the validation rules above:
//!
//! - **`serde`**: (de)serializes [`Lei`] as its 20-character string (e.g. `"5493000IBP32UQZ0KL24"`).
//!   Deserialization re-runs full validation, so an untrusted payload can never produce an invalid
//!   [`Lei`].
//! - **`schemars`**: implements `JsonSchema` for [`Lei`], describing it as a pattern-constrained
//!   string (`^[A-Z0-9]{18}[0-9]{2}$`). Implies `serde`.
//! - **`arbitrary`**: implements `Arbitrary` for [`Lei`], generating structurally valid,
//!   checksum-correct values for fuzz targets.
//! - **`proptest`**: exposes reusable `proptest` strategies (`valqeron_identifiers::lei::proptest`,
//!   when this feature is enabled) for generating checksum-valid [`Lei`] values.
//!
//! # Error handling
//!
//! Every fallible constructor returns [`LeiError`], which is `Clone + PartialEq + Eq` and
//! implements [`core::error::Error`] and [`core::fmt::Display`], so it composes with `?` and with
//! error-aggregation crates alike:
//!
//! ```
//! use valqeron_identifiers::{Lei, LeiError};
//!
//! match Lei::parse("5493000IBP32UQZ0KL25") {
//!     Ok(lei) => println!("valid: {lei}"),
//!     Err(LeiError::InvalidCheckDigits { expected, found }) => {
//!         println!("checksum mismatch: expected {expected:02}, found {found:02}");
//!     }
//!     Err(other) => println!("rejected: {other}"),
//! }
//! ```
//!
//! # Examples
//!
//! ```
//! use valqeron_identifiers::Lei;
//!
//! let bbc = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
//! assert_eq!(bbc.lou_prefix(), "5493");
//! assert_eq!(bbc.entity_id(), "000IBP32UQZ0KL");
//! assert_eq!(bbc.check_digits(), 24);
//! assert_eq!(bbc.as_str(), "5493000IBP32UQZ0KL24");
//! ```
//!
//! Sorting and deduplicating a batch of LEIs, e.g. after importing them from a spreadsheet:
//!
//! ```
//! use valqeron_identifiers::Lei;
//!
//! let mut leis: Vec<Lei> = ["213800WSGIIZCXF1P572", "5493000IBP32UQZ0KL24", "5493000IBP32UQZ0KL24"]
//!     .into_iter()
//!     .map(|s| Lei::parse(s).unwrap())
//!     .collect();
//! leis.sort();
//! leis.dedup();
//! assert_eq!(leis.len(), 2);
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

pub use error::{CharacterClass, LeiError};

use core::convert::TryFrom;
use core::str::{FromStr, from_utf8_unchecked};

/// A validated LEI (Legal Entity Identifier, ISO 17442).
///
/// `Lei` is a 20-byte, `Copy`, allocation-free value object. Once constructed, it is guaranteed to
/// satisfy the structural rules and the ISO/IEC 7064 MOD 97-10 check digits required by ISO 17442.
/// There is no way to obtain a `Lei` that hasn't passed validation.
///
/// Internally, the identifier is stored as raw uppercase ASCII bytes (`'0'...='9'` or `'A'...='Z'`).
///
/// # Constructing a `Lei`
///
/// | Constructor                    | Accepts                                          |
/// |--------------------------------|--------------------------------------------------|
/// | [`Lei::parse`] / [`Lei::new`]  | 20-character strings, any ASCII case, trimmed    |
/// | [`Lei::from_bytes`]            | Exactly 20 pre-normalized uppercase ASCII bytes  |
/// | [`FromStr`] / [`TryFrom<&str>`]| Same as `parse`, for use in generic code         |
///
/// All of them run the same validation and return [`LeiError`] on failure. See the
/// [module-level documentation](self) for the segment layout, checksum, and validation-policy rationale.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed Lei should be used; discarding it wastes the validation work"]
pub struct Lei {
    bytes: [u8; 20],
}

impl Lei {
    /// Parses an LEI from a string.
    ///
    /// The parser trims surrounding whitespace and folds ASCII letters to uppercase before
    /// validation. This is the primary constructor; [`Lei::new`], [`FromStr`], and
    /// [`TryFrom<&str>`] all delegate to it.
    ///
    /// # Errors
    ///
    /// Returns [`LeiError`] if the input is empty, does not contain exactly 20 characters after
    /// trimming, contains a character invalid for its position, or fails the ISO/IEC 7064 MOD 97-10
    /// check digits.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Lei;
    ///
    /// assert!(Lei::parse("5493000IBP32UQZ0KL24").is_ok());
    /// assert!(Lei::parse("5493000ibp32uqz0kl24").is_ok()); // lowercase is folded automatically
    /// assert!(Lei::parse(" 5493000IBP32UQZ0KL24 ").is_ok()); // surrounding whitespace is trimmed
    /// assert!(Lei::parse("5493000IBP32UQZ0KL25").is_err()); // wrong check digits
    /// ```
    pub fn parse(input: &str) -> Result<Self, LeiError> {
        let candidate = parser::normalize(input)?;
        Self::from_bytes(candidate)
    }

    /// Alias for [`Lei::parse`].
    ///
    /// # Errors
    ///
    /// See [`Lei::parse`].
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Lei;
    ///
    /// assert_eq!(Lei::new("5493000IBP32UQZ0KL24"), Lei::parse("5493000IBP32UQZ0KL24"));
    /// ```
    #[inline]
    pub fn new(input: &str) -> Result<Self, LeiError> {
        Self::parse(input)
    }

    /// Constructs a `Lei` directly from 20 raw ASCII bytes.
    ///
    /// Each byte must already be uppercase and valid for its position (eighteen alphanumerics, two
    /// digits). Use [`Lei::parse`] if the input might contain surrounding whitespace or lowercase
    /// letters.
    ///
    /// # Errors
    ///
    /// Returns [`LeiError`] under the same conditions as [`Lei::parse`], except that length is
    /// guaranteed by the `[u8; 20]` type itself: [`LeiError::InvalidLength`] cannot occur here.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Lei;
    ///
    /// let lei = Lei::from_bytes(*b"5493000IBP32UQZ0KL24").unwrap();
    /// assert_eq!(lei.as_str(), "5493000IBP32UQZ0KL24");
    ///
    /// // A malformed checksum is rejected just like it would be through `parse`.
    /// assert!(Lei::from_bytes(*b"5493000IBP32UQZ0KL25").is_err());
    /// ```
    pub fn from_bytes(bytes: [u8; 20]) -> Result<Self, LeiError> {
        validation::validate(&bytes)?;
        Ok(Lei { bytes })
    }

    /// Returns the 20 raw ASCII bytes backing this LEI (for example, `b"5493000IBP32UQZ0KL24"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Lei;
    ///
    /// let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
    /// assert_eq!(lei.as_bytes(), b"5493000IBP32UQZ0KL24");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.bytes
    }

    /// Returns the full 20-character LEI as a `&str`.
    ///
    /// This never allocates: the bytes are guaranteed to be valid ASCII by construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Lei;
    ///
    /// let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
    /// assert_eq!(lei.as_str(), "5493000IBP32UQZ0KL24");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `Lei::from_bytes` guarantees the bytes are ASCII letters and digits only.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }

    /// Returns the four-character LOU prefix (positions 1–4) identifying the issuing Local
    /// Operating Unit.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Lei;
    ///
    /// let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
    /// assert_eq!(lei.lou_prefix(), "5493");
    /// ```
    #[inline]
    #[must_use]
    pub fn lou_prefix(&self) -> &str {
        &self.as_str()[0..4]
    }

    /// Returns the fourteen-character entity-specific part (positions 5–18).
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Lei;
    ///
    /// let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
    /// assert_eq!(lei.entity_id(), "000IBP32UQZ0KL");
    /// ```
    #[inline]
    #[must_use]
    pub fn entity_id(&self) -> &str {
        &self.as_str()[4..18]
    }

    /// Returns the two check digits (positions 19–20) as a numeric value in `0...=99`.
    ///
    /// For a valid `Lei`, this always equals [`Lei::computed_check_digits`].
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Lei;
    ///
    /// let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
    /// assert_eq!(lei.check_digits(), 24);
    /// ```
    #[inline]
    #[must_use]
    pub fn check_digits(&self) -> u8 {
        (self.bytes[18] - b'0') * 10 + (self.bytes[19] - b'0')
    }

    /// Recomputes the check digits that the ISO/IEC 7064 MOD 97-10 algorithm produces from the
    /// first 18 characters of this value, as a numeric value in `0...=99`.
    ///
    /// For a valid `Lei` this always matches [`Lei::check_digits`]; the method exists so callers can
    /// reproduce the algorithm's output without a separate crate.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Lei;
    ///
    /// let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
    /// assert_eq!(lei.computed_check_digits(), lei.check_digits());
    /// ```
    #[inline]
    #[must_use]
    pub fn computed_check_digits(&self) -> u8 {
        validation::compute_check_digits(
            self.bytes[..18]
                .try_into()
                .expect("a Lei always has 18 base characters"),
        )
    }
}

impl FromStr for Lei {
    type Err = LeiError;

    /// Delegates to [`Lei::parse`], enabling `input.parse::<Lei>()` and use in generic code bounded
    /// by [`FromStr`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Lei {
    type Error = LeiError;

    /// Delegates to [`Lei::parse`], enabling `Lei::try_from(input)` and use in generic code bounded
    /// by [`TryFrom<&str>`].
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 20]> for Lei {
    type Error = LeiError;

    /// Delegates to [`Lei::from_bytes`]. The bytes must already be pre normalized uppercase ASCII.
    fn try_from(value: [u8; 20]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for Lei {
    type Error = LeiError;

    /// Validates a byte slice as an LEI. The slice must be exactly 20 pre normalized uppercase ASCII
    /// bytes; any other length yields [`LeiError::InvalidLength`]. Once the length is confirmed,
    /// this behaves like [`Lei::from_bytes`].
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 20] = value
            .try_into()
            .map_err(|_| LeiError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
    }
}

impl PartialEq<str> for Lei {
    /// Compares against a string slice by its canonical 20 character representation.
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Lei {
    /// Compares against a string slice by its canonical 20 character representation.
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Lei> for str {
    fn eq(&self, other: &Lei) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Lei> for &str {
    fn eq(&self, other: &Lei) -> bool {
        *self == other.as_str()
    }
}

impl AsRef<[u8]> for Lei {
    /// Equivalent to [`Lei::as_bytes`], borrowed as a slice.
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for Lei {
    /// Equivalent to [`Lei::as_str`].
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
