//! CNPJ (Cadastro Nacional da Pessoa Jurídica) — Brazil's national registry identifier for legal
//! entities, issued by the Receita Federal.
//!
//! This module provides the validated Rust representation ([`Cnpj`]) and the parsing, formatting,
//! validation, and error types that surround it. It accepts both the conventional punctuated
//! `AA.AAA.AAA/AAAA-DD` form and the compact 14-character form, normalizes ASCII case, and
//! guarantees that any constructed [`Cnpj`] satisfies the format and Módulo 11 checksum rules
//! described below. There is no partially-validated state: if you hold a [`Cnpj`], it is valid.
//!
//! # What this type represents
//!
//! A CNPJ has 14 meaningful characters, split into three logical segments:
//!
//! | Positions | Length | Segment            | Meaning                                                        |
//! |-----------|--------|---------------------|-----------------------------------------------------------------|
//! | 1–8       | 8      | Root (raiz)          | Identifies the entity itself; shared by the head office and every branch |
//! | 9–12      | 4      | Branch/order (ordem) | `"0001"` conventionally denotes the head office (matriz)         |
//! | 13–14     | 2      | Verification digits  | Computed from the first 12 characters via Módulo 11              |
//!
//! [`Cnpj`] stores those 14 characters as normalized uppercase ASCII and exposes borrowed
//! accessors for the root ([`Cnpj::root`]), the branch/order segment ([`Cnpj::branch_code`]), and
//! both the compact ([`Cnpj::as_str`]) and punctuated ([`Cnpj::formatted`]) renderings.
//!
//! # Numeric and alphanumeric formats
//!
//! The public format changed in 2026: the first 12 positions (root + branch/order) may now
//! contain uppercase letters as well as digits, while the final two verification digits remain
//! numeric. This crate follows `Nota Técnica Conjunta COCAD/SUARA/RFB nº 49/2024`, which keeps the
//! legacy numeric-only Módulo 11 calculation unchanged as a special case: each character
//! contributes its ASCII code minus `'0'` to the checksum (so `'A'` = 17, ..., `'Z'` = 42, and
//! digits contribute their own value), meaning a purely numeric CNPJ produces exactly the checksum
//! it always has.
//!
//! Older numeric-only CNPJs remain valid and are treated as a special case of the same
//! 14-character, same-checksum format — [`Cnpj`] represents both uniformly; there is no separate
//! legacy type, and no separate code path to keep in sync.
//!
//! # Validation rules
//!
//! Every fallible constructor runs the same rules, in order, and each maps to one [`CnpjError`]
//! variant:
//!
//! 1. **Length** — after formatting is stripped, the input must contain exactly 14 meaningful
//!    characters ([`CnpjError::InvalidLength`]).
//! 2. **Character class** — positions 1–12 accept a digit or an uppercase letter; positions 13–14
//!    accept only a digit ([`CnpjError::InvalidCharacter`]).
//! 3. **Not degenerate** — the 14 characters cannot all be identical, e.g. `"00000000000000"`
//!    ([`CnpjError::RepeatedDigits`]). Such inputs are structurally well-formed, and can even
//!    satisfy the checksum for certain repeated digits, but the Receita Federal never issues them;
//!    they are reliably placeholder or data-entry artifacts.
//! 4. **Checksum** — both verification digits must match the Módulo 11 algorithm applied to the
//!    preceding characters ([`CnpjError::InvalidCheckDigits`]).
//!
//! [`Cnpj::parse`] additionally strips conventional punctuation (`.`, `/`, `-`, ASCII spaces)
//! before these rules apply, and rejects empty input up front ([`CnpjError::Empty`]).
//! [`Cnpj::from_bytes`] skips the punctuation-stripping step but still enforces every rule above.
//!
//! # Design notes
//!
//! - **No invalid state is representable.** [`Cnpj`]'s only field is private; the only ways to
//!   obtain one are [`Cnpj::parse`], [`Cnpj::new`], [`Cnpj::from_bytes`], [`FromStr`], and
//!   [`TryFrom<&str>`] — every one of them runs full validation. There is no unchecked or
//!   "trust me" constructor exposed publicly.
//! - **Zero allocation, `Copy`, allocation-free.** [`Cnpj`] is a 14-byte value type wrapping
//!   `[u8; 14]`. Parsing, validating, formatting, and every accessor operate on the stack; nothing
//!   in this module requires an allocator.
//! - **Ordering and hashing are byte-wise.** [`Cnpj`] derives [`Ord`] and [`Hash`] directly over
//!   its underlying ASCII bytes, which matches [`str`] ordering on [`Cnpj::as_str`]. Because ASCII
//!   digits (`'0'..='9'`) sort before uppercase letters (`'A'..='Z'`), a numeric-format CNPJ always
//!   sorts before any alphanumeric CNPJ sharing the same leading digits. This is lexicographic
//!   string order, not numeric order — don't read it as meaning "issued earlier" or
//!   "smaller root number".
//! - **Safe to use as a map/set key.** [`Cnpj`] implements [`Eq`] and [`Hash`] consistently with
//!   [`PartialEq`], so it works as a `HashMap`/`HashSet` key or a `BTreeMap`/`BTreeSet` key out of
//!   the box.
//!
//! # Feature flags
//!
//! This module's optional integrations are off by default and purely additive — enabling one
//! never changes the behavior of [`Cnpj::parse`] or the validation rules above:
//!
//! - **`serde`** — (de)serializes [`Cnpj`] as its compact 14-character string (e.g.
//!   `"12ABC34501DE35"`), never the punctuated form. Deserialization re-runs full validation, so an
//!   untrusted payload can never produce an invalid [`Cnpj`].
//! - **`schemars`** — implements `JsonSchema` for [`Cnpj`], describing it as a pattern-constrained
//!   string (`^[A-Z0-9]{12}[0-9]{2}$`). Implies `serde`.
//! - **`arbitrary`** — implements `Arbitrary` for [`Cnpj`], generating structurally valid,
//!   checksum-correct values for fuzz targets.
//! - **`proptest`** — exposes reusable `proptest` strategies (`valqeron_identifiers::cnpj::proptest`,
//!   when this feature is enabled) for generating checksum-valid [`Cnpj`] values and their
//!   formatted string representations, so downstream property tests don't need to hand-roll a
//!   generator.
//!
//! # Error handling
//!
//! Every fallible constructor returns [`CnpjError`], which is `Clone + PartialEq + Eq` and
//! implements [`core::error::Error`] and [`core::fmt::Display`], so it composes with `?` and with
//! error-aggregation crates alike. Match on it when you need to react to a specific failure mode
//! (for example, surfacing "which character was wrong" to a form field) rather than just the
//! human-readable message:
//!
//! ```
//! use valqeron_identifiers::{Cnpj, CnpjError};
//!
//! match Cnpj::parse("12.345.678/0001-XX") {
//!     Ok(cnpj) => println!("valid: {cnpj}"),
//!     Err(CnpjError::InvalidCheckDigits { expected, found, .. }) => {
//!         println!("checksum mismatch: expected {expected}, found {found}");
//!     }
//!     Err(other) => println!("rejected: {other}"),
//! }
//! ```
//!
//! # Examples
//!
//! ```
//! use valqeron_identifiers::Cnpj;
//!
//! let numeric = Cnpj::parse("00.000.000/0001-91").unwrap();
//! assert!(numeric.is_root());
//! assert_eq!(numeric.as_str(), "00000000000191");
//! assert_eq!(numeric.formatted().as_str(), "00.000.000/0001-91");
//!
//! let alpha = Cnpj::parse("12ABC34501DE35").unwrap();
//! assert_eq!(alpha.branch_code(), "01DE");
//! assert_eq!(alpha.branch_number(), None);
//! ```
//!
//! Sorting and deduplicating a batch of CNPJs, e.g. after importing them from a spreadsheet:
//!
//! ```
//! use valqeron_identifiers::Cnpj;
//!
//! let mut cnpjs: Vec<Cnpj> = ["11.222.333/0002-62", "00.000.000/0001-91", "00.000.000/0001-91"]
//!     .into_iter()
//!     .map(|s| Cnpj::parse(s).unwrap())
//!     .collect();
//! cnpjs.sort();
//! cnpjs.dedup();
//! assert_eq!(cnpjs.len(), 2);
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

