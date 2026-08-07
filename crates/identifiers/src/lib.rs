//! Validated identifier types.
//!
//! This crate provides small, `Copy`, allocation-free value types that can only ever hold a valid
//! identifier. Every constructor runs full validation up front, so there is no partially validated
//! state: if you hold one of these types, it is valid.
//!
//! # Identifiers
//!
//! * [`Cnpj`]: Brazil's Cadastro Nacional da Pessoa Jurídica, the national registry identifier for
//!   legal entities. It supports the punctuated `AA.AAA.AAA/AAAA-DD` form, the compact 14-character
//!   form, the legacy numeric layout, and the 2026 alphanumeric layout, validated with the Módulo
//!   11 checksum.
//! * [`Isin`]: the ISO 6166 International Securities Identification Number, a 12-character
//!   securities identifier validated with the ISO 6166 Luhn check digit.
//! * [`Cfi`]: the ISO 10962 Classification of Financial Instruments code, a six-letter taxonomy
//!   code validated against an embedded copy of the standard's code table.
//! * [`CountryCode`]: the ISO 3166-1 alpha-2 country code, two letters validated against the
//!   officially assigned set.
//! * [`Lei`]: the ISO 17442 Legal Entity Identifier, a 20-character alphanumeric code validated
//!   with the ISO/IEC 7064 MOD 97-10 check digits.
//! * [`Mic`]: the ISO 10383 Market Identifier Code, a 4-character exchange and trading venue code
//!   validated against an embedded snapshot of the official registry.
//!
//! # Design
//!
//! * No invalid state is representable. Every constructor runs the full validation rules and
//!   returns a typed error on failure. There is no unchecked public constructor.
//! * Zero allocation and `Copy`. Each type wraps a fixed size byte array, and parsing, validation,
//!   and every accessor operate on the stack.
//! * Consistent ordering and hashing. Ordering and hashing operate over the raw ASCII bytes and
//!   match [`str`] ordering on the string accessor, so every type works as a map or set key.
//!
//! # Feature flags
//!
//! The optional integrations are off by default and purely additive. Enabling one never changes the
//! behavior of the parsers or the validation rules:
//!
//! * `serde`: (de)serializes each type as its canonical string. Deserialization re-runs full
//!   validation.
//! * `schemars`: implements `JsonSchema` for each type. Implies `serde`.
//! * `arbitrary`: implements `Arbitrary` for each type, generating valid values for fuzz targets.
//! * `proptest`: exposes reusable `proptest` strategies for generating valid values.
//!
//! # Example
//!
//! ```
//! use valqeron_identifiers::{Cnpj, Isin, Cfi, CountryCode, Lei, Mic};
//!
//! let cnpj = Cnpj::parse("00.000.000/0001-91").unwrap();
//! assert_eq!(cnpj.as_str(), "00000000000191");
//!
//! let isin = Isin::parse("US0378331005").unwrap();
//! assert_eq!(isin.country_code(), "US");
//!
//! let cfi = Cfi::parse("ESVUFR").unwrap();
//! assert_eq!(cfi.category(), 'E');
//!
//! let country = CountryCode::parse("br").unwrap();
//! assert_eq!(country.as_str(), "BR");
//!
//! let lei = Lei::parse("5493000IBP32UQZ0KL24").unwrap();
//! assert_eq!(lei.lou_prefix(), "5493");
//!
//! let mic = Mic::parse("xnys").unwrap();
//! assert_eq!(mic.as_str(), "XNYS");
//! ```
#![warn(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
// These identifier implementations operate on fixed-size, validated ASCII byte arrays. The
// indexing, arithmetic, and conversions below are guarded by the parser/validator invariants.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::string_slice,
    clippy::unreachable,
    clippy::unwrap_used
)]

pub mod cnpj;
#[doc(inline)]
pub use cnpj::{Cnpj, CnpjError, FormattedCnpj};

pub mod isin;
#[doc(inline)]
pub use isin::{Isin, IsinError};

pub mod cfi;
#[doc(inline)]
pub use cfi::{Cfi, CfiError};

pub mod country;
#[doc(inline)]
pub use country::{CountryCode, CountryCodeError};

pub mod lei;
#[doc(inline)]
pub use lei::{Lei, LeiError};

pub mod mic;
#[doc(inline)]
pub use mic::{Mic, MicError};
