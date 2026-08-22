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

use arbitrary::{Arbitrary, Unstructured};
use core::convert::TryFrom;
use core::str::{FromStr, from_utf8_unchecked};
use proptest::collection;
use proptest::prelude::Strategy;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::borrow::Cow;
use std::fmt;

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
        let candidate = normalize(input)?;
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
        validate(&bytes)?;
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
        compute_check_digits(
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

/// The number of leading positions (LOU prefix + entity-specific part) that precede the two check
/// digits.
const BASE_LEN: usize = 18;


pub(crate) const ALPHANUMERIC: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Runs every validation rule against a normalized candidate, in order from cheapest/most-specific
/// to most-expensive:
/// 1. Character class per position (alphanumeric base, numeric check digits).
/// 2. ISO/IEC 7064 MOD 97-10 check digits.
fn validate(candidate: &[u8; 20]) -> Result<(), LeiError> {
    validate_character_classes(candidate)?;
    validate_check_digits(candidate)?;
    Ok(())
}

fn validate_character_classes(candidate: &[u8; 20]) -> Result<(), LeiError> {
    for (i, &byte) in candidate.iter().enumerate() {
        let (is_valid, expected) = if i < BASE_LEN {
            (
                byte.is_ascii_digit() || byte.is_ascii_uppercase(),
                CharacterClass::Alphanumeric,
            )
        } else {
            (byte.is_ascii_digit(), CharacterClass::Digit)
        };

        if !is_valid {
            return Err(LeiError::InvalidCharacter {
                character: byte as char,
                position: (i + 1) as u8,
                expected,
            });
        }
    }
    Ok(())
}

fn validate_check_digits(candidate: &[u8; 20]) -> Result<(), LeiError> {
    let expected = compute_check_digits(
        candidate[..BASE_LEN]
            .try_into()
            .expect("BASE_LEN bytes precede the check digits"),
    );
    // Character-class validation above guarantees the final two bytes are ASCII digits.
    let found = (candidate[BASE_LEN] - b'0') * 10 + (candidate[BASE_LEN + 1] - b'0');
    if expected != found {
        return Err(LeiError::InvalidCheckDigits { expected, found });
    }
    Ok(())
}

/// Computes the two ISO/IEC 7064 MOD 97-10 check digits for a well-formed 18-character base
/// segment (each byte already `'0'...='9'` or `'A'...='Z'`), returned as a single value in
/// `0...=99`.
///
/// Appends the placeholder `"00"` to the base, folds the expanded integer modulo 97, and returns
/// `98 - (n mod 97)`.
///
/// Also used by the [`super::arbitrary`] and [`super::proptest`] generators (behind their features)
/// to produce checksum-correct values without duplicating the algorithm, hence the `allow`.
fn compute_check_digits(base: &[u8; BASE_LEN]) -> u8 {
    // Fold the base, then the two placeholder '0' characters, modulo 97.
    let mut rem = fold_mod_97(0, base);
    rem = (rem * 100) % 97; // equivalent to folding "00"
    (98 - rem) as u8
}

/// Folds a slice of already-validated ASCII bytes into an existing MOD 97 remainder.
///
/// A digit advances the running value by one decimal place (`rem * 10 + d`); a letter expands to
/// its two-digit ordinal and advances by two places (`rem * 100 + value`). The remainder is reduced
/// modulo 97 at every step so it never leaves `0...97`.
#[inline]
fn fold_mod_97(mut rem: u32, bytes: &[u8]) -> u32 {
    for &c in bytes {
        if c.is_ascii_digit() {
            rem = (rem * 10 + (c - b'0') as u32) % 97;
        } else {
            // 'A' => 10, ..., 'Z' => 35.
            let value = (c - b'A' + 10) as u32;
            rem = (rem * 100 + value) % 97;
        }
    }
    rem
}

/// Computes the full ISO/IEC 7064 MOD 97-10 residue of all 20 characters. A conforming LEI yields
/// `1`. Exposed to sibling test modules for cross-checking; `validate` itself goes through
/// [`compute_check_digits`] so the acceptance and generation paths share one implementation.
#[cfg(test)]
fn residue(candidate: &[u8; 20]) -> u32 {
    fold_mod_97(0, candidate)
}

/// Builds a structurally valid LEI candidate from alphabet indices, then appends the matching two
/// check digits.
///
/// This keeps the generator-specific code focused on randomness while centralizing the shape of a
/// valid LEI: eighteen alphanumeric base characters and two checksum digits.
fn build_valid_lei_bytes(base_indices: &[usize; BASE_LEN]) -> [u8; 20] {
    let mut base = [0u8; BASE_LEN];
    for (slot, idx) in base.iter_mut().zip(base_indices) {
        *slot = ALPHANUMERIC[*idx];
    }

    let check = compute_check_digits(&base);
    let mut bytes = [0u8; 20];
    bytes[..BASE_LEN].copy_from_slice(&base);
    bytes[BASE_LEN] = b'0' + check / 10;
    bytes[BASE_LEN + 1] = b'0' + check % 10;
    bytes
}

/// The class of characters permitted at a given position of an LEI.
/// Reported by [`LeiError::InvalidCharacter`] to describe what was expected where an invalid
/// character was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharacterClass {
    /// An ASCII digit, `'0'...='9'` (the two check digits at positions 19-20).
    Digit,
    /// An ASCII digit or an uppercase ASCII letter, `'0'...='9' | 'A'...='Z'` (positions 1-18: the
    /// LOU prefix and the entity-specific part).
    Alphanumeric,
}

