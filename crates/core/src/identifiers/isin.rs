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
//! - **Zero allocation, `Copy`, allocation-free.** [`Isin`] is a 12-byte value type wrapping
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
//! - **`proptest`** — exposes reusable `proptest` strategies (`valqeron_identifiers::isin::proptest`,
//!   when this feature is enabled) for generating checksum-valid [`Isin`] values.
//!
//! # Error handling
//!
//! Every fallible constructor returns [`IsinError`], which is `Clone + PartialEq + Eq` and
//! implements [`core::error::Error`] and [`core::fmt::Display`], so it composes with `?` and with
//! error-aggregation crates alike:
//!
//! ```
//! use valqeron_identifiers::{Isin, IsinError};
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
//! use valqeron_identifiers::Isin;
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
//! use valqeron_identifiers::Isin;
//!
//! let mut isins: Vec<Isin> = ["US0231351067", "US0378331005", "US0378331005"]
//!     .into_iter()
//!     .map(|s| Isin::parse(s).unwrap())
//!     .collect();
//! isins.sort();
//! isins.dedup();
//! assert_eq!(isins.len(), 2);
//! ```

use crate::identifiers::CountryCode;
use arbitrary::{Arbitrary, Unstructured};
use proptest::prelude::{Strategy, prop};
use schemars::JsonSchema;
use schemars::{Schema, SchemaGenerator, json_schema};
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::borrow::Cow;
use std::fmt;
use std::str::{FromStr, from_utf8_unchecked};

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
    /// use valqeron_identifiers::Isin;
    ///
    /// assert!(Isin::parse("US0378331005").is_ok());
    /// assert!(Isin::parse("us0378331005").is_ok()); // lowercase is folded automatically
    /// assert!(Isin::parse(" US0378331005 ").is_ok()); // surrounding whitespace is trimmed
    /// assert!(Isin::parse("US0378331006").is_err()); // wrong check digit
    /// ```
    pub fn parse(input: &str) -> Result<Self, IsinError> {
        let candidate = normalize(input)?;
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
    /// use valqeron_identifiers::Isin;
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
    /// use valqeron_identifiers::Isin;
    ///
    /// let isin = Isin::from_bytes(*b"US0378331005").unwrap();
    /// assert_eq!(isin.as_str(), "US0378331005");
    ///
    /// // A malformed checksum is rejected just like it would be through `parse`.
    /// assert!(Isin::from_bytes(*b"US0378331006").is_err());
    /// ```
    pub fn from_bytes(bytes: [u8; 12]) -> Result<Self, IsinError> {
        validate(&bytes)?;
        Ok(Isin { bytes })
    }

    /// Returns the 12 raw ASCII bytes backing this ISIN (for example, `b"US0378331005"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Isin;
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
    /// use valqeron_identifiers::Isin;
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
    /// use valqeron_identifiers::Isin;
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
    /// use valqeron_identifiers::{Isin, CountryCode};
    ///
    /// let apple = Isin::parse("US0378331005").unwrap();
    /// assert_eq!(apple.country(), Some(CountryCode::parse("US").unwrap()));
    /// ```
    #[inline]
    #[must_use]
    pub fn country(&self) -> Option<CountryCode> {
        CountryCode::from_bytes([self.bytes[0], self.bytes[1]]).ok()
    }

    /// Returns the nine-character National Securities Identifying Number (positions 3–11).
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Isin;
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
    /// use valqeron_identifiers::Isin;
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
    /// use valqeron_identifiers::Isin;
    ///
    /// let isin = Isin::parse("US0378331005").unwrap();
    /// assert_eq!(isin.computed_check_digit(), isin.check_digit());
    /// ```
    #[inline]
    #[must_use]
    pub fn computed_check_digit(&self) -> u8 {
        compute_check_digit(&self.bytes[..11])
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

impl fmt::Display for Isin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Isin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Isin").field(&self.as_str()).finish()
    }
}

/// The number of positions occupied by the country code plus the NSIN (everything except the
/// trailing check digit).
const BASE_LEN: usize = 11;
const LETTERS: &[u8; 26] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const ALPHANUMERIC: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Runs every validation rule against a normalized candidate, in order from cheapest/most-specific
/// to most-expensive:
/// 1. Character class per position (two-letter country code, alphanumeric NSIN, numeric check digit).
/// 2. Luhn (ISO 6166 Annex C) check digit.
fn validate(candidate: &[u8; 12]) -> Result<(), IsinError> {
    validate_character_classes(candidate)?;
    validate_check_digit(candidate)?;
    Ok(())
}

