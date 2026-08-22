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
//!   works in standard-library environments. Parsing, validation, and every accessor operate on the stack.
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
//! * `proptest`: exposes reusable `proptest` strategies (`valqeron_identifiers::country::proptest`,
//!   when this feature is enabled) for generating valid [`CountryCode`] values.
//!
//! # Error handling
//!
//! Every fallible constructor returns [`CountryCodeError`], which is `Clone + PartialEq + Eq` and
//! implements [`core::error::Error`] and [`Display`], so it composes with `?` and with
//! error aggregation crates alike:
//!
//! ```
//! use valqeron_identifiers::{CountryCode, CountryCodeError};
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
//! use valqeron_identifiers::CountryCode;
//!
//! let code = CountryCode::parse("us").unwrap(); // lowercase is folded automatically
//! assert_eq!(code.as_str(), "US");
//! assert_eq!(code.as_bytes(), b"US");
//! ```
//!
//! Sorting and deduplicating a batch of codes, for example after importing them from a spreadsheet:
//!
//! ```
//! use valqeron_identifiers::CountryCode;
//!
//! let mut codes: Vec<CountryCode> = ["US", "BR", "US"]
//!     .into_iter()
//!     .map(|s| CountryCode::parse(s).unwrap())
//!     .collect();
//! codes.sort();
//! codes.dedup();
//! assert_eq!(codes.len(), 2);
//! ```

use arbitrary::{Arbitrary, Unstructured};
use core::fmt;
use fmt::{Debug, Display};
use proptest::prelude::Strategy;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, de::Visitor};
use std::borrow::Cow;
use std::str::{FromStr, from_utf8_unchecked};

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
    /// use valqeron_identifiers::CountryCode;
    ///
    /// assert!(CountryCode::parse("US").is_ok());
    /// assert!(CountryCode::parse("us").is_ok()); // lowercase is folded automatically
    /// assert!(CountryCode::parse(" BR ").is_ok()); // surrounding whitespace is trimmed
    /// assert!(CountryCode::parse("ZZ").is_err()); // well formed but not assigned
    /// ```
    pub fn parse(input: &str) -> Result<Self, CountryCodeError> {
        let candidate = normalize(input)?;
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
    /// use valqeron_identifiers::CountryCode;
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
    /// use valqeron_identifiers::CountryCode;
    ///
    /// let code = CountryCode::from_bytes(*b"US").unwrap();
    /// assert_eq!(code.as_str(), "US");
    ///
    /// // A well formed but unassigned code is rejected just like it would be through `parse`.
    /// assert!(CountryCode::from_bytes(*b"ZZ").is_err());
    /// ```
    pub fn from_bytes(bytes: [u8; 2]) -> Result<Self, CountryCodeError> {
        validate(&bytes)?;
        Ok(CountryCode { bytes })
    }

    /// Returns the two raw ASCII bytes backing this code (for example, `b"US"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use valqeron_identifiers::CountryCode;
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
    /// use valqeron_identifiers::CountryCode;
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

impl Display for CountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Debug for CountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CountryCode").field(&self.as_str()).finish()
    }
}

/// Runs every validation rule against a normalized candidate, cheapest first:
///
/// 1. Character class: both positions must be uppercase ASCII letters.
/// 2. Membership: the two letters together must name an officially assigned code.
fn validate(candidate: &[u8; 2]) -> Result<(), CountryCodeError> {
    validate_character_classes(candidate)?;
    validate_membership(candidate)?;
    Ok(())
}

fn validate_character_classes(candidate: &[u8; 2]) -> Result<(), CountryCodeError> {
    for (i, &byte) in candidate.iter().enumerate() {
        if !byte.is_ascii_uppercase() {
            return Err(CountryCodeError::InvalidCharacter {
                character: byte as char,
                position: (i + 1) as u8,
            });
        }
    }
    Ok(())
}

fn validate_membership(candidate: &[u8; 2]) -> Result<(), CountryCodeError> {
    if is_assigned(candidate) {
        Ok(())
    } else {
        Err(CountryCodeError::Unassigned {
            code: [candidate[0] as char, candidate[1] as char],
        })
    }
}