impl fmt::Display for CharacterClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CharacterClass::Digit => write!(f, "a digit (0-9)"),
            CharacterClass::Alphanumeric => {
                write!(f, "a digit (0-9) or an uppercase letter (A-Z)")
            }
        }
    }
}

/// The set of reasons an LEI string can fail validation.
/// Every fallible constructor of [`Lei`](super::Lei) returns this type; each variant maps to a
/// single, specific failure so callers can react programmatically (for example, highlighting the
/// offending character in a form field) rather than parsing a human-readable message.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeiError {
    /// The input was an empty string.
    Empty,

    /// After trimming surrounding whitespace, the input did not contain exactly 20 characters.
    InvalidLength {
        /// The number of characters found after trimming.
        found: usize,
    },

    /// A character outside the allowed set was found at a given position.
    InvalidCharacter {
        /// The offending character, as originally provided (before case folding).
        character: char,
        /// 1-indexed position within the 20 characters.
        position: u8,
        /// The character class that was expected at this position.
        expected: CharacterClass,
    },

    /// The two check digits (positions 19-20) did not satisfy the ISO/IEC 7064 MOD 97-10 checksum
    /// required by ISO 17442.
    ///
    /// `expected` is the two-digit value the algorithm derives from the first 18 characters;
    /// `found` is the value actually present in the input. Both are in the range `0...=99`.
    InvalidCheckDigits {
        /// The check-digit value computed by the ISO/IEC 7064 MOD 97-10 algorithm (`0...=99`).
        expected: u8,
        /// The check-digit value actually present in the input (`0...=99`).
        found: u8,
    },
}

impl fmt::Display for LeiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeiError::Empty => f.write_str("LEI input is empty"),
            LeiError::InvalidLength { found } => {
                write!(f, "LEI must contain exactly 20 characters, found {found}")
            }
            LeiError::InvalidCharacter {
                character,
                position,
                expected,
            } => write!(
                f,
                "invalid character '{character}' at position {position} of 20: expected {expected}"
            ),
            LeiError::InvalidCheckDigits { expected, found } => write!(
                f,
                "invalid check digits at positions 19-20 of 20: expected {expected:02}, found {found:02}"
            ),
        }
    }
}

impl core::error::Error for LeiError {}

impl fmt::Display for Lei {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Lei {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Lei").field(&self.as_str()).finish()
    }
}

/// Normalizes `input` into a 20-byte ASCII array.
///
/// - Empty input is rejected as [`LeiError::Empty`].
/// - Leading and trailing whitespace is trimmed; interior characters are left untouched.
/// - Remaining characters are ASCII-uppercased (so a lowercase LEI is accepted transparently).
/// - Any non-ASCII character, or a character count other than 20 after trimming, is rejected.
///
/// This function does **not** check that each position holds a character valid for that position
/// (alphanumeric vs. digit); see [`super::validation::validate`] for that.
fn normalize(input: &str) -> Result<[u8; 20], LeiError> {
    if input.is_empty() {
        return Err(LeiError::Empty);
    }

    let trimmed = input.trim();
    let found = trimmed.chars().count();
    if found != 20 {
        return Err(LeiError::InvalidLength { found });
    }

    let mut buf = [0u8; 20];
    for (i, ch) in trimmed.chars().enumerate() {
        if !ch.is_ascii() {
            let expected = if i < 18 {
                CharacterClass::Alphanumeric
            } else {
                CharacterClass::Digit
            };

            return Err(LeiError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
                expected,
            });
        }
        buf[i] = ch.to_ascii_uppercase() as u8;
    }

    Ok(buf)
}

impl<'a> Arbitrary<'a> for Lei {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Positions 1-18: alphanumeric LOU prefix + entity-specific part.
        let mut base = [0usize; BASE_LEN];
        for slot in &mut base {
            *slot = u.arbitrary::<u8>()? as usize % ALPHANUMERIC.len();
        }

