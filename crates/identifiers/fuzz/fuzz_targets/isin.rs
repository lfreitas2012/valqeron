#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use valqeron_identifiers::{CountryCode, Isin, IsinError};
use libfuzzer_sys::fuzz_target;

const LEN: usize = 12;
const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const DIGITS: &[u8] = b"0123456789";

/// Full invariant oracle for a value the library accepted.
fn check(value: Isin) {
    // `as_str` goes through `from_utf8_unchecked`; it must be sound and equal to the raw bytes.
    assert_eq!(std::str::from_utf8(value.as_bytes()), Ok(value.as_str()));

    // Fixed length and per-position character classes.
    assert_eq!(value.as_bytes().len(), LEN);
    assert_eq!(value.as_str().len(), LEN);
    let bytes = value.as_bytes();
    assert!(bytes[0].is_ascii_uppercase() && bytes[1].is_ascii_uppercase());
    assert!(bytes[2..11].iter().all(|b| b.is_ascii_alphanumeric()));
    assert!(bytes[11].is_ascii_digit());

    // Accessors are consistent with the canonical string, and the stored Luhn digit matches the
    // recomputed one (this would catch any regression in the checksum implementation).
    assert_eq!(value.country_code(), &value.as_str()[0..2]);
    assert_eq!(value.nsin(), &value.as_str()[2..11]);
    assert_eq!(value.check_digit(), value.computed_check_digit());
    assert!(value.check_digit() < 10);

    // `country()` is the ISO 3166-1 cross-validation of the prefix. When present it must agree
    // with the raw prefix, and it must be exactly what `CountryCode::parse` accepts for it.
    let country = value.country();
    assert_eq!(country, CountryCode::parse(value.country_code()).ok());
    if let Some(cc) = country {
        assert_eq!(cc.as_str(), value.country_code());
    }

    // Every constructor agrees and parsing the canonical form is idempotent.
    assert_eq!(Isin::parse(value.as_str()), Ok(value));
    assert_eq!(Isin::new(value.as_str()), Ok(value));
    assert_eq!(Isin::from_bytes(*value.as_bytes()), Ok(value));
    assert_eq!(value.as_str().parse::<Isin>(), Ok(value));
    assert_eq!(Isin::try_from(value.as_str()), Ok(value));
    assert_eq!(Isin::try_from(*value.as_bytes()), Ok(value));
    assert_eq!(Isin::try_from(value.as_bytes().as_slice()), Ok(value));

    // Equality and reference conversions agree with the canonical string/bytes.
    assert_eq!(value, *value.as_str());
    assert_eq!(value, value.as_str());
    assert_eq!(<Isin as AsRef<str>>::as_ref(&value), value.as_str());
    assert_eq!(<Isin as AsRef<[u8]>>::as_ref(&value), value.as_bytes());

    // serde round-trips through the canonical string.
    let json = serde_json::to_string(&value).expect("serialize");
    assert_eq!(
        serde_json::from_str::<Isin>(&json).expect("deserialize"),
        value
    );

    // Rendering must never panic.
    let _ = format!("{value}");
    let _ = format!("{value:?}");
}

/// Builds a candidate with the correct per-position character classes (two letters, nine
/// alphanumeric, one digit), so the Luhn checksum runs on near-valid input every time.
fn shaped(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let pick = |set: &[u8], i: usize| set[(data[i % data.len()] as usize) % set.len()] as char;
    let mut out = String::with_capacity(LEN);
    out.push(pick(LETTERS, 0));
    out.push(pick(LETTERS, 1));
    for i in 2..11 {
        out.push(pick(ALPHANUMERIC, i));
    }
    out.push(pick(DIGITS, 11));
    out
}

fuzz_target!(|data: &[u8]| {
    // 1. Arbitrary text: the parser must never panic, and any accepted value satisfies the oracle.
    if let Ok(text) = std::str::from_utf8(data) {
        match Isin::parse(text) {
            Ok(value) => check(value),
            // Formatting the error must never panic, and reported metadata must match the input.
            Err(err) => {
                let _ = err.to_string();
                if let IsinError::InvalidLength { found } = err {
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
        if let Ok(value) = Isin::from_bytes(bytes) {
            check(value);
        }
    }

    // 3. Structure-aware candidate: correct per-position classes, so the Luhn checksum runs on
    //    near-valid input rather than being rejected at the character gate.
    if let Ok(value) = Isin::parse(&shaped(data)) {
        check(value);
    }

    // 4. Arbitrary-generated valid value: exercises the acceptance path deterministically.
    let mut unstructured = Unstructured::new(data);
    if let Ok(value) = Isin::arbitrary(&mut unstructured) {
        check(value);
    }
});