fn validate_character_classes(candidate: &[u8; 12]) -> Result<(), IsinError> {
    for (i, &byte) in candidate.iter().enumerate() {
        let (is_valid, expected) = if i < 2 {
            (byte.is_ascii_uppercase(), CharacterClass::Letter)
        } else if i < BASE_LEN {
            (
                byte.is_ascii_digit() || byte.is_ascii_uppercase(),
                CharacterClass::Alphanumeric,
            )
        } else {
            (byte.is_ascii_digit(), CharacterClass::Digit)
        };

        if !is_valid {
            return Err(IsinError::InvalidCharacter {
                character: byte as char,
                position: (i + 1) as u8,
                expected,
            });
        }
    }
    Ok(())
}

fn validate_check_digit(candidate: &[u8; 12]) -> Result<(), IsinError> {
    let expected = compute_check_digit(&candidate[..BASE_LEN]);
    // Character-class validation above guarantees `candidate[11]` is an ASCII digit.
    let found = candidate[BASE_LEN] - b'0';
    if expected != found {
        return Err(IsinError::InvalidCheckDigit { expected, found });
    }
    Ok(())
}

/// Computes the ISO 6166 Luhn check digit for the first 11 characters of an ISIN.
///
/// Walks the base right-to-left so expansion and doubling happen in a single allocation-free pass:
/// for a letter, its units digit is the right-most of the pair and is therefore processed (and
/// toggles the doubling flag) before its tens digit. The rightmost expanded digit starts in a
/// doubled position, because the check digit this function returns will occupy the units position
/// once appended.
///
/// Also used by the [`super::arbitrary`] and [`super::proptest`] generators (behind their features)
/// to produce checksum-correct values without duplicating the algorithm, hence the `allow`.
fn compute_check_digit(base: &[u8]) -> u8 {
    debug_assert_eq!(base.len(), BASE_LEN);

    let mut sum = 0u32;
    let mut double = true;

    for &c in base.iter().rev() {
        if c.is_ascii_digit() {
            sum += luhn_step((c - b'0') as u32, double);
            double = !double;
        } else {
            // 'A' => 10, ..., 'Z' => 35; split into tens and units.
            let value = (c - b'A' + 10) as u32;
            // Units is the rightmost digit of the expanded pair, so it is processed first.
            sum += luhn_step(value % 10, double);
            double = !double;
            sum += luhn_step(value / 10, double);
            double = !double;
        }
    }

    ((10 - (sum % 10)) % 10) as u8
}

/// Builds a structurally valid ISIN candidate from alphabet indices, then appends the matching
/// check digit.
///
/// This keeps the generator-specific code focused on randomness while centralizing the shape of a
/// valid ISIN: two country-code letters, nine alphanumeric NSIN characters, and one checksum
/// digit.
fn build_valid_isin_bytes(country: [usize; 2], nsin: &[usize]) -> [u8; 12] {
    debug_assert_eq!(nsin.len(), BASE_LEN - 2);

    let mut base = [0u8; BASE_LEN];
    base[0] = LETTERS[country[0]];
    base[1] = LETTERS[country[1]];
    for (slot, idx) in base[2..].iter_mut().zip(nsin) {
        *slot = ALPHANUMERIC[*idx];
    }

    let check = compute_check_digit(&base);
    let mut bytes = [0u8; 12];
    bytes[..BASE_LEN].copy_from_slice(&base);
    bytes[BASE_LEN] = check + b'0';
    bytes
}

// ================================= ERRORS =================================
/// The class of characters permitted at a given position of an ISIN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharacterClass {
    /// An ASCII digit, `'0'...='9'` (the check digit at position 12).
    Digit,
    /// An uppercase ASCII letter, `'A'...='Z'` (the two-character country code).
    Letter,
    /// An ASCII digit or an uppercase ASCII letter, `'0'...='9' | 'A'...='Z'` (the NSIN body).
    Alphanumeric,
}

impl fmt::Display for CharacterClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CharacterClass::Digit => write!(f, "a digit (0-9)"),
            CharacterClass::Letter => write!(f, "an uppercase letter (A-Z)"),
            CharacterClass::Alphanumeric => {
                write!(f, "a digit (0-9) or an uppercase letter (A-Z)")
            }
        }
    }
}

