#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use valqeron_identifiers::{Lei, LeiError};
use libfuzzer_sys::fuzz_target;

const LEN: usize = 20;
const BASE_LEN: usize = 18;
const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const DIGITS: &[u8] = b"0123456789";

/// Full invariant oracle for a value the library accepted.
fn check(value: Lei) {
    // `as_str` goes through `from_utf8_unchecked`; it must be sound and equal to the raw bytes.
    assert_eq!(std::str::from_utf8(value.as_bytes()), Ok(value.as_str()));

    // Fixed length and per-position character classes.
    assert_eq!(value.as_bytes().len(), LEN);
    assert_eq!(value.as_str().len(), LEN);
    let bytes = value.as_bytes();
    assert!(bytes[..BASE_LEN].iter().all(|b| b.is_ascii_alphanumeric()));
    assert!(bytes[BASE_LEN..].iter().all(|b| b.is_ascii_digit()));

    // Accessors are consistent with the canonical string, and the stored MOD 97-10 digits match the
    // recomputed ones (this would catch any regression in the checksum implementation).
    assert_eq!(value.lou_prefix(), &value.as_str()[0..4]);
    assert_eq!(value.entity_id(), &value.as_str()[4..18]);
    assert_eq!(value.check_digits(), value.computed_check_digits());
    assert!(value.check_digits() < 100);

    // Every constructor agrees and parsing the canonical form is idempotent.
    assert_eq!(Lei::parse(value.as_str()), Ok(value));
    assert_eq!(Lei::new(value.as_str()), Ok(value));
    assert_eq!(Lei::from_bytes(*value.as_bytes()), Ok(value));
    assert_eq!(value.as_str().parse::<Lei>(), Ok(value));
    assert_eq!(Lei::try_from(value.as_str()), Ok(value));
    assert_eq!(Lei::try_from(*value.as_bytes()), Ok(value));
    assert_eq!(Lei::try_from(value.as_bytes().as_slice()), Ok(value));

    // Equality and reference conversions agree with the canonical string/bytes.
    assert_eq!(value, *value.as_str());
    assert_eq!(value, value.as_str());
    assert_eq!(<Lei as AsRef<str>>::as_ref(&value), value.as_str());
    assert_eq!(<Lei as AsRef<[u8]>>::as_ref(&value), value.as_bytes());

    // serde round-trips through the canonical string.
    let json = serde_json::to_string(&value).expect("serialize");
    assert_eq!(serde_json::from_str::<Lei>(&json).expect("deserialize"), value);

    // Rendering must never panic.
    let _ = format!("{value}");
    let _ = format!("{value:?}");
}

/// Builds a candidate with the correct per-position character classes (eighteen alphanumeric, two
/// digits), so the MOD 97-10 checksum runs on near-valid input every time.
fn shaped(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let pick = |set: &[u8], i: usize| set[(data[i % data.len()] as usize) % set.len()] as char;
    let mut out = String::with_capacity(LEN);
    for i in 0..BASE_LEN {
        out.push(pick(ALPHANUMERIC, i));
    }
    out.push(pick(DIGITS, BASE_LEN));
    out.push(pick(DIGITS, BASE_LEN + 1));
    out
}

fuzz_target!(|data: &[u8]| {
    // 1. Arbitrary text: the parser must never panic, and any accepted value satisfies the oracle.
    if let Ok(text) = std::str::from_utf8(data) {
        match Lei::parse(text) {
            Ok(value) => check(value),
            // Formatting the error must never panic, and reported metadata must match the input.
            Err(err) => {
                let _ = err.to_string();
                if let LeiError::InvalidLength { found } = err {
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
        if let Ok(value) = Lei::from_bytes(bytes) {
            check(value);
        }
    }

    // 3. Structure-aware candidate: correct per-position classes, so the MOD 97-10 checksum runs on
    //    near-valid input rather than being rejected at the character gate.
    if let Ok(value) = Lei::parse(&shaped(data)) {
        check(value);
    }

    // 4. Arbitrary-generated valid value: exercises the acceptance path deterministically.
    let mut unstructured = Unstructured::new(data);
    if let Ok(value) = Lei::arbitrary(&mut unstructured) {
        check(value);
    }
});
