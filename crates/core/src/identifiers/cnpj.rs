//! CNPJ (Cadastro Nacional da Pessoa Jurídica), Brazil's national registry identifier for legal
//! entities, issued by the Receita Federal.
//!
//! This module provides the validated Rust representation ([`Cnpj`]) and the parsing, formatting,
//! validation, and error types that surround it. It accepts both the conventional punctuated
//! `AA.AAA.AAA/AAAA-DD` form and the compact 14-character form, normalizes ASCII cases, and
//! guarantees that any constructed [`Cnpj`] satisfies the format and Módulo 11 checksum rules
//! described below. There is no partially validated state: if you hold a [`Cnpj`], it is valid.
//!
//! # What this type represents
//!
//! A CNPJ has 14 meaningful characters, split into three logical segments:
//!
//! | Positions | Length | Segment | Meaning |
//! |-----------|--------|---------------------|-----------------------------------------------------------------|
//! | 1–8 | 8 | Root (raiz) | Identifies the entity itself; shared by the head office and every branch |
//! | 9–12 | 4 | Branch/order (ordem) | `"0001"` conventionally denotes the head office (matriz) |
//! | 13–14 | 2 | Verification digits | Computed from the first 12 characters via Módulo 11 algorithm |
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
//! 14-character, same-checksum format. [`Cnpj`] represents both uniformly; there is no separate
//! legacy type and no separate code path to keep in sync.
//!
//! # Validation rules
//!
//! Every fallible constructor runs the same rules, in order, and each maps to one [`CnpjError`]
//! variant:
//!
//! 1. **Length**: after formatting is stripped, the input must contain exactly 14 meaningful
//!    characters ([`CnpjError::InvalidLength`]).
//! 2. **Character class**: positions 1–12 accept a digit or an uppercase letter; positions 13–14
//!    accept only a digit ([`CnpjError::InvalidCharacter`]).
//! 3. **Not degenerate**: the 14 characters cannot all be identical, e.g. `"00000000000000"`
//!    ([`CnpjError::RepeatedDigits`]). Such inputs are structurally well-formed and can even
//!    satisfy the checksum for certain repeated digits, but the Receita Federal never issues them;
//!    they are reliably placeholder or data-entry artifacts.
//! 4. **Checksum**: both verification digits must match the Módulo 11 algorithm applied to the
//!    preceding characters ([`CnpjError::InvalidCheckDigits`]).
//!
//! [`Cnpj::parse`] additionally strips conventional punctuation (`.`, `/`, `-`, ASCII spaces)
//! before these rules apply, and rejects empty input up front ([`CnpjError::Empty`]).
//! [`Cnpj::from_bytes`] skips the punctuation-stripping step but still enforces every rule above.
//!
//! # Design notes
//!
//! - **No invalid state is representable.** [`Cnpj`]'s only field is private; the only ways to
//!   get one are [`Cnpj::parse`], [`Cnpj::new`], [`Cnpj::from_bytes`], [`FromStr`], and
//!   [`TryFrom<&str>`] — every one of them runs full validation. There is no unchecked or
//!   "trust me" constructor exposed publicly.
//! - **Zero allocation, `Copy`, allocation-free.** [`Cnpj`] is a 14-byte value type wrapping
//!   `[u8; 14]`. Parsing, validating, formatting, and every accessor operate on the stack; nothing
//!   in this module requires an allocator.
//! - **Ordering and hashing are byte-wise.** [`Cnpj`] derives [`Ord`] and [`Hash`] directly over
//!   its underlying ASCII bytes, which matches [`str`] ordering on [`Cnpj::as_str`]. Because ASCII
//!   digits (`'0'...='9'`) sort before uppercase letters (`'A'...='Z'`), a numeric-format CNPJ always
//!   sorts before any alphanumeric CNPJ sharing the same leading digits. This is a lexicographic
//!   string order, not a numeric order. Don't read it as meaning "issued earlier" or "smaller root number".
//! - **Safe to use as a map/set key.** [`Cnpj`] implements [`Eq`] and [`Hash`] consistently with
//!   [`PartialEq`], so it works as a `HashMap`/`HashSet` key or a `BTreeMap`/`BTreeSet` key out of
//!   the box.
//!
//! # Feature flags
//!
//! This module's optional integrations are off by default and purely additive, enabling one
//! never changes the behavior of [`Cnpj::parse`] or the validation rules above:
//!
//! - **`serde`**: (de)serializes [`Cnpj`] as its compact 14-character string (e.g.
//!   `"12ABC34501DE35"`), never the punctuated form. Deserialization re-runs full validation, so an
//!   untrusted payload can never produce an invalid [`Cnpj`].
//! - **`schemars`**: implements `JsonSchema` for [`Cnpj`], describing it as a pattern-constrained
//!   string (`^[A-Z0-9]{12}[0-9]{2}$`). Implies `serde`.
//! - **`arbitrary`**: implements `Arbitrary` for [`Cnpj`], generating structurally valid,
//!   checksum-correct values for fuzz targets.
//! - **`proptest`**: exposes reusable `proptest` strategies (`valqeron_identifiers::cnpj::proptest`,
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
//! Sorting and deduplicating a batch of CNPJs, e.g., after importing them from a spreadsheet:
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

