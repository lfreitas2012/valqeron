use arbitrary::{Arbitrary, Unstructured};

use super::Cnpj;
use super::validation::{BASE_LEN, avoid_all_repeated, compute_valid_check_digits};

const ALPHABET: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

impl<'a> Arbitrary<'a> for Cnpj {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let mut base = [0u8; BASE_LEN];
        for slot in base.iter_mut() {
            let idx = u32::from(u.arbitrary::<u8>()?) as usize % ALPHABET.len();
            *slot = ALPHABET[idx];
        }

        // Retry on the all-repeated-character candidate, which `from_bytes` would otherwise reject.
        avoid_all_repeated(&mut base);

        let (dv1, dv2) = compute_valid_check_digits(&base);
        let mut bytes = [0u8; 14];
        bytes[..BASE_LEN].copy_from_slice(&base);
        bytes[BASE_LEN] = dv1 + b'0';
        bytes[BASE_LEN + 1] = dv2 + b'0';

        Cnpj::from_bytes(bytes).map_err(|_| arbitrary::Error::IncorrectFormat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_produces_valid_cnpjs() {
        for seed in 0u32..256 {
            let data = seed.to_le_bytes().repeat(8);
            let mut u = Unstructured::new(&data);
            let cnpj = Cnpj::arbitrary(&mut u).expect("arbitrary should always succeed");
            // Re-validating via parse() proves the value round-trips
            // through the exact same checks a hand-typed input would.
            assert!(Cnpj::parse(cnpj.as_str()).is_ok());
        }
    }
}
