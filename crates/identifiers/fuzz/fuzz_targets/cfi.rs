#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;
use valqeron_identifiers::{Cfi, CfiError};

const LEN: usize = 6;
const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Full invariant oracle for a value the library accepted.
fn check(value: Cfi) {
    // `as_str` goes through `from_utf8_unchecked`; it must be sound and equal to the raw bytes.
    assert_eq!(std::str::from_utf8(value.as_bytes()), Ok(value.as_str()));

    // Fixed length and all uppercase letters.
    assert_eq!(value.as_bytes().len(), LEN);
    assert_eq!(value.as_str().len(), LEN);
    assert!(value.as_bytes().iter().all(|b| b.is_ascii_uppercase()));

    // Accessors are consistent with the canonical string.
    let chars: Vec<char> = value.as_str().chars().collect();
    assert_eq!(value.category(), chars[0]);
    assert_eq!(value.group(), chars[1]);
    assert_eq!(value.attributes(), [chars[2], chars[3], chars[4], chars[5]]);

    // Every constructor agrees and parsing the canonical form is idempotent.
    assert_eq!(Cfi::parse(value.as_str()), Ok(value));
    assert_eq!(Cfi::new(value.as_str()), Ok(value));
    assert_eq!(Cfi::from_bytes(*value.as_bytes()), Ok(value));
    assert_eq!(value.as_str().parse::<Cfi>(), Ok(value));
    assert_eq!(Cfi::try_from(value.as_str()), Ok(value));
    assert_eq!(Cfi::try_from(*value.as_bytes()), Ok(value));
    assert_eq!(Cfi::try_from(value.as_bytes().as_slice()), Ok(value));

    // Equality and reference conversions agree with the canonical string/bytes.
    assert_eq!(value, *value.as_str());
    assert_eq!(value, value.as_str());
    assert_eq!(<Cfi as AsRef<str>>::as_ref(&value), value.as_str());
    assert_eq!(<Cfi as AsRef<[u8]>>::as_ref(&value), value.as_bytes());

    // serde round-trips through the canonical string.
    let json = serde_json::to_string(&value).expect("serialize");
    assert_eq!(
        serde_json::from_str::<Cfi>(&json).expect("deserialize"),
        value
    );

    // Rendering must never panic.
    let _ = format!("{value}");
    let _ = format!("{value:?}");
}

/// Builds a candidate with the exact canonical length and correct character class (six letters), so
/// the taxonomy lookup runs on near-valid input rather than being rejected at the gate.
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
        match Cfi::parse(text) {
            Ok(value) => check(value),
            // Formatting the error must never panic, and reported metadata must match the input.
            Err(err) => {
                let _ = err.to_string();
                if let CfiError::InvalidLength { found } = err {
                    // `found` counts characters after trimming surrounding Unicode whitespace.
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
        if let Ok(value) = Cfi::from_bytes(bytes) {
            check(value);
        }
    }

    // 3. Structure-aware candidate: correct shape and class, so the taxonomy logic runs on
    //    near-valid input rather than being rejected at the gate.
    if let Ok(value) = Cfi::parse(&shaped(data)) {
        check(value);
    }

    // 4. Arbitrary-generated valid value: exercises the acceptance path deterministically.
    let mut unstructured = Unstructured::new(data);
    if let Ok(value) = Cfi::arbitrary(&mut unstructured) {
        check(value);
    }
});
