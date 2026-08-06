//! `Display`/`Debug` for [`Cnpj`] and the zero-allocation [`FormattedCnpj`] helper returned by [`Cnpj::formatted`].

use crate::cnpj::Cnpj;
use core::fmt;
use core::ops::Deref;

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

#[cfg(test)]
mod tests {
    use crate::cnpj::Cnpj;
    use alloc::format;
    use alloc::string::ToString;

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
