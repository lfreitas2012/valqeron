#![no_main]

use libfuzzer_sys::arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;
use valqeron_core::identifiers::{CountryCode, CountryCodeError};

const LEN: usize = 2;
const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Full invariant oracle for a value the library accepted. Every property here must hold for any
/// valid `CountryCode`, no matter how it was produced.
fn check(value: CountryCode) {
    // `as_str` goes through `from_utf8_unchecked`; it must be sound and equal to the raw bytes.
    assert_eq!(std::str::from_utf8(value.as_bytes()), Ok(value.as_str()));

    // The canonical form always has the fixed length and stays inside the allowed byte class.
    assert_eq!(value.as_bytes().len(), LEN);
    assert_eq!(value.as_str().len(), LEN);
    assert!(value.as_bytes().iter().all(|b| b.is_ascii_uppercase()));

    // Every constructor agrees and parsing the canonical form is idempotent.
    assert_eq!(CountryCode::parse(value.as_str()), Ok(value));
    assert_eq!(CountryCode::new(value.as_str()), Ok(value));
    assert_eq!(CountryCode::from_bytes(*value.as_bytes()), Ok(value));
    assert_eq!(value.as_str().parse::<CountryCode>(), Ok(value));
    assert_eq!(CountryCode::try_from(value.as_str()), Ok(value));
    assert_eq!(CountryCode::try_from(*value.as_bytes()), Ok(value));
    assert_eq!(
        CountryCode::try_from(value.as_bytes().as_slice()),
        Ok(value)
    );

    // Equality and reference conversions agree with the canonical string/bytes.
    assert_eq!(value, *value.as_str());
    assert_eq!(value, value.as_str());
    assert_eq!(<CountryCode as AsRef<str>>::as_ref(&value), value.as_str());
    assert_eq!(
        <CountryCode as AsRef<[u8]>>::as_ref(&value),
        value.as_bytes()
    );

    // serde round-trips through the canonical string.
    let json = serde_json::to_string(&value).expect("serialize");
    assert_eq!(
        serde_json::from_str::<CountryCode>(&json).expect("deserialize"),
        value
    );

    // Rendering must never panic.
    let _ = format!("{value}");
    let _ = format!("{value:?}");
}

/// Builds a candidate with the exact canonical length and correct character class, so it survives
/// the length and character gates and forces the membership lookup to run on near-valid input.
fn shaped(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    (0..LEN)
        .map(|i| LETTERS[(data[i % data.len()] as usize) % LETTERS.len()] as char)
        .collect()
}

fuzz_target!(|data: &[u8]| {
    // 1. Arbitrary text: the parser must never panic, and any accepted value satisfies the oracle.
    if let Ok(text) = std::str::from_utf8(data) {
        match CountryCode::parse(text) {
            Ok(value) => check(value),
            // Formatting the error must never panic, and reported metadata must match the input.
            Err(err) => {
                let _ = err.to_string();
                if let CountryCodeError::InvalidLength { found } = err {
                    // `found` counts characters after trimming surrounding whitespace.
                    assert_eq!(found, text.trim().chars().count());
                    assert_ne!(found, LEN);
                }
            }
        }
    }

    // 2. Arbitrary raw bytes into `from_bytes`: never panic; accepted values stay sound.
    if data.len() >= LEN {
        let mut bytes = [0u8; LEN];
        bytes.copy_from_slice(&data[..LEN]);
        if let Ok(value) = CountryCode::from_bytes(bytes) {
            check(value);
        }
    }

    // 3. Structure-aware candidate: correct shape and class, so the membership logic runs on
    //    near-valid input rather than being rejected at the gate.
    if let Ok(value) = CountryCode::parse(&shaped(data)) {
        check(value);
    }

    // 4. Arbitrary-generated valid value: exercises the acceptance path deterministically.
    let mut unstructured = Unstructured::new(data);
    if let Ok(value) = CountryCode::arbitrary(&mut unstructured) {
        check(value);
    }
});
