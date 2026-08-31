#![no_main]

use libfuzzer_sys::fuzz_target;
use std::str::FromStr;
use valqeron_core::identifiers::Cnpj;

fuzz_target!(|data: &[u8]| {
    if data.len() < 12 {
        return;
    }

    // 1. Force the first 12 bytes into valid ASCII digits
    let mut bytes = [0u8; 14];
    for i in 0..12 {
        bytes[i] = b'0' + (data[i] % 10);
    }

    // 2. Synthesize correct check digits so `Cnpj::parse` always succeeds
    bytes[12] = b'0' + reference_modulo11(&bytes[..12], &[5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]);
    bytes[13] = b'0' + reference_modulo11(&bytes[..13], &[6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]);
    let valid_str = std::str::from_utf8(&bytes).unwrap();

    // 3. EXERCISE ALL UNCOVERED TRAITS AND WRAPPERS
    let cnpj = Cnpj::new(valid_str).expect("Valid string must parse");
    let _ = cnpj.check_digits(); // Guaranteed to run now

    let _ = Cnpj::from_str(valid_str).unwrap();
    let _ = valid_str.parse::<Cnpj>().unwrap();

    let _ = Cnpj::try_from(valid_str).unwrap();
    let _ = Cnpj::try_from(bytes).unwrap();
    let _ = Cnpj::try_from(&bytes[..]).unwrap();

    // 4. EXERCISE COMMUTATIVE EQUALITY & REFERENCES
    assert_eq!(cnpj, valid_str);
    assert_eq!(valid_str, cnpj);
    assert_eq!(*valid_str, cnpj);

    let _: &[u8] = cnpj.as_ref();
    let _: &str = cnpj.as_ref();
});

fn reference_modulo11(digits: &[u8], weights: &[u32]) -> u8 {
    let sum: u32 = digits
        .iter()
        .zip(weights)
        .map(|(&b, &w)| (b - b'0') as u32 * w)
        .sum();
    let rem = sum % 11;
    if rem < 2 { 0 } else { (11 - rem) as u8 }
}