/// The set of reasons an ISIN string can fail validation.
///
/// Every fallible constructor of [`Isin`](super::Isin) returns this type; each variant maps to a
/// single, specific failure so callers can react programmatically (for example, highlighting the
/// offending character in a form field) rather than parsing a human-readable message.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsinError {
    /// The input was an empty string.
    Empty,

    /// After trimming surrounding whitespace, the input did not contain exactly 12 characters.
    InvalidLength {
        /// The number of characters found after trimming.
        found: usize,
    },

    /// A character outside the allowed set was found at a given position.
    InvalidCharacter {
        /// The offending character, as originally provided (before case folding).
        character: char,
        /// 1-indexed position within the 12 characters.
        position: u8,
        /// The character class that was expected at this position.
        expected: CharacterClass,
    },

    /// The Luhn check digit (position 12) did not match the value computed from the first 11
    /// characters.
    InvalidCheckDigit {
        /// The check digit computed by the ISO 6166 Luhn algorithm.
        expected: u8,
        /// The check digit actually present in the input.
        found: u8,
    },
}

impl fmt::Display for IsinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IsinError::Empty => f.write_str("ISIN input is empty"),
            IsinError::InvalidLength { found } => {
                write!(f, "ISIN must contain exactly 12 characters, found {found}")
            }
            IsinError::InvalidCharacter {
                character,
                position,
                expected,
            } => write!(
                f,
                "invalid character '{character}' at position {position} of 12: expected {expected}"
            ),
            IsinError::InvalidCheckDigit { expected, found } => write!(
                f,
                "invalid check digit at position 12 of 12: expected {expected}, found {found}"
            ),
        }
    }
}

impl core::error::Error for IsinError {}

// ================================= PARSER =================================
/// Normalizes `input` into a 12-byte ASCII array.
///
/// - Empty input is rejected as [`IsinError::Empty`].
/// - Leading and trailing whitespace is trimmed; interior characters are left untouched.
/// - Remaining characters are ASCII-uppercased (so a lowercase ISIN is accepted transparently).
/// - Any non-ASCII character, or a character count other than 12 after trimming, is rejected.
///
/// This function does **not** check that each position holds a character valid for that position
/// (letter vs. digit vs. alphanumeric); see [`super::validation::validate`] for that.
fn normalize(input: &str) -> Result<[u8; 12], IsinError> {
    if input.is_empty() {
        return Err(IsinError::Empty);
    }

    let trimmed = input.trim();
    let found = trimmed.chars().count();
    if found != 12 {
        return Err(IsinError::InvalidLength { found });
    }

    let mut buf = [0u8; 12];
    for (i, ch) in trimmed.chars().enumerate() {
        if !ch.is_ascii() {
            let expected = match i {
                0 | 1 => CharacterClass::Letter,
                2..=10 => CharacterClass::Alphanumeric,
                11 => CharacterClass::Digit,
                _ => unreachable!(),
            };
            return Err(IsinError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
                expected,
            });
        }
        buf[i] = ch.to_ascii_uppercase() as u8;
    }

    Ok(buf)
}

// ================================= SERDE =================================
impl Serialize for Isin {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct IsinVisitor;

impl<'de> Visitor<'de> for IsinVisitor {
    type Value = Isin;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 12-character ISIN (ISO 6166), e.g. US0378331005")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Isin::parse(v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Isin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(IsinVisitor)
    }
}

// ================================= SCHEMARS, PROPTEST, ARBITRARY =================================
impl<'a> Arbitrary<'a> for Isin {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Positions 1-2: Generate a valid CountryCode first to ensure
        // we only use assigned ISO 3166-1 alpha-2 prefixes.
        let country_code = u.arbitrary::<CountryCode>()?;
        let cc_bytes = country_code.as_bytes();

        // Map the selected country code's bytes back into LETTERS indices.
        let country = [
            LETTERS
                .iter()
                .position(|&b| b == cc_bytes[0])
                .expect("valid letter"),
            LETTERS
                .iter()
                .position(|&b| b == cc_bytes[1])
                .expect("valid letter"),
        ];

        // Positions 3-11: alphanumeric NSIN.
        let mut nsin = [0usize; BASE_LEN - 2];
        for slot in &mut nsin {
            *slot = u.arbitrary::<u8>()? as usize % ALPHANUMERIC.len();
        }

        let bytes = build_valid_isin_bytes(country, &nsin);