#[cfg(feature = "arbitrary")]
use arbitrary::{Arbitrary, Unstructured};
#[cfg(feature = "proptest")]
use proptest::prelude::{Strategy, prop};
#[cfg(feature = "schemars")]
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, de::Visitor};
#[cfg(feature = "proptest")]
use std::borrow::Cow;
use std::{
    fmt,
    ops::Deref,
    str::{FromStr, from_utf8_unchecked},
};

/// A validated CNPJ (Cadastro Nacional da Pessoa Jurídica).
///
/// `Cnpj` is a 14-byte, `Copy`, allocation-free value object. Once constructed, it is guaranteed to
/// satisfy the structural rules and Módulo 11 checksum required by the crate. There is no way to
/// get a `Cnpj` that hasn't passed validation.
///
/// Internally, the identifier is stored as raw uppercase ASCII bytes (`'0'...='9'` or `'A'...='Z'`).
/// This keeps the compact representation lossless and makes borrowed access to the normalized form cheap.
///
/// # Constructing a `Cnpj`
///
/// | Constructor | Accepts |
/// |-----------------------------------|----------------------------------------------------------------|
/// | [`Cnpj::parse`] / [`Cnpj::new`] | Punctuated or compact strings, any ASCII case |
/// | [`Cnpj::from_bytes`] | Exactly 14 pre-normalized ASCII bytes, no punctuation |
/// | [`FromStr`] / [`TryFrom<&str>`] | Same as `parse`, for use in generic code |
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
    /// This is the primary constructor; [`Cnpj::new`], [`FromStr`], and [`TryFrom<&str>`] all
    /// delegate to it.
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
        let candidate = normalize(input)?;
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
        validate(&bytes)?;
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

    /// Delegates to [`Cnpj::from_bytes`]. The bytes must already be pre-normalized ASCII, without
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
    /// Compares against a string slice by its compact 14-character representation (not the
    /// punctuated form).
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Cnpj {
    /// Compares against a string slice by its compact 14-character representation (not the
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

/// Total length of the punctuated representation: `AA.AAA.AAA/AAAA-DD`.
const FORMATTED_LEN: usize = 18;

/// A stack-allocated, punctuated rendering of a [`Cnpj`] (`AA.AAA.AAA/AAAA-DD`), e.g. `"12.ABC.345/01DE-35"`.
///
/// This exists so that [`Cnpj::formatted`] can hand back a `Display`-able, `Deref<Target = str>`
/// value without allocating on the heap. Everything backing it is a fixed-size `[u8; 18]` produced
/// once at construction.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormattedCnpj([u8; FORMATTED_LEN]);

impl FormattedCnpj {
    pub(super) fn new(cnpj: &Cnpj) -> Self {
        let d = cnpj.as_bytes();
        let mut out = [0u8; FORMATTED_LEN];
        let layout: [u8; FORMATTED_LEN] = [
            d[0], d[1], b'.', d[2], d[3], d[4], b'.', d[5], d[6], d[7], b'/', d[8], d[9], d[10],
            d[11], b'-', d[12], d[13],
        ];
        out.copy_from_slice(&layout);
        FormattedCnpj(out)
    }

    /// Borrows the formatted value as a `&str`.
    ///
    /// This never allocates and never panics: the byte layout is built exclusively from [`Cnpj`]'s
    /// own validated ASCII bytes plus ASCII punctuation, which is guaranteed valid UTF-8.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `FormattedCnpj::new` builds this buffer exclusively from a validated `Cnpj`'s
        // ASCII bytes interleaved with ASCII punctuation (`.`, `/`, `-`), so it is always UTF-8.
        unsafe { core::str::from_utf8_unchecked(&self.0) }
    }
}