pub use error::CnpjError;
pub use fmt::FormattedCnpj;

use core::convert::TryFrom;
use core::str::{FromStr, from_utf8_unchecked};

/// A validated CNPJ (Cadastro Nacional da Pessoa Jurídica).
///
/// `Cnpj` is a 14-byte, `Copy`, allocation-free value object. Once constructed, it is guaranteed to
/// satisfy the structural rules and Módulo 11 checksum required by the crate — there is no way to
/// obtain a `Cnpj` that hasn't passed validation.
///
/// Internally, the identifier is stored as raw uppercase ASCII bytes (`'0'...='9'` or `'A'...='Z'`).
/// This keeps the compact representation lossless and makes borrowed access to the normalized form cheap.
///
/// # Constructing a `Cnpj`
///
/// | Constructor                      | Accepts                                                       |
/// |-----------------------------------|----------------------------------------------------------------|
/// | [`Cnpj::parse`] / [`Cnpj::new`]   | Punctuated or compact strings, any ASCII case                   |
/// | [`Cnpj::from_bytes`]              | Exactly 14 pre-normalized ASCII bytes, no punctuation            |
/// | [`FromStr`] / [`TryFrom<&str>`]   | Same as `parse`, for use in generic code                        |
///
/// All of them run the same validation and return [`CnpjError`] on failure. See the [module-level
/// documentation](self) for the field layout, format history, and design rationale.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a parsed Cnpj should be used; discarding it wastes the validation work"]
pub struct Cnpj {
    bytes: [u8; 14],
}

