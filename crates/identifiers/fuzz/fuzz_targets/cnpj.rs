#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use valqeron_identifiers::{Cnpj, CnpjError};
use libfuzzer_sys::fuzz_target;

const LEN: usize = 14;
const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const DIGITS: &[u8] = b"0123456789";

/// Full invariant oracle for a value the library accepted.
fn check(value: Cnpj) {
    // `as_str` goes through `from_utf8_unchecked`; it must be sound and equal to the raw bytes.
    assert_eq!(std::str::from_utf8(value.as_bytes()), Ok(value.as_str()));

    // Fixed length: twelve alphanumeric base characters followed by two numeric check digits.
    assert_eq!(value.as_bytes().len(), LEN);
    assert_eq!(value.as_str().len(), LEN);
    let bytes = value.as_bytes();
    assert!(bytes[..12].iter().all(|b| b.is_ascii_alphanumeric()));
    assert!(bytes[12].is_ascii_digit() && bytes[13].is_ascii_digit());

    // Accessors are consistent with the canonical string.
    assert_eq!(value.root(), &value.as_str()[0..8]);
    assert_eq!(value.branch_code(), &value.as_str()[8..12]);
    let (dv1, dv2) = value.check_digits();
    assert!(dv1 < 10 && dv2 < 10);
    assert_eq!(dv1, bytes[12] - b'0');
    assert_eq!(dv2, bytes[13] - b'0');

    // Branch semantics: `is_root` is exactly the "0001" marker, and `branch_number` is `Some`
    // only for a purely numeric branch, in which case it round-trips through the segment.
    assert_eq!(value.is_root(), value.branch_code() == "0001");
    let branch_is_numeric = value.branch_code().bytes().all(|b| b.is_ascii_digit());
    assert_eq!(value.branch_number().is_some(), branch_is_numeric);
    if let Some(n) = value.branch_number() {
        assert_eq!(format!("{n:04}"), value.branch_code());
    }

    // Every constructor agrees and parsing the canonical form is idempotent.
    assert_eq!(Cnpj::parse(value.as_str()), Ok(value));
    assert_eq!(Cnpj::new(value.as_str()), Ok(value));
    assert_eq!(Cnpj::from_bytes(*value.as_bytes()), Ok(value));
    assert_eq!(value.as_str().parse::<Cnpj>(), Ok(value));
    assert_eq!(Cnpj::try_from(value.as_str()), Ok(value));
    assert_eq!(Cnpj::try_from(*value.as_bytes()), Ok(value));
    assert_eq!(Cnpj::try_from(value.as_bytes().as_slice()), Ok(value));

    // Equality and reference conversions agree with the canonical string/bytes.
    assert_eq!(value, *value.as_str());
    assert_eq!(value, value.as_str());
    assert_eq!(<Cnpj as AsRef<str>>::as_ref(&value), value.as_str());
    assert_eq!(<Cnpj as AsRef<[u8]>>::as_ref(&value), value.as_bytes());

    // serde round-trips through the canonical string.
    let json = serde_json::to_string(&value).expect("serialize");
    assert_eq!(
        serde_json::from_str::<Cnpj>(&json).expect("deserialize"),
        value
    );

    // Rendering must never panic. `Display` uses the punctuated form produced by `formatted()`.
    let formatted = value.formatted();
    assert_eq!(formatted.as_str().len(), 18);
    assert_eq!(&*formatted, formatted.as_str());
    assert_eq!(format!("{value}"), formatted.as_str());
    assert_eq!(format!("{formatted}"), formatted.as_str());
    let _ = format!("{formatted:?}");
    let _ = format!("{value:?}");
}

/// Builds a candidate with the correct character classes (twelve alphanumeric base characters and
/// two numeric check digits), so the modulus-11 checksum runs on near-valid input every time.
fn shaped(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let pick = |set: &[u8], i: usize| set[(data[i % data.len()] as usize) % set.len()] as char;
    let mut out = String::with_capacity(LEN);
    for i in 0..12 {
        out.push(pick(ALPHANUMERIC, i));
    }
    out.push(pick(DIGITS, 12));
    out.push(pick(DIGITS, 13));
    out
}

fuzz_target!(|data: &[u8]| {
    // 1. Arbitrary text: the parser must never panic, and any accepted value satisfies the oracle.
    if let Ok(text) = std::str::from_utf8(data) {
        match Cnpj::parse(text) {
            Ok(value) => check(value),
            // Formatting the error must never panic, and reported metadata must match the input.
            Err(err) => {
                let _ = err.to_string();
                if let CnpjError::InvalidLength { found } = err {
                    // `found` counts meaningful characters, i.e. everything except the
                    // formatting characters the parser strips (`.`, `/`, `-`, and ASCII
                    // whitespace, so tabs and newlines are formatting too).
                    let meaningful = text
                        .chars()
                        .filter(|c| !(matches!(c, '.' | '/' | '-') || c.is_ascii_whitespace()))
                        .count();
                    assert_eq!(found, meaningful);
                    assert_ne!(found, LEN);
                }
            }
        }
    }

    // 2. Arbitrary raw bytes into `from_bytes`: never panic; accepted values stay sound.
    if data.len() >= LEN {
        let mut bytes = [0u8; LEN];
        bytes.copy_from_slice(&data[..LEN]);
        if let Ok(value) = Cnpj::from_bytes(bytes) {
            check(value);
        }
    }

    // 3. Structure-aware candidate: correct classes, so the modulus-11 checksum runs on near-valid
    //    input rather than being rejected at the character gate.
    if let Ok(value) = Cnpj::parse(&shaped(data)) {
        check(value);
    }

    // 4. Arbitrary-generated valid value: exercises the acceptance path deterministically.
    let mut unstructured = Unstructured::new(data);
    if let Ok(value) = Cnpj::arbitrary(&mut unstructured) {
        check(value);
    }
});