        Isin::from_bytes(bytes).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

/// A strategy producing structurally valid, checksum-correct [`Isin`] values: a two-letter country
/// code, a nine-character alphanumeric NSIN, and a matching Luhn check digit.
fn valid_isin() -> impl Strategy<Value=Isin> {
    (
        prop::collection::vec(0..LETTERS.len(), 2),
        prop::collection::vec(0..ALPHANUMERIC.len(), BASE_LEN - 2),
    )
        .prop_map(|(country, nsin)| {
            let bytes = build_valid_isin_bytes([country[0], country[1]], &nsin);
            Isin::from_bytes(bytes).expect("generated candidate is checksum-valid by construction")
        })
}

/// A strategy producing a valid [`Isin`] rendered as its canonical 12-character `String`, useful
/// for round-trip-through-parsing property tests.
fn valid_isin_string() -> impl Strategy<Value=String> {
    valid_isin().prop_map(|isin| isin.as_str().to_string())
}

/// Applies the Luhn per-digit rule: double when in a doubled position, then cast the two-digit
/// product back to a single digit by subtracting 9.
#[inline]
fn luhn_step(value: u32, double: bool) -> u32 {
    if double {
        let doubled = value * 2;
        if doubled > 9 { doubled - 9 } else { doubled }
    } else {
        value
    }
}

impl JsonSchema for Isin {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Isin")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "isin",
            "minLength": 12,
            "maxLength": 12,
            "pattern": "^[A-Z]{2}[A-Z0-9]{9}[0-9]$",
            "description": "ISIN (International Securities Identification Number, ISO 6166), Luhn checksum-valid."
        })
    }
}

#[cfg(test)]
mod tests_formating {
    use crate::identifiers::isin::Isin;
    use std::format;
    use std::string::ToString;

    #[test]
    fn display_is_the_canonical_string() {
        let isin = Isin::parse("US0378331005").unwrap();
        assert_eq!(isin.to_string(), "US0378331005");
    }

    #[test]
    fn debug_is_readable() {
        let isin = Isin::parse("US0378331005").unwrap();
        assert_eq!(format!("{isin:?}"), "Isin(\"US0378331005\")");
    }
}

#[cfg(test)]
mod tests_validation {
    use super::*;

    fn candidate(s: &str) -> [u8; 12] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 12];
        out.copy_from_slice(bytes);
        out
    }

    /// A second, deliberately naive Luhn implementation used to cross-check
    /// [`compute_check_digit`]: it materializes the full expanded digit buffer instead of doing a
    /// single reverse pass.
    fn reference_check_digit(base: &str) -> u8 {
        let mut digits = std::vec::Vec::new();
        for &c in base.as_bytes() {
            if c.is_ascii_digit() {
                digits.push((c - b'0') as u32);
            } else {
                let v = (c - b'A' + 10) as u32;
                digits.push(v / 10);
                digits.push(v % 10);
            }
        }
        // The check digit will be appended to the right, so the rightmost base digit is doubled.
        let mut sum = 0u32;
        let n = digits.len();
        for (i, &d) in digits.iter().enumerate() {
            let from_right = n - i; // 1-based position of this digit once the check digit exists
            let mut v = d;
            if from_right % 2 == 1 {
                v *= 2;
                if v > 9 {
                    v -= 9;
                }
            }
            sum += v;
        }
        ((10 - (sum % 10)) % 10) as u8
    }

    #[test]
    fn accepts_known_real_world_isins() {
        for s in [
            "US0378331005", // Apple
            "US0231351067", // Amazon
            "BRPETRACNOR9", // Petrobras ON
            "GB0002634946", // UK gilt
            "DE0001102333", // German Bund
            "JP3633400001", // Japanese equity
            "AU000000BHP4", // BHP
            "CH0012221716", // Nestlé
        ] {
            assert!(validate(&candidate(s)).is_ok(), "{s} should be valid");
        }
    }

    #[test]
    fn computes_the_documented_apple_check_digit() {
        assert_eq!(compute_check_digit(b"US037833100"), 5);
    }

    #[test]
    fn computes_an_all_letter_nsin_check_digit() {
        assert_eq!(compute_check_digit(b"BRPETRACNOR"), 9);
    }

    #[test]
    fn single_pass_matches_the_reference_implementation() {
        for base in [
            "US037833100",
            "US023135106",
            "BRPETRACNOR",
            "GB000263494",
            "AU000000BHP",
            "AA000000000",
            "ZZZZZZZZZZZ",
        ] {
            assert_eq!(
                compute_check_digit(base.as_bytes()),
                reference_check_digit(base),
                "{base}"
            );
        }
    }

    #[test]
    fn rejects_lowercase_country_code() {
        let err = validate(&candidate("uS0378331005")).unwrap_err();
        assert_eq!(
            err,
            IsinError::InvalidCharacter {
                character: 'u',
                position: 1,
                expected: CharacterClass::Letter,
            }
        );
    }

    #[test]
    fn rejects_digit_in_country_code() {
        let err = validate(&candidate("1S0378331005")).unwrap_err();
        assert_eq!(
            err,
            IsinError::InvalidCharacter {
                character: '1',
                position: 1,
                expected: CharacterClass::Letter,
            }
        );
    }

    #[test]
    fn rejects_letter_in_check_digit_position() {
        let err = validate(&candidate("US037833100X")).unwrap_err();
        assert_eq!(
            err,
            IsinError::InvalidCharacter {
                character: 'X',
                position: 12,
                expected: CharacterClass::Digit,
            }
        );
    }

    #[test]
    fn rejects_wrong_check_digit() {
        let err = validate(&candidate("US0378331006")).unwrap_err();
        assert_eq!(
            err,
            IsinError::InvalidCheckDigit {
                expected: 5,
                found: 6,
            }
        );
    }

    #[test]
    fn rejects_adjacent_transposition() {
        // Luhn catches most single adjacent transpositions.
        assert!(validate(&candidate("US3078331005")).is_err());
    }
}