impl Deref for FormattedCnpj {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for FormattedCnpj {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for FormattedCnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for FormattedCnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("FormattedCnpj")
            .field(&self.as_str())
            .finish()
    }
}

impl fmt::Display for Cnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.formatted().as_str())
    }
}

impl fmt::Debug for Cnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Cnpj")
            .field(&self.formatted().as_str())
            .finish()
    }
}

/// The set of characters permitted at a given position of a CNPJ.
///
/// Reported by [`CnpjError::InvalidCharacter`] to describe what was expected where an invalid
/// character was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CharacterClass {
    /// An ASCII digit, `'0'...='9'`.
    Digit,
    /// An ASCII digit or an uppercase ASCII letter, `'0'...='9' | 'A'...='Z'`.
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

/// Weights applied left-to-right to the 12 base characters (root plus branch) when computing the
/// first verification digit (DV1).
const WEIGHTS_DV1: [u32; 12] = [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];

/// Weights applied left-to-right to the 12 base characters plus DV1 (13 values total) when computing
/// the second verification digit (DV2).
const WEIGHTS_DV2: [u32; 13] = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];

/// The number of positions occupied by the root + branch/order segment.
const BASE_LEN: usize = 12;

/// Adjusts a generated base segment so it is not a degenerate all-repeated-character value.
///
/// This is only used by fuzz/property generators. It preserves the generated shape while avoiding
/// the one pattern that `validate` rejects independently.
#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
fn avoid_all_repeated(base: &mut [u8; BASE_LEN]) {
    if base.iter().all(|&b| b == base[0]) {
        base[0] = if base[0] == b'0' { b'1' } else { b'0' };
    }

    debug_assert!(
        !base.iter().all(|&b| b == base[0]),
        "Base segment must not consist of entirely repeated characters. Is BASE_LEN too small?"
    );
}

/// Runs every validation rule against a normalized candidate, in order from cheapest/most-specific
/// to most-expensive:
/// 1. Character class per position (digit-only tail, alphanumeric head).
/// 2. Rejection of degenerate all-repeated-character input.
/// 3. Módulo 11 checksum for both verification digits.
fn validate(candidate: &[u8; 14]) -> Result<(), CnpjError> {
    validate_character_classes(candidate)?;
    validate_not_repeated(candidate)?;
    validate_check_digits(candidate)?;
    Ok(())
}

/// Converts a validated ASCII byte (`'0'...='9'` or `'A'...='Z'`) into its numeric value for the
/// Módulo 11 calculation: `ASCII code - 48`.
///
/// For digits this is simply the digit's value (`'0'` -> 0, ..., `'9'` -> 9).
/// For uppercase letters this yields 17...=42 (`'A'` -> 17, ..., `'Z'` -> 42), per `Nota Técnica
/// Conjunta COCAD/SUARA/RFB nº 49/2024`, which keeps the legacy numeric-only calculation unchanged
/// as a special case.
#[inline]
fn char_value(byte: u8) -> u32 {
    (byte - b'0') as u32
}