/// Returns `true` when `candidate` names an officially assigned code.
///
/// The caller must have passed character class validation first, so both bytes are `b'A'..=b'Z'`
/// and the computed index is always within `ASSIGNED`.
#[inline]
fn is_assigned(candidate: &[u8; 2]) -> bool {
    debug_assert!(candidate[0].is_ascii_uppercase() && candidate[1].is_ascii_uppercase());
    let index = bit_index(*candidate);
    (ASSIGNED[index / 64] >> (index % 64)) & 1 == 1
}

/// The number of distinct two letter combinations, `26 * 26`.
const COMBINATIONS: usize = 26 * 26;

/// The number of 64 bit words needed to give every combination its own bit.
const WORDS: usize = COMBINATIONS.div_ceil(64);

/// The officially assigned ISO 3166-1 alpha-2 codes, in strictly ascending byte order.
///
/// Reserved codes (for example `EU` or `UK`), user assigned ranges, and codes that were once used
/// and later withdrawn are deliberately absent.
const ASSIGNED_CODES: &[[u8; 2]] = &[
    *b"AD", *b"AE", *b"AF", *b"AG", *b"AI", *b"AL", *b"AM", *b"AO", *b"AQ", *b"AR", //
    *b"AS", *b"AT", *b"AU", *b"AW", *b"AX", *b"AZ", *b"BA", *b"BB", *b"BD", *b"BE", //
    *b"BF", *b"BG", *b"BH", *b"BI", *b"BJ", *b"BL", *b"BM", *b"BN", *b"BO", *b"BQ", //
    *b"BR", *b"BS", *b"BT", *b"BV", *b"BW", *b"BY", *b"BZ", *b"CA", *b"CC", *b"CD", //
    *b"CF", *b"CG", *b"CH", *b"CI", *b"CK", *b"CL", *b"CM", *b"CN", *b"CO", *b"CR", //
    *b"CU", *b"CV", *b"CW", *b"CX", *b"CY", *b"CZ", *b"DE", *b"DJ", *b"DK", *b"DM", //
    *b"DO", *b"DZ", *b"EC", *b"EE", *b"EG", *b"EH", *b"ER", *b"ES", *b"ET", *b"FI", //
    *b"FJ", *b"FK", *b"FM", *b"FO", *b"FR", *b"GA", *b"GB", *b"GD", *b"GE", *b"GF", //
    *b"GG", *b"GH", *b"GI", *b"GL", *b"GM", *b"GN", *b"GP", *b"GQ", *b"GR", *b"GS", //
    *b"GT", *b"GU", *b"GW", *b"GY", *b"HK", *b"HM", *b"HN", *b"HR", *b"HT", *b"HU", //
    *b"ID", *b"IE", *b"IL", *b"IM", *b"IN", *b"IO", *b"IQ", *b"IR", *b"IS", *b"IT", //
    *b"JE", *b"JM", *b"JO", *b"JP", *b"KE", *b"KG", *b"KH", *b"KI", *b"KM", *b"KN", //
    *b"KP", *b"KR", *b"KW", *b"KY", *b"KZ", *b"LA", *b"LB", *b"LC", *b"LI", *b"LK", //
    *b"LR", *b"LS", *b"LT", *b"LU", *b"LV", *b"LY", *b"MA", *b"MC", *b"MD", *b"ME", //
    *b"MF", *b"MG", *b"MH", *b"MK", *b"ML", *b"MM", *b"MN", *b"MO", *b"MP", *b"MQ", //
    *b"MR", *b"MS", *b"MT", *b"MU", *b"MV", *b"MW", *b"MX", *b"MY", *b"MZ", *b"NA", //
    *b"NC", *b"NE", *b"NF", *b"NG", *b"NI", *b"NL", *b"NO", *b"NP", *b"NR", *b"NU", //
    *b"NZ", *b"OM", *b"PA", *b"PE", *b"PF", *b"PG", *b"PH", *b"PK", *b"PL", *b"PM", //
    *b"PN", *b"PR", *b"PS", *b"PT", *b"PW", *b"PY", *b"QA", *b"RE", *b"RO", *b"RS", //
    *b"RU", *b"RW", *b"SA", *b"SB", *b"SC", *b"SD", *b"SE", *b"SG", *b"SH", *b"SI", //
    *b"SJ", *b"SK", *b"SL", *b"SM", *b"SN", *b"SO", *b"SR", *b"SS", *b"ST", *b"SV", //
    *b"SX", *b"SY", *b"SZ", *b"TC", *b"TD", *b"TF", *b"TG", *b"TH", *b"TJ", *b"TK", //
    *b"TL", *b"TM", *b"TN", *b"TO", *b"TR", *b"TT", *b"TV", *b"TW", *b"TZ", *b"UA", //
    *b"UG", *b"UM", *b"US", *b"UY", *b"UZ", *b"VA", *b"VC", *b"VE", *b"VG", *b"VI", //
    *b"VN", *b"VU", *b"WF", *b"WS", *b"YE", *b"YT", *b"ZA", *b"ZM", *b"ZW",
];

