//! `Display`/`Debug` for [`Lei`].
//!
//! Like an ISIN (and unlike a CNPJ), an LEI has no conventional punctuated form: its canonical
//! rendering *is* the compact 20-character string. There is therefore no separate zero-allocation
//! formatted-string helper [`Lei::as_str`](Lei::as_str) already returns the canonical form.

use crate::lei::Lei;
use core::fmt;

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

#[cfg(test)]
mod tests {
    use crate::lei::Lei;
    use alloc::format;
    use alloc::string::ToString;

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
