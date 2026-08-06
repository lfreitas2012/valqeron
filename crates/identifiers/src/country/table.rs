//! The officially assigned ISO 3166-1 alpha-2 code set.
//!
//! The set is authored once as [`ASSIGNED_CODES`], a sorted list that reads like a phone book. The
//! membership bitmap [`ASSIGNED`] is derived from that list at compile time by [`build_bitmap`], so
//! the two can never drift apart: the list is the single source of truth. A compile time check
//! ([`check_table`]) proves the list is strictly ascending and holds only uppercase ASCII letters,
//! turning any typo into a build error.

/// The number of distinct two letter combinations, `26 * 26`.
const COMBINATIONS: usize = 26 * 26;

/// The number of 64 bit words needed to give every combination its own bit.
const WORDS: usize = COMBINATIONS.div_ceil(64);

/// The officially assigned ISO 3166-1 alpha-2 codes, in strictly ascending byte order.
///
/// Reserved codes (for example `EU` or `UK`), user assigned ranges, and codes that were once used
/// and later withdrawn are deliberately absent.
pub(crate) const ASSIGNED_CODES: &[[u8; 2]] = &[
    *b"AD", *b"AE", *b"AF", *b"AG", *b"AI", *b"AL", *b"AM", *b"AO", *b"AQ", *b"AR", //
    *b"AS", *b"AT", *b"AU", *b"AW", *b"AX", *b"AZ", *b"BA", *b"BB", *b"BD", *b"BE", //
    *b"BF", *b"BG", *b"BH", *b"BI", *b"BJ", *b"BL", *b"BM", *b"BN", *b"BO", *b"BQ", //
    *b"BR", *b"BS", *b"BT", *b"BV", *b"BW", *b"BY", *b"BZ", *b"CA", *b"CC", *b"CD", //
    *b"CF", *b"CG", *b"CH", *b"CI", *b"CK", *b"CL", *b"CM", *b"CN", *b"CO", *b"CR", //
    *b"CU", *b"CV", *b"CW", *b"CX", *b"CY", *b"CZ", *b"DE", *b"DJ", *b"DK", *b"DM", //
    *b"DO", *b"DZ", *b"EC", *b"EE", *b"EG", *b"EH", *b"ER", *b"ES", *b"ET", *b"FI", //
    *b"FJ", *b"FK", *b"FM", *b"FO", *b"FR", *b"GA", *b"GB", *b"GD", *b"GE", *b"GF", //
    *b"GG", *b"GH", *b"GI", *b"GL", *b"GM", *b"GN", *b"GP", *b"GQ", *b"GR", *b"GS", //
    *b"GT", *b"GU", *b"GW", *b"GY", *b"HK", *b"HM", *b"HN", *b"HR", *b"HT", *b"HU", //
    *b"ID", *b"IE", *b"IL", *b"IM", *b"IN", *b"IO", *b"IQ", *b"IR", *b"IS", *b"IT", //
    *b"JE", *b"JM", *b"JO", *b"JP", *b"KE", *b"KG", *b"KH", *b"KI", *b"KM", *b"KN", //
    *b"KP", *b"KR", *b"KW", *b"KY", *b"KZ", *b"LA", *b"LB", *b"LC", *b"LI", *b"LK", //
    *b"LR", *b"LS", *b"LT", *b"LU", *b"LV", *b"LY", *b"MA", *b"MC", *b"MD", *b"ME", //
    *b"MF", *b"MG", *b"MH", *b"MK", *b"ML", *b"MM", *b"MN", *b"MO", *b"MP", *b"MQ", //
    *b"MR", *b"MS", *b"MT", *b"MU", *b"MV", *b"MW", *b"MX", *b"MY", *b"MZ", *b"NA", //
    *b"NC", *b"NE", *b"NF", *b"NG", *b"NI", *b"NL", *b"NO", *b"NP", *b"NR", *b"NU", //
    *b"NZ", *b"OM", *b"PA", *b"PE", *b"PF", *b"PG", *b"PH", *b"PK", *b"PL", *b"PM", //
    *b"PN", *b"PR", *b"PS", *b"PT", *b"PW", *b"PY", *b"QA", *b"RE", *b"RO", *b"RS", //
    *b"RU", *b"RW", *b"SA", *b"SB", *b"SC", *b"SD", *b"SE", *b"SG", *b"SH", *b"SI", //
    *b"SJ", *b"SK", *b"SL", *b"SM", *b"SN", *b"SO", *b"SR", *b"SS", *b"ST", *b"SV", //
    *b"SX", *b"SY", *b"SZ", *b"TC", *b"TD", *b"TF", *b"TG", *b"TH", *b"TJ", *b"TK", //
    *b"TL", *b"TM", *b"TN", *b"TO", *b"TR", *b"TT", *b"TV", *b"TW", *b"TZ", *b"UA", //
    *b"UG", *b"UM", *b"US", *b"UY", *b"UZ", *b"VA", *b"VC", *b"VE", *b"VG", *b"VI", //
    *b"VN", *b"VU", *b"WF", *b"WS", *b"YE", *b"YT", *b"ZA", *b"ZM", *b"ZW",
];

/// The dense index of a two letter code in `0..676`.
///
/// The caller must pass two uppercase ASCII letters. Character class validation in
/// [`super::validation`] guarantees this before any lookup runs.
#[inline]
pub(crate) const fn bit_index(code: [u8; 2]) -> usize {
    (code[0] - b'A') as usize * 26 + (code[1] - b'A') as usize
}

/// The membership bitmap. Bit `bit_index(code)` is set exactly when `code` is officially assigned.
///
/// Derived from [`ASSIGNED_CODES`] at compile time.
pub(crate) const ASSIGNED: [u64; WORDS] = build_bitmap(ASSIGNED_CODES);

/// Folds a list of codes into a bitmap by setting one bit per code.
const fn build_bitmap(codes: &[[u8; 2]]) -> [u64; WORDS] {
    let mut bits = [0u64; WORDS];
    let mut i = 0;
    while i < codes.len() {
        let index = bit_index(codes[i]);
        bits[index / 64] |= 1u64 << (index % 64);
        i += 1;
    }
    bits
}

// Compile time guard. Any violation is a build error, so the table cannot regress unnoticed.
const _: () = check_table(ASSIGNED_CODES);

/// Asserts that every entry is two uppercase ASCII letters and that the list is strictly ascending.
///
/// Strict ascension gives two guarantees at once: the list is sorted, and it holds no duplicates.
const fn check_table(codes: &[[u8; 2]]) {
    let mut i = 0;
    while i < codes.len() {
        let a = codes[i][0];
        let b = codes[i][1];
        assert!(
            a >= b'A' && a <= b'Z' && b >= b'A' && b <= b'Z',
            "every assigned code must be two uppercase ASCII letters"
        );
        if i > 0 {
            let prev_a = codes[i - 1][0];
            let prev_b = codes[i - 1][1];
            assert!(
                prev_a < a || (prev_a == a && prev_b < b),
                "assigned codes must be listed in strictly ascending order"
            );
        }
        i += 1;
    }
}
