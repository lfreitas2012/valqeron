#![no_main]

use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use valqeron_core::identifiers::Cnpj;

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    // 1. Construct a mutated string with randomly interspersed formatting characters
    let mut raw_bytes = Vec::new();
    let num_chars: usize = match u.int_in_range(0..=20) {
        Ok(n) => n,
        Err(_) => return,
    };

    for _ in 0..num_chars {
        // Interleave random formatting punctuation/spaces with raw payload
        if u.ratio(1, 4).unwrap_or(false) {
            let punct = match u.int_in_range(0..=4).unwrap_or(0) {
                0 => b'.',
                1 => b'/',
                2 => b'-',
                3 => b' ',
                _ => b'\t',
            };
            raw_bytes.push(punct);
        }

        if let Ok(b) = u.arbitrary::<u8>() {
            // Randomly flip casing if it's an ASCII letter
            let byte = if u.ratio(1, 2).unwrap_or(false) && b.is_ascii_alphabetic() {
                b.to_ascii_lowercase()
            } else {
                b
            };
            raw_bytes.push(byte);
        }
    }

    let input_str = match std::str::from_utf8(&raw_bytes) {
        Ok(s) => s,
        Err(_) => return,
    };

    // 2. INVARIANT ORACLE: Parsing must never panic, and if valid, formatted output must roundtrip
    if let Ok(cnpj) = Cnpj::parse(input_str) {
        let compact = cnpj.as_str();
        let formatted = cnpj.formatted().to_string();

        // Compact representation must strictly be 14 ASCII alphanumeric characters
        assert_eq!(compact.len(), 14);
        assert!(
            compact
                .chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
        );

        // Roundtrip invariant: Re-parsing both representations must be identical
        let re_compact = Cnpj::parse(compact).expect("compact roundtrip failed");
        let re_formatted = Cnpj::parse(&formatted).expect("formatted roundtrip failed");

        assert_eq!(cnpj, re_compact);
        assert_eq!(cnpj, re_formatted);
    }
});