fn validate_character_classes(candidate: &[u8; 14]) -> Result<(), CnpjError> {
    for (i, &byte) in candidate.iter().enumerate() {
        let is_valid = if i < BASE_LEN {
            byte.is_ascii_digit() || byte.is_ascii_uppercase()
        } else {
            byte.is_ascii_digit()
        };
        if !is_valid {
            let expected = if i < BASE_LEN {
                CharacterClass::Alphanumeric
            } else {
                CharacterClass::Digit
            };
            return Err(CnpjError::InvalidCharacter {
                character: byte as char,
                position: (i + 1) as u8,
                expected,
            });
        }
    }
    Ok(())
}

fn validate_not_repeated(candidate: &[u8; 14]) -> Result<(), CnpjError> {
    if candidate.iter().all(|&b| b == candidate[0]) {
        return Err(CnpjError::RepeatedDigits);
    }
    Ok(())
}

fn validate_check_digits(candidate: &[u8; 14]) -> Result<(), CnpjError> {
    let values: [u32; 14] = core::array::from_fn(|i| char_value(candidate[i]));

    let dv1 = compute_check_digit(&values[..BASE_LEN], &WEIGHTS_DV1);
    let found_dv1 = values[12] as u8;
    if dv1 != found_dv1 {
        return Err(CnpjError::InvalidCheckDigits {
            position: 13,
            expected: dv1,
            found: found_dv1,
        });
    }

    let mut dv2_input = [0u32; BASE_LEN + 1];
    dv2_input[..BASE_LEN].copy_from_slice(&values[..BASE_LEN]);
    dv2_input[BASE_LEN] = dv1 as u32;
    let dv2 = compute_check_digit(&dv2_input, &WEIGHTS_DV2);
    let found_dv2 = values[13] as u8;
    if dv2 != found_dv2 {
        return Err(CnpjError::InvalidCheckDigits {
            position: 14,
            expected: dv2,
            found: found_dv2,
        });
    }

    Ok(())
}

/// Computes the two verification digits for a well-formed 12-character base segment (each byte
/// already `'0'...='9'` or `'A'...='Z'`).
///
/// Exposed to sibling modules ([`super::arbitrary`], [`super::proptest`]) so they can generate
/// structurally valid, checksum-correct `Cnpj` values without duplicating the Módulo 11 algorithm.
/// Only called when one of those optional features is enabled, hence the `allow`.
#[cfg_attr(
    not(any(feature = "arbitrary", feature = "proptest")),
    allow(dead_code)
)]
fn compute_valid_check_digits(base: &[u8; BASE_LEN]) -> (u8, u8) {
    let base_values: [u32; BASE_LEN] = core::array::from_fn(|i| char_value(base[i]));
    let dv1 = compute_check_digit(&base_values, &WEIGHTS_DV1);

    let mut dv2_input = [0u32; BASE_LEN + 1];
    dv2_input[..BASE_LEN].copy_from_slice(&base_values);
    dv2_input[BASE_LEN] = dv1 as u32;
    let dv2 = compute_check_digit(&dv2_input, &WEIGHTS_DV2);

    (dv1, dv2)
}

/// The classic Módulo 11 verification-digit algorithm, unchanged since the numeric-only CNPJ era:
/// weighted sum, remainder mod 11, then `0` if the remainder is `0` or `1`, otherwise `11 - remainder`.
fn compute_check_digit(values: &[u32], weights: &[u32]) -> u8 {
    debug_assert_eq!(values.len(), weights.len());
    let sum: u32 = values.iter().zip(weights).map(|(v, w)| v * w).sum();
    let remainder = sum % 11;
    if remainder < 2 {
        0
    } else {
        (11 - remainder) as u8
    }
}

