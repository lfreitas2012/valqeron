use super::Isin;
use super::validation::{ALPHANUMERIC, BASE_LEN, LETTERS, build_valid_isin_bytes};
use crate::CountryCode;
use arbitrary::{Arbitrary, Unstructured};

impl<'a> Arbitrary<'a> for Isin {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Positions 1-2: Generate a valid CountryCode first to ensure
        // we only use assigned ISO 3166-1 alpha-2 prefixes.
        let country_code = u.arbitrary::<CountryCode>()?;
        let cc_bytes = country_code.as_bytes();

        // Map the selected country code's bytes back into LETTERS indices.
        let country = [
            LETTERS
                .iter()
                .position(|&b| b == cc_bytes[0])
                .expect("valid letter"),
            LETTERS
                .iter()
                .position(|&b| b == cc_bytes[1])
                .expect("valid letter"),
        ];

        // Positions 3-11: alphanumeric NSIN.
        let mut nsin = [0usize; BASE_LEN - 2];
        for slot in &mut nsin {
            *slot = u.arbitrary::<u8>()? as usize % ALPHANUMERIC.len();
        }

        let bytes = build_valid_isin_bytes(country, &nsin);

        Isin::from_bytes(bytes).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_produces_valid_isins() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let isin = Isin::arbitrary(&mut u).expect("arbitrary should always succeed");
            // Re-validating via parse() proves the value round-trips through the exact same
            // checks a hand-typed input would.
            assert!(Isin::parse(isin.as_str()).is_ok());
        }
    }
}
