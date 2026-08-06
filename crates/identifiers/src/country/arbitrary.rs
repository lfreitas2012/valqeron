use arbitrary::{Arbitrary, Unstructured};

use super::CountryCode;
use super::table::ASSIGNED_CODES;

impl<'a> Arbitrary<'a> for CountryCode {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Pick straight from the assigned set so every generated value is valid by construction.
        let code = *u.choose(ASSIGNED_CODES)?;
        CountryCode::from_bytes(code).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_produces_assigned_codes() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let code = CountryCode::arbitrary(&mut u).expect("arbitrary should always succeed");
            // Re-validating via parse() proves the value round trips through the exact same checks
            // a hand typed input would.
            assert!(CountryCode::parse(code.as_str()).is_ok());
        }
    }
}