/// The error returned when a [`Cnpj`](crate::Cnpj) fails to parse or validate.
///
/// Each variant corresponds to one validation rule, in the order the rules are applied. See the
/// [module level documentation](crate::cnpj) for the full rule list.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CnpjError {
    /// The input was an empty string.
    Empty,

    /// After stripping punctuation (`.`, `/`, `-`, Unicode whitespace), the input did not contain exactly
    /// 14 meaningful characters.
    InvalidLength {
        /// The number of meaningful (non-punctuation) characters found.
        found: usize,
    },

    /// A character outside the allowed set was found at a given position.
    InvalidCharacter {
        /// The offending character, as originally provided (before case folding).
        character: char,
        /// 1-indexed position within the 14 meaningful characters.
        position: u8,
        /// The character class that was expected at this position.
        expected: CharacterClass,
    },

    /// The Módulo 11 checksum did not match one of the two verification digits.
    InvalidCheckDigits {
        /// 1-indexed position of the mismatching verification digit (13 or 14).
        position: u8,
        /// The verification digit computed from the Módulo 11 algorithm.
        expected: u8,
        /// The verification digit actually present in the input.
        found: u8,
    },

    /// All 14 characters were identical (e.g. `"00000000000000"`).
    ///
    /// Such inputs are structurally well-formed and can even satisfy the Módulo 11 checksum for
    /// certain repeated digits, but the Receita Federal never issues them;
    /// they are reliably placeholder or data-entry artifacts.
    RepeatedDigits,
}

impl fmt::Display for CnpjError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CnpjError::Empty => f.write_str("CNPJ input is empty"),
            CnpjError::InvalidLength { found } => write!(
                f,
                "CNPJ must contain exactly 14 characters once formatting is removed, found {found}"
            ),
            CnpjError::InvalidCharacter {
                character,
                position,
                expected,
            } => write!(
                f,
                "invalid character '{character}' at position {position} of 14: expected {expected}"
            ),
            CnpjError::InvalidCheckDigits {
                position,
                expected,
                found,
            } => write!(
                f,
                "invalid check digit at position {position} of 14: expected {expected}, found {found}"
            ),
            CnpjError::RepeatedDigits => {
                f.write_str("CNPJ cannot consist of a single character repeated 14 times")
            }
        }
    }
}

impl core::error::Error for CnpjError {}

// ================================= PARSER =================================
/// Characters stripped from input before length/content checks apply.
#[inline]
fn is_formatting_char(c: char) -> bool {
    matches!(c, '.' | '/' | '-') || c.is_whitespace()
}

/// Normalizes `input` into a 14-byte ASCII array.
///
/// - Empty input is rejected as [`CnpjError::Empty`].
/// - Formatting characters (`.`, `/`, `-`, Unicode whitespace) are stripped anywhere they appear.
/// - Remaining characters are ASCII-uppercased (so lowercase letters in the alphanumeric portion
///   are accepted transparently).
/// - Any remaining non-ASCII character, or a meaningful-character count other than 14, is rejected.
///
/// This function does **not** check that each position holds a character valid for that position
/// (digit vs. alphanumeric); see [`super::validation::validate`] for that.
fn normalize(input: &str) -> Result<[u8; 14], CnpjError> {
    if input.is_empty() {
        return Err(CnpjError::Empty);
    }

    let meaningful = input.chars().filter(|&c| !is_formatting_char(c));
    let found = meaningful.clone().count();
    if found != 14 {
        return Err(CnpjError::InvalidLength { found });
    }

    let mut buf = [0u8; 14];
    for (i, ch) in meaningful.enumerate() {
        if !ch.is_ascii() {
            return Err(CnpjError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
                expected: CharacterClass::Alphanumeric,
            });
        }
        buf[i] = ch.to_ascii_uppercase() as u8;
    }

    Ok(buf)
}

// ================================= SERDE =================================
#[cfg(feature = "serde")]
impl Serialize for Cnpj {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
struct CnpjVisitor;

#[cfg(feature = "serde")]
impl<'de> Visitor<'de> for CnpjVisitor {
    type Value = Cnpj;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 14-character CNPJ, optionally punctuated as AA.AAA.AAA/AAAA-DD")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Cnpj::parse(v).map_err(de::Error::custom)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Cnpj {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(CnpjVisitor)
    }
}

// ================================= SCHEMARS, PROPTEST, ARBITRARY =================================
const ALPHABET: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

#[cfg(feature = "schemars")]
impl JsonSchema for Cnpj {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Cnpj")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "cnpj",
            "minLength": 14,
            "maxLength": 14,
            "pattern": "^[A-Z0-9]{12}[0-9]{2}$",
            "description": "Brazilian CNPJ (Cadastro Nacional da Pessoa Jurídica), unformatted, Módulo 11 checksum-valid."
        })
    }
}