        let bytes = build_valid_lei_bytes(&base);

        Lei::from_bytes(bytes).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

/// A strategy producing structurally valid, checksum-correct [`Lei`] values: eighteen alphanumeric
/// base characters and two matching ISO/IEC 7064 MOD 97-10 check digits.
pub fn valid_lei() -> impl Strategy<Value=Lei> {
    collection::vec(0..ALPHANUMERIC.len(), BASE_LEN).prop_map(|base| {
        let mut indices = [0usize; BASE_LEN];
        indices.copy_from_slice(&base);
        let bytes = build_valid_lei_bytes(&indices);
        Lei::from_bytes(bytes).expect("generated candidate is checksum-valid by construction")
    })
}

/// A strategy producing a valid [`Lei`] rendered as its canonical 20-character `String`, useful for
/// round-trip-through-parsing property tests.
pub fn valid_lei_string() -> impl Strategy<Value=String> {
    valid_lei().prop_map(|lei| lei.as_str().to_string())
}

impl JsonSchema for Lei {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Lei")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "lei",
            "minLength": 20,
            "maxLength": 20,
            "pattern": "^[A-Z0-9]{18}[0-9]{2}$",
            "description": "LEI (Legal Entity Identifier, ISO 17442), ISO/IEC 7064 MOD 97-10 checksum-valid."
        })
    }
}

impl Serialize for Lei {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct LeiVisitor;

impl<'de> Visitor<'de> for LeiVisitor {
    type Value = Lei;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 20-character LEI (ISO 17442), e.g. 5493000IBP32UQZ0KL24")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Lei::parse(v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Lei {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(LeiVisitor)
    }
}

#[cfg(test)]
mod tests_serde {
    use crate::identifiers::lei::Lei;

    #[test]
    fn round_trips_through_json() {
        let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
        let json = serde_json::to_string(&lei).unwrap();
        assert_eq!(json, "\"5493000IBP32UQZ0KL24\"");
        let back: Lei = serde_json::from_str(&json).unwrap();
        assert_eq!(lei, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<Lei>("\"not-an-lei\"").unwrap_err();
        assert!(err.to_string().contains("LEI"));
    }
}

#[cfg(test)]
mod tests_schemars {
    use crate::identifiers::lei::Lei;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(Lei);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "lei");
        assert_eq!(json["minLength"], 20);
        assert_eq!(json["maxLength"], 20);
        assert_eq!(json["pattern"], "^[A-Z0-9]{18}[0-9]{2}$");
    }
}

#[cfg(test)]
mod tests_proptest {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_lei_always_round_trips_through_parse(lei in valid_lei()) {
            let reparsed = Lei::parse(lei.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(lei, reparsed.unwrap());
        }

        #[test]
        fn valid_lei_string_always_parses(s in valid_lei_string()) {
            prop_assert!(Lei::parse(&s).is_ok());
        }
    }
}

#[cfg(test)]
mod tests_arbitrary {
    use super::*;

    #[test]
    fn always_produces_valid_leis() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let lei = Lei::arbitrary(&mut u).expect("arbitrary should always succeed");
            // Re-validating via parse() proves the value round-trips through the exact same
            // checks a hand-typed input would.
            assert!(Lei::parse(lei.as_str()).is_ok());
        }
    }
}

#[cfg(test)]
mod tests_parser {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize(""), Err(LeiError::Empty));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(
            normalize("  5493000IBP32UQZ0KL24 "),
            normalize("5493000IBP32UQZ0KL24")
        );
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(
            normalize("5493000ibp32uqz0kl24").unwrap(),
            *b"5493000IBP32UQZ0KL24"
        );
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            normalize("5493000IBP32UQZ0KL2"),
            Err(LeiError::InvalidLength { found: 19 })
        );
    }

    #[test]
    fn whitespace_only_is_a_length_error() {
        assert_eq!(normalize("   "), Err(LeiError::InvalidLength { found: 0 }));
    }

    #[test]
    fn keeps_interior_characters_for_validation() {
        // An interior space survives normalization (count is still 20) and is left for
        // `validation` to reject as a non-alphanumeric character.
        assert_eq!(
            normalize("5493000IBP32UQZ0K 24"),
            Ok(*b"5493000IBP32UQZ0K 24")
        );
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("5493000IBP32UQZ0KL2£").unwrap_err();
        assert!(matches!(
            err,
            LeiError::InvalidCharacter {
                character: '£', ..
            }
        ));
    }
}

#[cfg(test)]
mod tests_formating {
    use crate::identifiers::lei::Lei;
    use std::format;
    use std::string::ToString;