/// The dense index of a two letter code in `0..676`.
///
/// The caller must pass two uppercase ASCII letters. Character class validation in
/// [`super::validation`] guarantees this before any lookup runs.
#[inline]
const fn bit_index(code: [u8; 2]) -> usize {
    (code[0] - b'A') as usize * 26 + (code[1] - b'A') as usize
}

/// The membership bitmap. Bit `bit_index(code)` is set exactly when `code` is officially assigned.
///
/// Derived from [`ASSIGNED_CODES`] at compile time.
const ASSIGNED: [u64; WORDS] = build_bitmap(ASSIGNED_CODES);

/// Folds a list of codes into a bitmap by setting one bit per code.
const fn build_bitmap(codes: &[[u8; 2]]) -> [u64; WORDS] {
    let mut bits = [0u64; WORDS];
    let mut i = 0;
    while i < codes.len() {
        let index = bit_index(codes[i]);
        bits[index / 64] |= 1u64 << (index % 64);
        i += 1;
    }
    bits
}

// Compile time guard. Any violation is a build error, so the table cannot regress unnoticed.
const _: () = check_table(ASSIGNED_CODES);

/// Asserts that every entry is two uppercase ASCII letters and that the list is strictly ascending.
///
/// Strict ascension gives two guarantees at once: the list is sorted, and it holds no duplicates.
const fn check_table(codes: &[[u8; 2]]) {
    let mut i = 0;
    while i < codes.len() {
        let a = codes[i][0];
        let b = codes[i][1];
        assert!(
            a >= b'A' && a <= b'Z' && b >= b'A' && b <= b'Z',
            "every assigned code must be two uppercase ASCII letters"
        );
        if i > 0 {
            let prev_a = codes[i - 1][0];
            let prev_b = codes[i - 1][1];
            assert!(
                prev_a < a || (prev_a == a && prev_b < b),
                "assigned codes must be listed in strictly ascending order"
            );
        }
        i += 1;
    }
}

/// The set of reasons a country code string can fail validation.
///
/// Every fallible constructor of [`CountryCode`](super::CountryCode) returns this type. A country
/// code carries no checksum. Its validity is defined by membership in the officially assigned ISO
/// 3166-1 alpha-2 set, so beyond the structural checks there is one assignment failure mode
/// ([`CountryCodeError::Unassigned`]). Each variant maps to a single, specific failure, so callers
/// can react programmatically rather than parsing a message.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountryCodeError {
    /// The input was an empty string.
    Empty,

    /// After trimming surrounding whitespace, the input did not contain exactly two characters.
    InvalidLength {
        /// The number of characters found after trimming.
        found: usize,
    },

    /// A character outside `A` to `Z` was found at a given position. Every position of a country
    /// code is an uppercase ASCII letter.
    InvalidCharacter {
        /// The offending character, as originally provided (before case folding).
        character: char,
        /// The 1 indexed position within the two characters.
        position: u8,
    },

    /// The two letters are well formed but are not a code that ISO 3166-1 officially assigns.
    Unassigned {
        /// The offending code, as two uppercase letters.
        code: [char; 2],
    },
}

impl Display for CountryCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CountryCodeError::Empty => f.write_str("country code input is empty"),
            CountryCodeError::InvalidLength { found } => {
                write!(
                    f,
                    "country code must contain exactly 2 characters, found {found}"
                )
            }
            CountryCodeError::InvalidCharacter {
                character,
                position,
            } => write!(
                f,
                "invalid character '{character}' at position {position} of 2: expected an uppercase letter (A to Z)"
            ),
            CountryCodeError::Unassigned { code } => write!(
                f,
                "'{}{}' is not an officially assigned ISO 3166-1 alpha-2 code",
                code[0], code[1]
            ),
        }
    }
}