/// A strategy producing structurally valid, checksum-correct [`Cnpj`] values, spanning both the
/// legacy numeric-only format and the alphanumeric format.
#[cfg(feature = "proptest")]
pub fn valid_cnpj() -> impl Strategy<Value = Cnpj> {
    prop::collection::vec(0..ALPHABET.len(), BASE_LEN).prop_map(|indices| {
        let mut base = [0u8; BASE_LEN];
        for (slot, idx) in base.iter_mut().zip(indices) {
            *slot = ALPHABET[idx];
        }
        avoid_all_repeated(&mut base);

        let (dv1, dv2) = compute_valid_check_digits(&base);
        let mut bytes = [0u8; 14];
        bytes[..BASE_LEN].copy_from_slice(&base);
        bytes[BASE_LEN] = dv1 + b'0';
        bytes[BASE_LEN + 1] = dv2 + b'0';

        Cnpj::from_bytes(bytes).expect("generated candidate is checksum-valid by construction")
    })
}

/// A strategy producing a valid [`Cnpj`] rendered with conventional `AA.AAA.AAA/AAAA-DD` punctuation,
/// useful for round-trip-through-formatting property tests.
#[cfg(feature = "proptest")]
pub fn valid_cnpj_formatted_string() -> impl Strategy<Value = String> {
    valid_cnpj().prop_map(|c| c.formatted().to_string())
}

#[cfg(feature = "arbitrary")]
impl<'a> Arbitrary<'a> for Cnpj {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let mut base = [0u8; BASE_LEN];
        for slot in base.iter_mut() {
            let idx = u32::from(u.arbitrary::<u8>()?) as usize % ALPHABET.len();
            *slot = ALPHABET[idx];
        }

        // Retry on the all-repeated-character candidate, which `from_bytes` would otherwise reject.
        avoid_all_repeated(&mut base);

        let (dv1, dv2) = compute_valid_check_digits(&base);
        let mut bytes = [0u8; 14];
        bytes[..BASE_LEN].copy_from_slice(&base);
        bytes[BASE_LEN] = dv1 + b'0';
        bytes[BASE_LEN + 1] = dv2 + b'0';

        Cnpj::from_bytes(bytes).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

#[cfg(test)]
mod tests_parsers {
    use crate::identifiers::CnpjError;
    use crate::identifiers::cnpj::normalize;

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize(""), Err(CnpjError::Empty));
    }

    #[test]
    fn strips_conventional_punctuation() {
        assert_eq!(normalize("12.345.678/0001-95"), normalize("12345678000195"));
    }

    #[test]
    fn strips_all_whitespace() {
        assert_eq!(
            normalize(" \t12.345.678/0001-95\n \r"),
            normalize("12345678000195")
        );
    }

    #[test]
    fn strips_non_ascii_whitespace() {
        assert_eq!(
            normalize("\u{00A0}12.345.678/0001-95\u{00A0}"),
            normalize("12345678000195")
        );
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(normalize("12abc34501de35").unwrap(), *b"12ABC34501DE35");
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            normalize("1234"),
            Err(CnpjError::InvalidLength { found: 4 })
        );
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("12ç45678000195").unwrap_err();
        assert!(matches!(
            err,
            CnpjError::InvalidCharacter {
                character: 'ç', ..
            }
        ));
    }
}

#[cfg(test)]
mod tests_validation {
    use crate::identifiers::CnpjError;
    use crate::identifiers::cnpj::{CharacterClass, validate};

