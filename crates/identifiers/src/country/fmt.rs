//! `Display` and `Debug` for [`CountryCode`].
//!
//! A country code has no punctuated form. Its canonical rendering is the compact two letter string,
//! which [`CountryCode::as_str`](CountryCode::as_str) already returns.

use crate::country::CountryCode;
use core::fmt;

impl fmt::Display for CountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for CountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CountryCode").field(&self.as_str()).finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::country::CountryCode;
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