impl core::error::Error for CountryCodeError {}

// ================================= PARSER =================================
/// Normalizes `input` into a two byte ASCII array.
///
/// The steps are:
///
/// * Empty input is rejected as [`CountryCodeError::Empty`].
/// * Leading and trailing whitespace is trimmed. Interior characters are left untouched.
/// * Remaining characters are ASCII uppercased, so a lowercase code is accepted transparently.
/// * Any non ASCII character, or a character count other than two after trimming, is rejected.
///
/// This function does not check the character class of each position, nor whether the two letters
/// name an assigned code. See [`super::validation::validate`] for that.
fn normalize(input: &str) -> Result<[u8; 2], CountryCodeError> {
    if input.is_empty() {
        return Err(CountryCodeError::Empty);
    }

    let trimmed = input.trim();
    let found = trimmed.chars().count();
    if found != 2 {
        return Err(CountryCodeError::InvalidLength { found });
    }

    let mut buf = [0u8; 2];
    for (i, ch) in trimmed.chars().enumerate() {
        if !ch.is_ascii() {
            return Err(CountryCodeError::InvalidCharacter {
                character: ch,
                position: (i + 1) as u8,
            });
        }
        buf[i] = ch.to_ascii_uppercase() as u8;
    }

    Ok(buf)
}

// ================================= SERDE =================================
impl Serialize for CountryCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct CountryCodeVisitor;

impl<'de> Visitor<'de> for CountryCodeVisitor {
    type Value = CountryCode;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 2-character ISO 3166-1 alpha-2 country code, e.g. US")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        CountryCode::parse(v).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for CountryCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(CountryCodeVisitor)
    }
}
// ================================= SCHEMARS, PROPTEST, ARBITRARY =================================
/// A strategy producing valid [`CountryCode`] values by picking from the officially assigned set.
pub fn valid_country_code() -> impl Strategy<Value=CountryCode> {
    (0..ASSIGNED_CODES.len()).prop_map(|index| {
        CountryCode::from_bytes(ASSIGNED_CODES[index])
            .expect("codes in the assigned set are valid by construction")
    })
}

/// A strategy producing a valid [`CountryCode`] rendered as its canonical two letter `String`,
/// useful for round trip through parsing property tests.
pub fn valid_country_code_string() -> impl Strategy<Value=String> {
    valid_country_code().prop_map(|code| code.as_str().to_string())
}

impl JsonSchema for CountryCode {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("CountryCode")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "iso3166-1-alpha2",
            "minLength": 2,
            "maxLength": 2,
            "pattern": "^[A-Z]{2}$",
            "description": "ISO 3166-1 alpha-2 country code. \
            The pattern is structural; membership in the assigned set is enforced on deserialization."
        })
    }
}

impl<'a> Arbitrary<'a> for CountryCode {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Pick straight from the assigned set so every generated value is valid by construction.
        let code = *u.choose(ASSIGNED_CODES)?;
        CountryCode::from_bytes(code).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

#[cfg(test)]
mod tests_formating {
    use crate::identifiers::country_code::CountryCode;
    use std::format;
    use std::string::ToString;

    #[test]
    fn display_is_the_canonical_string() {
        let code = CountryCode::parse("US").unwrap();
        assert_eq!(code.to_string(), "US");
    }

    #[test]
    fn debug_is_readable() {
        let code = CountryCode::parse("US").unwrap();
        assert_eq!(format!("{code:?}"), "CountryCode(\"US\")");
    }
}

#[cfg(test)]
mod tests_validation {
    use super::*;

    fn candidate(s: &str) -> [u8; 2] {
        let bytes = s.as_bytes();
        let mut out = [0u8; 2];
        out.copy_from_slice(bytes);
        out
    }

    #[test]
    fn accepts_known_assigned_codes() {
        for s in ["US", "BR", "GB", "DE", "SS", "CW", "AD", "ZW"] {
            assert!(validate(&candidate(s)).is_ok(), "{s} should be valid");
        }
    }

