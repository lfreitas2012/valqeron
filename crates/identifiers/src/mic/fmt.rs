//! `Display` and `Debug` for [`Mic`].
//!
//! A MIC has no punctuated form. Its canonical rendering is the compact four character string,
//! which [`Mic::as_str`](Mic::as_str) already returns.

use crate::mic::Mic;
use core::fmt;

impl fmt::Display for Mic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Mic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Mic").field(&self.as_str()).finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::mic::Mic;
    use std::format;
    use std::string::ToString;

    #[test]
    fn display_is_the_canonical_string() {
        let mic = Mic::parse("XNYS").unwrap();
        assert_eq!(mic.to_string(), "XNYS");
    }

    #[test]
    fn debug_is_readable() {
        let mic = Mic::parse("XNYS").unwrap();
        assert_eq!(format!("{mic:?}"), "Mic(\"XNYS\")");
    }
}
