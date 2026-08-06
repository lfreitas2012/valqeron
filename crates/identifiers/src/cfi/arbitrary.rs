use arbitrary::{Arbitrary, Unstructured};

use super::Cfi;
use super::table::CATEGORIES;
use super::validation::nth_letter;

impl<'a> Arbitrary<'a> for Cfi {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Walk the embedded taxonomy so every generated value is valid by construction: a category,
        // a group within it, and a permitted letter for each of the four attribute positions.
        let category = &CATEGORIES[u.arbitrary::<u8>()? as usize % CATEGORIES.len()];
        let group = &category.groups[u.arbitrary::<u8>()? as usize % category.groups.len()];

        let mut bytes = [0u8; 6];
        bytes[0] = category.code;
        bytes[1] = group.code;
        for (i, &mask) in group.attrs.iter().enumerate() {
            let selector = u.arbitrary::<u8>()? as usize;
            bytes[2 + i] = nth_letter(mask, selector);
        }

        Cfi::from_bytes(bytes).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_produces_valid_cfis() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let cfi = Cfi::arbitrary(&mut u).expect("arbitrary should always succeed");
            // Re-validating via parse() proves the value round-trips through the exact same checks
            // a hand-typed input would.
            assert!(Cfi::parse(cfi.as_str()).is_ok());
        }
    }
}
