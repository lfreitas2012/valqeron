use arbitrary::{Arbitrary, Unstructured};

use super::Lei;
use super::validation::{ALPHANUMERIC, BASE_LEN, build_valid_lei_bytes};

impl<'a> Arbitrary<'a> for Lei {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Positions 1-18: alphanumeric LOU prefix + entity-specific part.
        let mut base = [0usize; BASE_LEN];
        for slot in &mut base {
            *slot = u.arbitrary::<u8>()? as usize % ALPHANUMERIC.len();
        }

        let bytes = build_valid_lei_bytes(&base);

        Lei::from_bytes(bytes).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_produces_valid_leis() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let lei = Lei::arbitrary(&mut u).expect("arbitrary should always succeed");
            // Re-validating via parse() proves the value round-trips through the exact same
            // checks a hand-typed input would.
            assert!(Lei::parse(lei.as_str()).is_ok());
        }
    }
}
