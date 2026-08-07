//! `Display`/`Debug` for [`Isin`].
//!
//! Unlike a CNPJ, an ISIN has no conventional punctuated form: its canonical rendering *is* the
//! compact 12-character string. There is therefore no separate zero-allocation formatted-string
//! helper — [`Isin::as_str`](crate::isin::Isin::as_str) already returns the canonical form.

use crate::isin::Isin;
use core::fmt;

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

#[cfg(test)]
mod tests {
    use crate::isin::Isin;
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