impl Cnpj {
    /// Parses a CNPJ from a string.
    ///
    /// The parser accepts the conventional `AA.AAA.AAA/AAAA-DD` form as well as the compact
    /// 14-character form. It also tolerates surrounding and embedded ASCII spaces and folds ASCII
    /// letters to uppercase before validation.
    ///
    /// This is the primary constructor; [`Cnpj::new`], [`FromStr`], and [`TryFrom<&str>`] all delegate to it.
    ///
    /// # Errors
    ///
    /// Returns [`CnpjError`] if the input is empty, does not contain exactly 14 meaningful
    /// characters after formatting is removed, contains a character invalid for its position,
    /// consists of a single repeated character, or fails the checksum.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cnpj;
    ///
    /// assert!(Cnpj::parse("00.000.000/0001-91").is_ok());
    /// assert!(Cnpj::parse("00000000000191").is_ok());
    /// assert!(Cnpj::parse("12abc34501de35").is_ok()); // lowercase is folded automatically
    /// assert!(Cnpj::parse("not-a-cnpj").is_err());
    /// ```
    pub fn parse(input: &str) -> Result<Self, CnpjError> {
        let candidate = parser::normalize(input)?;
        Self::from_bytes(candidate)
    }

    /// Alias for [`Cnpj::parse`].
    ///
    /// # Errors
    ///
    /// See [`Cnpj::parse`].
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cnpj;
    ///
    /// assert_eq!(Cnpj::new("00000000000191"), Cnpj::parse("00000000000191"));
    /// ```
    #[inline]
    pub fn new(input: &str) -> Result<Self, CnpjError> {
        Self::parse(input)
    }

    /// Constructs a `Cnpj` directly from 14 raw ASCII bytes.
    ///
    /// Each byte must already be an ASCII digit, and for the first 12 positions may also be an
    /// uppercase ASCII letter. Use [`Cnpj::parse`] if the input might contain punctuation or
    /// lowercase letters.
    ///
    /// Numeric-only CNPJs remain fully supported. Pass ASCII digit bytes (`b'0'...=b'9'`), not raw
    /// numeric values.
    ///
    /// # Errors
    ///
    /// Returns [`CnpjError`] under the same conditions as [`Cnpj::parse`], except that length is
    /// guaranteed by the `[u8; 14]` type itself: [`CnpjError::InvalidLength`] cannot occur here.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cnpj;
    ///
    /// let cnpj = Cnpj::from_bytes(*b"00000000000191").unwrap();
    /// assert_eq!(cnpj.as_str(), "00000000000191");
    ///
    /// // A malformed checksum is rejected just like it would be through `parse`.
    /// assert!(Cnpj::from_bytes(*b"00000000000192").is_err());
    /// ```
    #[doc(alias = "from_digits")]
    pub fn from_bytes(bytes: [u8; 14]) -> Result<Self, CnpjError> {
        validation::validate(&bytes)?;
        Ok(Cnpj { bytes })
    }