    #[test]
    fn display_is_the_canonical_string() {
        let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
        assert_eq!(lei.to_string(), "5493000IBP32UQZ0KL24");
    }

    #[test]
    fn debug_is_readable() {
        let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
        assert_eq!(format!("{lei:?}"), "Lei(\"5493000IBP32UQZ0KL24\")");
    }
}

#[cfg(test)]
mod tests_validation {
    use super::*;

    fn candidate(s: &str) -> [u8; 20] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 20];
        out.copy_from_slice(bytes);
        out
    }

    /// A second, deliberately naive MOD 97-10 implementation used to cross-check
    /// [`compute_check_digits`]: it materializes the full expanded decimal string and reduces it
    /// digit by digit, independent of the folding in `fold_mod_97`.
    fn reference_check_digits(base: &str) -> u8 {
        let mut expanded = std::string::String::new();
        for &c in base.as_bytes() {
            if c.is_ascii_digit() {
                expanded.push(c as char);
            } else {
                let v = c - b'A' + 10;
                expanded.push((b'0' + v / 10) as char);
                expanded.push((b'0' + v % 10) as char);
            }
        }
        expanded.push('0');
        expanded.push('0');

        let mut rem = 0u32;
        for ch in expanded.chars() {
            rem = (rem * 10 + ch.to_digit(10).unwrap()) % 97;
        }
        (98 - rem) as u8
    }

    #[test]
    fn accepts_known_real_world_leis() {
        // Each verified to satisfy `residue == 1` before being committed here.
        for s in [
            "5493000IBP32UQZ0KL24", // British Broadcasting Corporation
            "213800WSGIIZCXF1P572", // Jaguar Land Rover Ltd
            "506700GE1G29325QX363", // GLEIF itself
            "54930084UKLVMY22DS16", // G.E. Financing GmbH
        ] {
            assert!(validate(&candidate(s)).is_ok(), "{s} should be valid");
            assert_eq!(residue(&candidate(s)), 1, "{s} residue must be 1");
        }
    }

    #[test]
    fn computes_documented_check_digits() {
        assert_eq!(compute_check_digits(b"5493000IBP32UQZ0KL"), 24);
        assert_eq!(compute_check_digits(b"213800WSGIIZCXF1P5"), 72);
        assert_eq!(compute_check_digits(b"506700GE1G29325QX3"), 63);
        assert_eq!(compute_check_digits(b"54930084UKLVMY22DS"), 16);
    }

    #[test]
    fn folding_matches_the_reference_implementation() {
        for base in [
            "5493000IBP32UQZ0KL",
            "213800WSGIIZCXF1P5",
            "506700GE1G29325QX3",
            "54930084UKLVMY22DS",
            "000000000000000000",
            "ZZZZZZZZZZZZZZZZZZ",
        ] {
            assert_eq!(
                compute_check_digits(base.as_bytes().try_into().unwrap()),
                reference_check_digits(base),
                "{base}"
            );
        }
    }

    #[test]
    fn rejects_lowercase_in_base() {
        let err = validate(&candidate("5493000ibp32UQZ0KL24")).unwrap_err();
        assert_eq!(
            err,
            LeiError::InvalidCharacter {
                character: 'i',
                position: 8,
                expected: CharacterClass::Alphanumeric,
            }
        );
    }

    #[test]
    fn rejects_letter_in_check_digit_position() {
        let err = validate(&candidate("5493000IBP32UQZ0KLX4")).unwrap_err();
        assert_eq!(
            err,
            LeiError::InvalidCharacter {
                character: 'X',
                position: 19,
                expected: CharacterClass::Digit,
            }
        );
    }

    #[test]
    fn rejects_wrong_check_digits() {
        let err = validate(&candidate("5493000IBP32UQZ0KL25")).unwrap_err();
        assert_eq!(
            err,
            LeiError::InvalidCheckDigits {
                expected: 24,
                found: 25,
            }
        );
    }

    #[test]
    fn rejects_residue_one_with_reserved_check_digits() {
        for (s, expected, found) in [
            ("PRKYQO9OOQ90FWGOFC00", 97u8, 0u8),
            ("TS43UAPFUU97VO4FE001", 98, 1),
            ("2MZDL7DS67LXXZ93H099", 2, 99),
        ] {
            assert_eq!(residue(&candidate(s)), 1, "{s} residue must be 1");
            assert_eq!(
                validate(&candidate(s)),
                Err(LeiError::InvalidCheckDigits { expected, found }),
                "{s} must be rejected despite residue 1"
            );
        }
    }

    #[test]
    fn rejects_adjacent_transposition() {
        // MOD 97-10 catches all single adjacent transpositions of unequal characters.
        assert!(validate(&candidate("5493000IBP32UQ0ZKL24")).is_err());
    }
}