    #[test]
    fn rejects_unassigned_but_well_formed() {
        let err = validate(&candidate("ZZ")).unwrap_err();
        assert_eq!(err, CountryCodeError::Unassigned { code: ['Z', 'Z'] });
    }

    #[test]
    fn rejects_reserved_codes() {
        // `EU` and `UK` are reserved, not officially assigned, so they are treated as unassigned.
        assert!(matches!(
            validate(&candidate("EU")),
            Err(CountryCodeError::Unassigned { .. })
        ));
        assert!(matches!(
            validate(&candidate("UK")),
            Err(CountryCodeError::Unassigned { .. })
        ));
    }

    #[test]
    fn rejects_lowercase_as_character_class() {
        let err = validate(&candidate("us")).unwrap_err();
        assert_eq!(
            err,
            CountryCodeError::InvalidCharacter {
                character: 'u',
                position: 1,
            }
        );
    }

    #[test]
    fn rejects_digit_as_character_class() {
        let err = validate(&candidate("U1")).unwrap_err();
        assert_eq!(
            err,
            CountryCodeError::InvalidCharacter {
                character: '1',
                position: 2,
            }
        );
    }
}

#[cfg(test)]
mod tests_serde {
    use crate::identifiers::country_code::CountryCode;

    #[test]
    fn round_trips_through_json() {
        let code = CountryCode::parse("US").unwrap();
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"US\"");
        let back: CountryCode = serde_json::from_str(&json).unwrap();
        assert_eq!(code, back);
    }

    #[test]
    fn rejects_invalid_json_string() {
        let err = serde_json::from_str::<CountryCode>("\"ZZ\"").unwrap_err();
        assert!(err.to_string().contains("ISO 3166-1"));
    }
}

#[cfg(test)]
mod tests_parser {
    use super::*;

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize(""), Err(CountryCodeError::Empty));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(normalize("  US "), normalize("US"));
    }

    #[test]
    fn uppercases_letters() {
        assert_eq!(normalize("us").unwrap(), *b"US");
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            normalize("USA"),
            Err(CountryCodeError::InvalidLength { found: 3 })
        );
    }

    #[test]
    fn whitespace_only_is_a_length_error() {
        assert_eq!(
            normalize("   "),
            Err(CountryCodeError::InvalidLength { found: 0 })
        );
    }

    #[test]
    fn keeps_non_letter_characters_for_validation() {
        // A non letter that is not surrounding whitespace survives normalization (the count is
        // still two) and is left for validation to reject as an invalid character.
        assert_eq!(normalize("U.").unwrap(), *b"U.");
    }

    #[test]
    fn rejects_non_ascii() {
        let err = normalize("U£").unwrap_err();
        assert!(matches!(
            err,
            CountryCodeError::InvalidCharacter {
                character: '£', ..
            }
        ));
    }
}

#[cfg(test)]
mod tests_proptest {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        #[test]
        fn valid_country_code_always_round_trips_through_parse(code in valid_country_code()) {
            let reparsed = CountryCode::parse(code.as_str());
            prop_assert!(reparsed.is_ok());
            prop_assert_eq!(code, reparsed.unwrap());
        }

        #[test]
        fn valid_country_code_string_always_parses(s in valid_country_code_string()) {
            prop_assert!(CountryCode::parse(&s).is_ok());
        }
    }
}

#[cfg(test)]
mod tests_schemars {
    use crate::identifiers::country_code::CountryCode;
    use schemars::schema_for;

    #[test]
    fn schema_is_a_pattern_constrained_string() {
        let schema = schema_for!(CountryCode);
        let json = serde_json::to_value(&schema).unwrap();

        assert_eq!(json["type"], "string");
        assert_eq!(json["format"], "iso3166-1-alpha2");
        assert_eq!(json["minLength"], 2);
        assert_eq!(json["maxLength"], 2);
        assert_eq!(json["pattern"], "^[A-Z]{2}$");
    }
}

#[cfg(test)]
mod tests_arbitrary {
    use super::*;

    #[test]
    fn always_produces_assigned_codes() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let code = CountryCode::arbitrary(&mut u).expect("arbitrary should always succeed");
            // Re-validating via parse() proves the value round trips through the exact same checks
            // a hand typed input would.
            assert!(CountryCode::parse(code.as_str()).is_ok());
        }
    }
}
