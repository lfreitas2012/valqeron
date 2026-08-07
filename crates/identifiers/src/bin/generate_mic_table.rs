//! Generates `src/mic/table.rs` from `data/mic.csv`, the code-only ISO 10383 (MIC) seed.
//!
//! This is a build-time tool, not part of the library. It only compiles under the `codegen` feature;
//! run it via `cargo run -p valqeron-identifiers --bin generate_mic_table --features codegen`.
//!
//! It has two modes:
//!
//! * Default (no arguments): read the committed seed `data/mic.csv` and regenerate
//!   `src/mic/table.rs` from it. This is fully offline and deterministic.
//! * `--import <path>`: read an official ISO 10383 publication (the CSV published at
//!   <https://www.iso20022.org/market-identifier-codes>), reduce it to the code-only seed, rewrite
//!   `data/mic.csv`, and then regenerate the table from the fresh seed. Only the registry *code*
//!   relationships are kept — MIC, operating MIC, kind, country, and status. No ISO descriptive
//!   text (market or legal entity names, cities, websites, comments) is reproduced.
//!
//! At import time the seed is normalized so that regeneration diffs stay meaningful: rows are
//! sorted by MIC, and the transient `UPDATED` publication status (which means "modified in this
//! publication", not a distinct lifecycle state) is folded into `ACTIVE`, so the committed seed
//! only ever distinguishes `ACTIVE` from `EXPIRED`.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use valqeron_identifiers::CountryCode;

/// The seed columns, which are also the exact column names used by the official publication.
const COLUMNS: [&str; 5] = [
    "MIC",
    "OPERATING MIC",
    "OPRT/SGMT",
    "ISO COUNTRY CODE (ISO 3166)",
    "STATUS",
];

/// One registered MIC reduced to its code relationships.
#[derive(Clone, Copy)]
struct Market {
    /// The four character market identifier code.
    mic: [u8; 4],
    /// The MIC of the operating market this entry belongs to; equals `mic` for operating MICs.
    operating: [u8; 4],
    /// The ISO 3166-1 alpha-2 country as published (`ZZ` on the off-exchange pseudo-MICs).
    country: [u8; 2],
    /// `false` when the registry lists the code as `EXPIRED`.
    active: bool,
}

fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let seed_path = Path::new(manifest_dir).join("data/mic.csv");
    let out_path = Path::new(manifest_dir).join("src/mic/table.rs");

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => {}
        [flag, source] if flag == "--import" => {
            let raw = fs::read_to_string(source)
                .unwrap_or_else(|e| panic!("failed to read {source}: {e}"));
            let markets = parse_publication(&raw);
            if let Some(parent) = seed_path.parent() {
                fs::create_dir_all(parent)
                    .unwrap_or_else(|e| panic!("failed to create {}: {e}", parent.display()));
            }
            fs::write(&seed_path, render_seed(&markets))
                .unwrap_or_else(|e| panic!("failed to write {}: {e}", seed_path.display()));
            println!("wrote {}", seed_path.display());
        }
        _ => panic!("expected no arguments or `--import <path/to/ISO10383_MIC.csv>`"),
    }

    // Always regenerate from the committed seed, so an import also round-trips through the exact
    // file (and parser) that later offline regenerations will use.
    let raw = fs::read_to_string(&seed_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", seed_path.display()));
    let markets = parse_seed(&raw);
    let rendered = render_table(&markets);

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("failed to create {}: {e}", parent.display()));
    }
    fs::write(&out_path, rendered)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));

    println!("wrote {}", out_path.display());
    println!("run `cargo fmt` to normalize the output (the `just` recipes do this for you)");
}

/// Parses an official ISO 10383 publication, locating the seed columns by header name.
fn parse_publication(raw: &str) -> Vec<Market> {
    let records = parse_csv(raw);
    let (header, rows) = records
        .split_first()
        .expect("the publication must have a header row");
    let columns = COLUMNS.map(|name| {
        header
            .iter()
            .position(|h| h.trim() == name)
            .unwrap_or_else(|| panic!("the publication header is missing the `{name}` column"))
    });
    finalize(parse_rows(rows, &columns, true))
}

/// Parses the committed code-only seed, whose header must be exactly the five seed columns.
fn parse_seed(raw: &str) -> Vec<Market> {
    let records = parse_csv(raw);
    let (header, rows) = records
        .split_first()
        .expect("the seed must have a header row");
    assert_eq!(
        header,
        &COLUMNS,
        "the seed header must be exactly `{}`",
        COLUMNS.join(",")
    );
    let columns = [0, 1, 2, 3, 4];
    finalize(parse_rows(rows, &columns, false))
}

fn parse_rows(rows: &[Vec<String>], columns: &[usize; 5], allow_updated: bool) -> Vec<Market> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| parse_row(row, columns, i + 2, allow_updated))
        .collect()
}