    /// Returns the 14 raw ASCII bytes backing this CNPJ.
    ///
    /// The returned bytes are in compact form, without punctuation (for example, `b"12ABC34501DE35"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cnpj;
    ///
    /// let cnpj = Cnpj::parse("00000000000191").unwrap();
    /// assert_eq!(cnpj.as_bytes(), b"00000000000191");
    /// ```
    #[doc(alias = "digits")]
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 14] {
        &self.bytes
    }

    /// Returns the compact CNPJ as a `&str`.
    ///
    /// This never allocates: the bytes are guaranteed to be valid ASCII by construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cnpj;
    ///
    /// let cnpj = Cnpj::parse("00.000.000/0001-91").unwrap();
    /// assert_eq!(cnpj.as_str(), "00000000000191");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: The bytes array is strictly guaranteed to contain only
        // valid ASCII uppercase alphanumeric characters by `Cnpj::from_bytes`.
        unsafe { from_utf8_unchecked(&self.bytes) }
    }

    /// Renders the punctuated `AA.AAA.AAA/AAAA-DD` form without heap allocation.
    ///
    /// See [`FormattedCnpj`]. [`Cnpj`]'s own [`Display`](core::fmt::Display) implementation
    /// delegates to this, so `cnpj.to_string()` and `cnpj.formatted().to_string()` are equivalent.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cnpj;
    ///
    /// let cnpj = Cnpj::parse("00000000000191").unwrap();
    /// assert_eq!(cnpj.formatted().as_str(), "00.000.000/0001-91");
    /// assert_eq!(cnpj.to_string(), cnpj.formatted().as_str());
    /// ```
    #[inline]
    #[must_use]
    pub fn formatted(&self) -> FormattedCnpj {
        FormattedCnpj::new(self)
    }

    /// Returns the 8-character root segment.
    ///
    /// This identifies the entity itself and is shared by the company and all of its branches.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cnpj;
    ///
    /// let cnpj = Cnpj::parse("00000000000191").unwrap();
    /// assert_eq!(cnpj.root(), "00000000");
    /// ```
    #[inline]
    #[must_use]
    pub fn root(&self) -> &str {
        &self.as_str()[0..8]
    }

    /// Returns the 4-character branch/order segment.
    ///
    /// `"0001"` conventionally denotes the head office (matriz); see [`Cnpj::is_root`].
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cnpj;
    ///
    /// let cnpj = Cnpj::parse("11.222.333/0002-62").unwrap();
    /// assert_eq!(cnpj.branch_code(), "0002");
    /// ```
    #[inline]
    #[must_use]
    pub fn branch_code(&self) -> &str {
        &self.as_str()[8..12]
    }

    /// Returns `true` when the branch/order segment is `"0001"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cnpj;
    ///
    /// assert!(Cnpj::parse("00000000000191").unwrap().is_root());
    /// assert!(!Cnpj::parse("11.222.333/0002-62").unwrap().is_root());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.branch_code() == "0001"
    }

    /// Returns the branch/order segment as a number when it is purely numeric.
    ///
    /// Returns `None` when the segment contains a letter, which is only possible for
    /// alphanumeric-format CNPJs. Numeric CNPJs, including the conventional matriz marker
    /// (`"0001"`), always parse successfully.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cnpj;
    ///
    /// let matriz = Cnpj::parse("00000000000191").unwrap();
    /// assert_eq!(matriz.branch_number(), Some(1));
    ///
    /// let alphanumeric_branch = Cnpj::parse("12ABC34501DE35").unwrap();
    /// assert_eq!(alphanumeric_branch.branch_code(), "01DE");
    /// assert_eq!(alphanumeric_branch.branch_number(), None);
    /// ```
    #[must_use]
    pub fn branch_number(&self) -> Option<u16> {
        self.branch_code().parse().ok()
    }

    /// Returns the two verification digits as numeric values.
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::Cnpj;
    ///
    /// let cnpj = Cnpj::parse("00000000000191").unwrap();
    /// assert_eq!(cnpj.check_digits(), (9, 1));
    /// ```
    #[inline]
    #[must_use]
    pub fn check_digits(&self) -> (u8, u8) {
        (self.bytes[12] - b'0', self.bytes[13] - b'0')
    }
}

impl FromStr for Cnpj {
    type Err = CnpjError;

    /// Delegates to [`Cnpj::parse`], enabling `input.parse::<Cnpj>()` and use in generic code
    /// bounded by [`FromStr`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Cnpj {
    type Error = CnpjError;

    /// Delegates to [`Cnpj::parse`], enabling `Cnpj::try_from(input)` and use in generic code
    /// bounded by [`TryFrom<&str>`].
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<[u8; 14]> for Cnpj {
    type Error = CnpjError;

    /// Delegates to [`Cnpj::from_bytes`]. The bytes must already be pre normalized ASCII, without
    /// punctuation.
    fn try_from(value: [u8; 14]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for Cnpj {
    type Error = CnpjError;

    /// Validates a byte slice as a CNPJ. The slice must be exactly 14 pre normalized ASCII bytes,
    /// without punctuation; any other length yields [`CnpjError::InvalidLength`]. Once the length is
    /// confirmed, this behaves like [`Cnpj::from_bytes`].
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 14] = value
            .try_into()
            .map_err(|_| CnpjError::InvalidLength { found: value.len() })?;
        Self::from_bytes(bytes)
    }
}

impl PartialEq<str> for Cnpj {
    /// Compares against a string slice by its compact 14 character representation (not the
    /// punctuated form).
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Cnpj {
    /// Compares against a string slice by its compact 14 character representation (not the
    /// punctuated form).
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Cnpj> for str {
    fn eq(&self, other: &Cnpj) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Cnpj> for &str {
    fn eq(&self, other: &Cnpj) -> bool {
        *self == other.as_str()
    }
}

impl AsRef<[u8]> for Cnpj {
    /// Equivalent to [`Cnpj::as_bytes`], borrowed as a slice.
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for Cnpj {
    /// Equivalent to [`Cnpj::as_str`].
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