#[cfg(test)]
mod tests_parser {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize(""), Err(IsinError::Empty));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(normalize("  US0378331005 "), normalize("US0378331005"));
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(normalize("us0378331005").unwrap(), *b"US0378331005");
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            normalize("US037833100"),
            Err(IsinError::InvalidLength { found: 11 })
        );
    }

    #[test]
    fn whitespace_only_is_a_length_error() {
        assert_eq!(normalize("   "), Err(IsinError::InvalidLength { found: 0 }));
    }

    #[test]
    fn keeps_interior_characters_for_validation() {
        // An interior space survives normalization (count is still 12) and is left for
        // `validation` to reject as a non-alphanumeric character.
        assert_eq!(normalize("US 378331005").unwrap(), *b"US 378331005");
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("US03783310£5").unwrap_err();
        assert!(matches!(
            err,
            IsinError::InvalidCharacter {
                character: '£',
                position: 11,
                expected: CharacterClass::Alphanumeric,
            }
        ));
    }

    #[test]
    fn reports_correct_expected_class_for_non_ascii() {
        // Position 1 (Letter expected)
        let err1 = normalize("£S0378331005").unwrap_err();
        assert_eq!(
            err1,
            IsinError::InvalidCharacter {
                character: '£',
                position: 1,
                expected: CharacterClass::Letter,
            }
        );

        // Position 12 (Digit expected)
        let err12 = normalize("US037833100£").unwrap_err();
        assert_eq!(
            err12,
            IsinError::InvalidCharacter {
                character: '£',
                position: 12,
                expected: CharacterClass::Digit,
            }
        );
    }
}

#[cfg(test)]
mod tests_arbitrary {
    use super::*;

    #[test]
    fn always_produces_valid_isins() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let isin = Isin::arbitrary(&mut u).expect("arbitrary should always succeed");
            // Re-validating via parse() proves the value round-trips through the exact same
            // checks a hand-typed input would.
            assert!(Isin::parse(isin.as_str()).is_ok());
        }
    }
}

#[cfg(test)]
mod tests_proptest {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_isin_always_round_trips_through_parse(isin in valid_isin()) {
            let reparsed = Isin::parse(isin.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(isin, reparsed.unwrap());
        }

        #[test]
        fn valid_isin_string_always_parses(s in valid_isin_string()) {
            prop_assert!(Isin::parse(&s).is_ok());
        }
    }
}

#[cfg(test)]
mod tests_schemars {
    use crate::identifiers::isin::Isin;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(Isin);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "isin");
        assert_eq!(json["minLength"], 12);
        assert_eq!(json["maxLength"], 12);
        assert_eq!(json["pattern"], "^[A-Z]{2}[A-Z0-9]{9}[0-9]$");
    }
}

#[cfg(test)]
mod tests_serde {
    use crate::identifiers::Isin;

    #[test]
    fn round_trips_through_json() {
        let isin = Isin::parse("US0378331005").unwrap();
        let json = serde_json::to_string(&isin).unwrap();
        assert_eq!(json, "\"US0378331005\"");
        let back: Isin = serde_json::from_str(&json).unwrap();
        assert_eq!(isin, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<Isin>("\"not-an-isin\"").unwrap_err();
        assert!(err.to_string().contains("ISIN"));
    }
}