/// Parses one data row. `line` is the 1-indexed physical line for error context (header is line 1).
fn parse_row(row: &[String], columns: &[usize; 5], line: usize, allow_updated: bool) -> Market {
    let field = |i: usize| -> &str {
        row.get(columns[i])
            .unwrap_or_else(|| panic!("line {line}: missing `{}` column", COLUMNS[i]))
            .trim()
    };

    let mic = code4(field(0), line, COLUMNS[0]);
    let operating = code4(field(1), line, COLUMNS[1]);
    let kind = field(2);
    let country = code2(field(3), line);
    let status = field(4);

    let self_operated = mic == operating;
    match kind {
        "OPRT" => assert!(
            self_operated,
            "line {line}: kind is OPRT but the OPERATING MIC differs from the MIC"
        ),
        "SGMT" => assert!(
            !self_operated,
            "line {line}: kind is SGMT but the OPERATING MIC equals the MIC"
        ),
        other => panic!("line {line}: unknown OPRT/SGMT value {other:?}"),
    }

    let active = match status {
        "ACTIVE" => true,
        "UPDATED" if allow_updated => true,
        "EXPIRED" => false,
        other => panic!("line {line}: unknown STATUS value {other:?}"),
    };

    // `ZZ` marks the off-exchange pseudo-MICs (for example `XOFF`). Every other country must be a
    // code this crate's own `CountryCode` accepts, so `Mic::country_code` can never silently
    // return `None` for a real country. A failure here means the ISO 3166-1 table in
    // `src/country/table.rs` needs updating first.
    if country != *b"ZZ"
        && let Err(e) = CountryCode::from_bytes(country)
    {
        panic!(
            "line {line}: country {:?} is not an assigned ISO 3166-1 code ({e}); \
             update src/country/table.rs first",
            as_str(&country)
        );
    }

    Market {
        mic,
        operating,
        country,
        active,
    }
}

/// Sorts by MIC and enforces the cross-row invariants the emitted table relies on.
fn finalize(mut markets: Vec<Market>) -> Vec<Market> {
    assert!(!markets.is_empty(), "the registry cannot be empty");
    assert!(
        u16::try_from(markets.len()).is_ok(),
        "the table stores operating references as u16 indexes"
    );

    markets.sort_by_key(|m| m.mic);
    for pair in markets.windows(2) {
        assert!(
            pair[0].mic != pair[1].mic,
            "duplicate MIC {:?}",
            as_str(&pair[0].mic)
        );
    }

    // Every operating reference must resolve, and chasing references must terminate. The registry
    // keeps the operating MIC a segment had at the time on expired rows, so a handful of expired
    // segments point at a code that was later re-parented and is now itself a segment; following
    // such a chain must still reach an operating MIC within a bounded number of hops.
    for market in &markets {
        let mut current = *market;
        let mut hops = 0usize;
        while current.mic != current.operating {
            let index = markets
                .binary_search_by_key(&current.operating, |m| m.mic)
                .unwrap_or_else(|_| {
                    panic!(
                        "MIC {:?} references operating MIC {:?}, which is not in the registry",
                        as_str(&current.mic),
                        as_str(&current.operating)
                    )
                });
            current = markets[index];
            hops += 1;
            assert!(
                hops <= markets.len(),
                "MIC {:?} starts a cycle of operating MIC references",
                as_str(&market.mic)
            );
        }
    }

    markets
}

/// Renders the committed `data/mic.csv` seed.
fn render_seed(markets: &[Market]) -> String {
    let mut s = String::new();
    s.push_str(&COLUMNS.join(","));
    s.push('\n');
    for m in markets {
        let kind = if m.mic == m.operating { "OPRT" } else { "SGMT" };
        let status = if m.active { "ACTIVE" } else { "EXPIRED" };
        writeln!(
            s,
            "{},{},{kind},{},{status}",
            as_str(&m.mic),
            as_str(&m.operating),
            as_str(&m.country)
        )
        .unwrap();
    }
    s
}

/// Renders the committed `src/mic/table.rs` source.
fn render_table(markets: &[Market]) -> String {
    let mut s = String::new();

    s.push_str(HEADER);

    s.push_str("pub(crate) static ENTRIES: &[MicEntry] = &[\n");
    for m in markets {
        let operating = markets
            .binary_search_by_key(&m.operating, |x| x.mic)
            .expect("finalize checked every operating reference");
        writeln!(
            s,
            "    MicEntry {{ code: *b\"{}\", operating: {operating}, country: *b\"{}\", active: {} }},",
            as_str(&m.mic),
            as_str(&m.country),
            m.active
        )
        .unwrap();
    }
    s.push_str("];\n");

    s.push_str(FOOTER);

    s
}

/// Validates a four character uppercase alphanumeric code and returns its bytes.
fn code4(s: &str, line: usize, what: &str) -> [u8; 4] {
    let bytes = s.as_bytes();
    assert!(
        bytes.len() == 4
            && bytes
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
        "line {line}: {what} must be four uppercase A-Z or 0-9 characters, found {s:?}"
    );
    [bytes[0], bytes[1], bytes[2], bytes[3]]
}

