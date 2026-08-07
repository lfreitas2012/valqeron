use arbitrary::{Arbitrary, Unstructured};

use super::Mic;
use super::table::ENTRIES;

impl<'a> Arbitrary<'a> for Mic {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Pick straight from the registry so every generated value is valid by construction.
        let entry = u.choose(ENTRIES)?;
        Mic::from_bytes(entry.code).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_produces_registered_codes() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let mic = Mic::arbitrary(&mut u).expect("arbitrary should always succeed");
            // Re-validating via parse() proves the value round trips through the exact same checks
            // a hand-typed input would.
            assert!(Mic::parse(mic.as_str()).is_ok());
        }
    }
}
