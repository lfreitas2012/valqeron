//! Interim currency identifier type.
//!
//! `CurrencyCode` (ISO 4217) is a planned addition to the `valqeron-identifiers`
//! crate. Until it lands there, core hosts this format-validated implementation
//! with the same API shape as that crate's types (byte-backed, `Copy`,
//! `new`/`parse`/`as_str`/`as_bytes`/`FromStr`, one error enum per type) so the
//! migration is a re-export swap in `lib.rs` — exactly how `Mic` moved once
//! the registry-validated implementation shipped.
//!
//! Validation is format-only (length and character class, with whitespace
//! trimming and ASCII case normalization); the official ISO 4217 registry is
//! not checked.

use std::fmt;
use std::str::FromStr;

const CURRENCY_CODE_LEN: usize = 3;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CurrencyCodeError {
    #[error("currency code cannot be empty")]
    Empty,

    #[error("currency code must be exactly {expected} characters", expected = CURRENCY_CODE_LEN)]
    InvalidLength,

    #[error("currency code must contain only ASCII letters")]
    InvalidCharacter,
}

/// ISO 4217 alphabetic currency code, e.g. `BRL` or `USD`.
///
/// Stored as normalized uppercase ASCII.
#[derive(Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord, Debug)]
pub struct CurrencyCode([u8; CURRENCY_CODE_LEN]);

impl CurrencyCode {
    /// Parses a currency code, trimming surrounding whitespace and normalizing
    /// to uppercase ASCII.
    pub fn parse(input: &str) -> Result<Self, CurrencyCodeError> {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return Err(CurrencyCodeError::Empty);
        }
        if trimmed.chars().count() != CURRENCY_CODE_LEN {
            return Err(CurrencyCodeError::InvalidLength);
        }
        if !trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(CurrencyCodeError::InvalidCharacter);
        }

        let mut bytes = [0u8; CURRENCY_CODE_LEN];
        for (slot, byte) in bytes.iter_mut().zip(trimmed.bytes()) {
            *slot = byte.to_ascii_uppercase();
        }
        Ok(Self(bytes))
    }

    /// Alias for [`CurrencyCode::parse`], mirroring the `valqeron-identifiers`
    /// API.
    pub fn new(input: &str) -> Result<Self, CurrencyCodeError> {
        Self::parse(input)
    }

    pub fn as_str(&self) -> &str {
        // The constructor guarantees uppercase ASCII, so this never falls back.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    pub fn as_bytes(&self) -> &[u8; CURRENCY_CODE_LEN] {
        &self.0
    }
}

impl FromStr for CurrencyCode {
    type Err = CurrencyCodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_code_parses_and_normalizes() {
        let currency_result = CurrencyCode::parse(" brl ");
        assert!(currency_result.is_ok());
        let Some(currency) = currency_result.ok() else {
            return;
        };
        assert_eq!(currency.as_str(), "BRL");
        assert_eq!(currency.as_bytes(), b"BRL");
        assert_eq!(currency.to_string(), "BRL");
    }

    #[test]
    fn currency_code_rejects_empty() {
        assert!(matches!(
            CurrencyCode::parse(""),
            Err(CurrencyCodeError::Empty)
        ));
    }

    #[test]
    fn currency_code_rejects_wrong_length() {
        assert!(matches!(
            CurrencyCode::parse("US"),
            Err(CurrencyCodeError::InvalidLength)
        ));
        assert!(matches!(
            CurrencyCode::parse("USDT"),
            Err(CurrencyCodeError::InvalidLength)
        ));
    }

    #[test]
    fn currency_code_rejects_non_letters() {
        assert!(matches!(
            CurrencyCode::parse("US1"),
            Err(CurrencyCodeError::InvalidCharacter)
        ));
    }

    #[test]
    fn currency_code_from_str_round_trips() {
        let currency_result = "usd".parse::<CurrencyCode>();
        assert!(matches!(currency_result, Ok(c) if c.as_str() == "USD"));
    }
}