/// Validates a two character uppercase letter code and returns its bytes.
fn code2(s: &str, line: usize) -> [u8; 2] {
    let bytes = s.as_bytes();
    assert!(
        bytes.len() == 2 && bytes.iter().all(u8::is_ascii_uppercase),
        "line {line}: {} must be two uppercase A-Z letters, found {s:?}",
        COLUMNS[3]
    );
    [bytes[0], bytes[1]]
}

/// Views validated ASCII code bytes as a `&str` for messages and rendering.
fn as_str(code: &[u8]) -> &str {
    std::str::from_utf8(code).expect("codes are validated ASCII")
}

/// Splits CSV text into records of fields, per RFC 4180: fields may be double-quoted, and quoted
/// fields may contain commas, CR/LF line breaks, and doubled quotes as escapes. A UTF-8 BOM before
/// the header is tolerated. Blank trailing lines are dropped.
fn parse_csv(input: &str) -> Vec<Vec<String>> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);

    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;

    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            match ch {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                _ => field.push(ch),
            }
            continue;
        }
        match ch {
            '"' if field.is_empty() => in_quotes = true,
            ',' => record.push(std::mem::take(&mut field)),
            '\r' => {} // the publication uses CRLF; the following '\n' terminates the record
            '\n' => {
                record.push(std::mem::take(&mut field));
                if record.iter().any(|f| !f.is_empty()) {
                    records.push(std::mem::take(&mut record));
                } else {
                    record.clear();
                }
            }
            _ => field.push(ch),
        }
    }
    assert!(!in_quotes, "unterminated quoted field");
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    records
}

const HEADER: &str = "\
// @generated by `just mic-generate` from data/mic.csv — DO NOT EDIT BY HAND.
//
// Derived from the ISO 10383 (MIC) registry publication, reduced to the code relationships only:
// each registered MIC, the operating MIC that owns it, its ISO 3166-1 alpha-2 country, and whether
// it is still active. No ISO descriptive text (market or legal entity names, cities, websites,
// comments) is reproduced here. Refresh the seed from the latest publication with `just
// mic-update`, regenerate offline with `just mic-generate`; `just mic-check` verifies there is no
// drift.

/// One registered ISO 10383 market identifier code and its code-level relationships.
///
/// `operating` is the index within [`ENTRIES`] of the operating MIC this entry belongs to, exactly
/// as published; an operating MIC references itself, so segment entries are exactly those whose
/// reference names a different code. On a few expired segments the published reference names a
/// code that was later re-parented and is now itself a segment; references always resolve and
/// never cycle, but they are not guaranteed to name a current operating MIC. `country` is the ISO
/// 3166-1 alpha-2 code as published; the special `ZZ` marker appears only on the off-exchange
/// pseudo-MICs. `active` is `false` for codes the registry lists as expired.
pub(crate) struct MicEntry {
    pub code: [u8; 4],
    pub operating: u16,
    pub country: [u8; 2],
    pub active: bool,
}

";

const FOOTER: &str = "
// Compile time guard. Any violation is a build error, so a regenerated table cannot regress
// unnoticed.
const _: () = check_table(ENTRIES);

/// Asserts that the table upholds the invariants the validator and the accessors rely on: codes
/// are four uppercase ASCII letters or digits in strictly ascending order (so binary search is
/// sound and codes are unique), countries are two uppercase ASCII letters, and every `operating`
/// index is in range. Deeper referential invariants (acyclicity of operating references) are
/// enforced by the generator.
const fn check_table(entries: &[MicEntry]) {
    let mut i = 0;
    while i < entries.len() {
        let code = entries[i].code;
        let mut j = 0;
        while j < 4 {
            assert!(
                (code[j] >= b'A' && code[j] <= b'Z') || (code[j] >= b'0' && code[j] <= b'9'),
                \"every MIC must be four uppercase ASCII letters or digits\"
            );
            j += 1;
        }

        let country = entries[i].country;
        assert!(
            country[0] >= b'A' && country[0] <= b'Z' && country[1] >= b'A' && country[1] <= b'Z',
            \"every country must be two uppercase ASCII letters\"
        );

        if i > 0 {
            assert!(
                ascending(&entries[i - 1].code, &code),
                \"entries must be listed in strictly ascending code order\"
            );
        }

        assert!(
            (entries[i].operating as usize) < entries.len(),
            \"every operating index must be in range\"
        );

        i += 1;
    }
}

/// `true` when `a` sorts strictly before `b` in byte-wise lexicographic order.
const fn ascending(a: &[u8; 4], b: &[u8; 4]) -> bool {
    let mut i = 0;
    while i < 4 {
        if a[i] < b[i] {
            return true;
        }
        if a[i] > b[i] {
            return false;
        }
        i += 1;
    }
    false
}
";
