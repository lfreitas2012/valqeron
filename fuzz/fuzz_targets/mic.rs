#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;
use valqeron_core::identifiers::{Mic, MicError};

const LEN: usize = 4;
const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Full invariant oracle for a value the library accepted. Every property here must hold for any
/// valid `Mic`, no matter how it was produced.
fn check(value: Mic) {
    assert_eq!(std::str::from_utf8(value.as_bytes()), Ok(value.as_str()));

    assert_eq!(value.as_bytes().len(), LEN);
    assert_eq!(value.as_str().len(), LEN);
    assert!(
        value
            .as_bytes()
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    );

    // Every constructor agrees, and parsing the canonical form is idempotent.
    assert_eq!(Mic::parse(value.as_str()), Ok(value));
    assert_eq!(Mic::new(value.as_str()), Ok(value));
    assert_eq!(Mic::from_bytes(*value.as_bytes()), Ok(value));
    assert_eq!(value.as_str().parse::<Mic>(), Ok(value));
    assert_eq!(Mic::try_from(value.as_str()), Ok(value));
    assert_eq!(Mic::try_from(*value.as_bytes()), Ok(value));
    assert_eq!(Mic::try_from(value.as_bytes().as_slice()), Ok(value));

    // Equality and reference conversions agree with the canonical string/bytes.
    assert_eq!(value, *value.as_str());
    assert_eq!(value, value.as_str());
    assert_eq!(<Mic as AsRef<str>>::as_ref(&value), value.as_str());
    assert_eq!(<Mic as AsRef<[u8]>>::as_ref(&value), value.as_bytes());

    // The registry accessors must never panic, and their answers must be mutually consistent.
    let _ = value.is_active();
    let operating = value.operating_mic();
    assert_eq!(Mic::parse(operating.as_str()), Ok(operating));
    assert_eq!(value.is_operating(), operating == value);
    assert_eq!(value.is_segment(), !value.is_operating());
    if let Some(country) = value.country_code() {
        // A returned country is a fully validated `CountryCode`; its canonical form is two
        // uppercase ASCII letters and never the `ZZ` placeholder.
        assert!(country.as_bytes().iter().all(|b| b.is_ascii_uppercase()));
        assert_ne!(country.as_bytes(), b"ZZ");
    }

    // serde round-trips through the canonical string.
    let json = serde_json::to_string(&value).expect("serialize");
    assert_eq!(
        serde_json::from_str::<Mic>(&json).expect("deserialize"),
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
        .map(|i| ALPHANUMERIC[(data[i % data.len()] as usize) % ALPHANUMERIC.len()] as char)
        .collect()
}

fuzz_target!(|data: &[u8]| {
    // 1. Arbitrary text: the parser must never panic, and any accepted value satisfies the oracle.
    if let Ok(text) = std::str::from_utf8(data) {
        match Mic::parse(text) {
            Ok(value) => check(value),
            // Formatting the error must never panic, and reported metadata must match the input.
            Err(err) => {
                let _ = err.to_string();
                if let MicError::InvalidLength { found } = err {
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
        if let Ok(value) = Mic::from_bytes(bytes) {
            check(value);
        }
    }

    // 3. Structure-aware candidate: correct shape and class, so the membership logic runs on
    //    near-valid input rather than being rejected at the gate.
    if let Ok(value) = Mic::parse(&shaped(data)) {
        check(value);
    }

    // 4. Arbitrary-generated valid value: exercises the acceptance path deterministically.
    let mut unstructured = Unstructured::new(data);
    if let Ok(value) = Mic::arbitrary(&mut unstructured) {
        check(value);
    }
});
