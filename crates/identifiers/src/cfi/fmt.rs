//! `Display`/`Debug` for [`Cfi`].
//!
//! A CFI has no conventional punctuated form: its canonical rendering *is* the
//! compact 6-character string. There is therefore no separate zero-allocation formatted-string
//! helper — [`Cfi::as_str`](Cfi::as_str) already returns the canonical form.

use crate::cfi::Cfi;
use core::fmt;

impl fmt::Display for Cfi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Cfi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Cfi").field(&self.as_str()).finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::cfi::Cfi;
    use alloc::format;
    use alloc::string::ToString;

    #[test]
    fn display_is_the_canonical_string() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        assert_eq!(cfi.to_string(), "ESVUFR");
    }

    #[test]
    fn debug_is_readable() {
        let cfi = Cfi::parse("ESVUFR").unwrap();
        assert_eq!(format!("{cfi:?}"), "Cfi(\"ESVUFR\")");
    }
}