    fn candidate(s: &str) -> [u8; 14] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 14];
        out.copy_from_slice(bytes);
        out
    }

    #[test]
    fn accepts_valid_legacy_numeric_cnpj() {
        // A well-known real CNPJ root (Banco do Brasil).
        assert!(validate(&candidate("00000000000191")).is_ok());
    }

    #[test]
    fn accepts_valid_alphanumeric_cnpj() {
        // Worked example from the official SERPRO technical note.
        assert!(validate(&candidate("12ABC34501DE35")).is_ok());
    }

    #[test]
    fn rejects_letter_in_check_digit_position() {
        let err = validate(&candidate("12ABC34501DEA5")).unwrap_err();
        assert_eq!(
            err,
            CnpjError::InvalidCharacter {
                character: 'A',
                position: 13,
                expected: CharacterClass::Digit,
            }
        );
    }

    #[test]
    fn rejects_symbol_in_base() {
        let err = validate(&candidate("12!BC34501DE35")).unwrap_err();
        assert_eq!(
            err,
            CnpjError::InvalidCharacter {
                character: '!',
                position: 3,
                expected: CharacterClass::Alphanumeric,
            }
        );
    }

    #[test]
    fn rejects_all_repeated_digit() {
        assert_eq!(
            validate(&candidate("11111111111111")).unwrap_err(),
            CnpjError::RepeatedDigits
        );
    }

    #[test]
    fn rejects_bad_checksum() {
        let err = validate(&candidate("00000000000192")).unwrap_err();
        assert_eq!(
            err,
            CnpjError::InvalidCheckDigits {
                position: 14,
                expected: 1,
                found: 2,
            }
        );
    }
}

#[cfg(test)]
mod tests_formated {
    use crate::identifiers::cnpj::Cnpj;
    use std::format;
    use std::string::ToString;

    #[test]
    fn formats_numeric_cnpj() {
        let cnpj = Cnpj::parse("00000000000191").unwrap();
        assert_eq!(cnpj.formatted().as_str(), "00.000.000/0001-91");
        assert_eq!(cnpj.to_string(), "00.000.000/0001-91");
    }

    #[test]
    fn formats_alphanumeric_cnpj() {
        let cnpj = Cnpj::parse("12ABC34501DE35").unwrap();
        assert_eq!(cnpj.formatted().as_str(), "12.ABC.345/01DE-35");
    }

    #[test]
    fn debug_is_readable() {
        let cnpj = Cnpj::parse("00000000000191").unwrap();
        assert_eq!(format!("{cnpj:?}"), "Cnpj(\"00.000.000/0001-91\")");
    }
}

#[cfg(all(test, feature = "arbitrary"))]
mod tests_arbitrary {
    use super::*;
    use arbitrary::Unstructured;

    #[test]
    fn always_produces_valid_cnpjs() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let cnpj = Cnpj::arbitrary(&mut u).expect("arbitrary should always succeed");

            // Re-validating via parse() proves the value round-trips
            // through the exact same checks a hand-typed input would.
            assert!(Cnpj::parse(cnpj.as_str()).is_ok());
        }
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests_serde {
    use crate::identifiers::cnpj::Cnpj;

    #[test]
    fn round_trips_through_json() {
        let cnpj = Cnpj::parse("00.000.000/0001-91").unwrap();
        let json = serde_json::to_string(&cnpj).unwrap();
        assert_eq!(json, "\"00000000000191\"");
        let back: Cnpj = serde_json::from_str(&json).unwrap();
        assert_eq!(cnpj, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<Cnpj>("\"not-a-cnpj\"").unwrap_err();
        assert!(err.to_string().contains("CNPJ"));
    }
}

#[cfg(all(test, feature = "proptest"))]
mod tests_proptest {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_cnpj_always_round_trips_through_parse(cnpj in valid_cnpj()) {
            let reparsed = Cnpj::parse(cnpj.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(cnpj, reparsed.unwrap());
        }

        #[test]
        fn formatted_string_always_round_trips(s in valid_cnpj_formatted_string()) {
            prop_assert!(Cnpj::parse(&s).is_ok());
        }
    }
}

#[cfg(all(test, feature = "schemars"))]
mod tests_schema {
    use crate::identifiers::cnpj::Cnpj;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(Cnpj);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "cnpj");
        assert_eq!(json["minLength"], 14);
        assert_eq!(json["maxLength"], 14);
        assert_eq!(json["pattern"], "^[A-Z0-9]{12}[0-9]{2}$");
    }
}
