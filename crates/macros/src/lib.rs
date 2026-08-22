use proc_macro::TokenStream;
use quote::quote;
use serde_json::Value;
use std::fs;
use std::path::Path;
use syn::{LitStr, parse_macro_input};

// --- CFI Table Macro ---

struct CfiGroup {
    code: u8,
    masks: [u32; 4],
}

struct CfiCategory {
    code: u8,
    groups: Vec<CfiGroup>,
}

#[proc_macro]
pub fn generate_cfi_table(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    let rel_path = path_lit.value();

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set");
    let seed_path = Path::new(&manifest_dir).join(&rel_path);
    let absolute_path_str = seed_path.to_str().expect("Path must be valid UTF-8");

    let raw = fs::read_to_string(&seed_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", seed_path.display()));
    let json: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", seed_path.display()));

    let categories = parse_cfi_categories(&json);

    let mut category_tokens = Vec::new();
    for cat in categories {
        let cat_code = syn::LitByte::new(cat.code, proc_macro2::Span::call_site());
        let mut group_tokens = Vec::new();

        for g in cat.groups {
            let g_code = syn::LitByte::new(g.code, proc_macro2::Span::call_site());
            let m0 = g.masks[0];
            let m1 = g.masks[1];
            let m2 = g.masks[2];
            let m3 = g.masks[3];

            group_tokens.push(quote! {
                CfiGroupEntry {
                    code: #g_code,
                    attrs: [#m0, #m1, #m2, #m3],
                }
            });
        }

        category_tokens.push(quote! {
            CfiCategoryEntry {
                code: #cat_code,
                groups: &[
                    #(#group_tokens),*
                ],
            }
        });
    }

    let expanded = quote! {
        const _: &[u8] = include_bytes!(#absolute_path_str);

        pub(crate) static CFI_CATEGORIES: &[CfiCategoryEntry] = &[
            #(#category_tokens),*
        ];
    };

    TokenStream::from(expanded)
}

fn parse_cfi_categories(json: &Value) -> Vec<CfiCategory> {
    let cats = json["categories"]
        .as_array()
        .expect("top-level `categories` must be an array");
    let mut out: Vec<CfiCategory> = cats
        .iter()
        .map(|cat| {
            let code = single_letter(&cat["code"], "category code");
            let mut groups = cat["groups"]
                .as_array()
                .expect("`groups` must be an array")
                .iter()
                .map(parse_cfi_group)
                .collect::<Vec<_>>();
            groups.sort_by_key(|g| g.code);
            CfiCategory { code, groups }
        })
        .collect();
    out.sort_by_key(|c| c.code);
    out
}

fn parse_cfi_group(group: &Value) -> CfiGroup {
    let code = single_letter(&group["code"], "group code");
    let attr_keys = ["attribute1", "attribute2", "attribute3", "attribute4"];
    let mut masks = [0u32; 4];

    for (i, key) in attr_keys.iter().enumerate() {
        let values = group[*key]["attributeValues"]
            .as_array()
            .unwrap_or_else(|| panic!("`{key}.attributeValues` must be an array"));

        let mut mask = 0u32;
        for value in values {
            let letter = single_letter(&value["code"], "attribute value code");
            mask |= 1 << (letter - b'A');
        }
        assert_ne!(
            mask, 0,
            "attribute `{key}` has no values in group {}",
            code as char
        );
        masks[i] = mask;
    }

    CfiGroup { code, masks }
}

fn single_letter(node: &Value, what: &str) -> u8 {
    let s = node
        .as_str()
        .unwrap_or_else(|| panic!("{what} must be a string"));
    let bytes = s.as_bytes();
    assert!(
        bytes.len() == 1 && bytes[0].is_ascii_uppercase(),
        "{what} must be a single uppercase A-Z letter, found {s:?}"
    );
    bytes[0]
}

// --- MIC Table Macro ---

const MIC_COLUMNS: [&str; 5] = [
    "MIC",
    "OPERATING MIC",
    "OPRT/SGMT",
    "ISO COUNTRY CODE (ISO 3166)",
    "STATUS",
];

#[derive(Clone, Copy)]
struct Market {
    mic: [u8; 4],
    operating: [u8; 4],
    country: [u8; 2],
    active: bool,
}

#[proc_macro]
pub fn generate_mic_table(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    let rel_path = path_lit.value();

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set");
    let seed_path = Path::new(&manifest_dir).join(&rel_path);
    let absolute_path_str = seed_path.to_str().expect("Path must be valid UTF-8");

    let raw = fs::read_to_string(&seed_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", seed_path.display()));

    let markets = parse_mic_seed(&raw);

    let mut entry_tokens = Vec::new();
    for m in &markets {
        let operating_idx = markets
            .binary_search_by_key(&m.operating, |x| x.mic)
            .expect("finalize checked every operating reference");

        let b0 = m.mic[0];
        let b1 = m.mic[1];
        let b2 = m.mic[2];
        let b3 = m.mic[3];

        let c0 = m.country[0];
        let c1 = m.country[1];
        let active = m.active;

        entry_tokens.push(quote! {
            MicEntry {
                code: [#b0, #b1, #b2, #b3],
                operating: #operating_idx as u16,
                country: [#c0, #c1],
                active: #active,
            }
        });
    }

    let expanded = quote! {
        const _: &[u8] = include_bytes!(#absolute_path_str);

        pub(crate) static MIC_ENTRIES: &[MicEntry] = &[
            #(#entry_tokens),*
        ];
    };

    TokenStream::from(expanded)
}

fn parse_mic_seed(raw: &str) -> Vec<Market> {
    let records = parse_csv(raw);
    let (header, rows) = records
        .split_first()
        .expect("the seed must have a header row");
    assert_eq!(
        header,
        &MIC_COLUMNS.map(String::from),
        "the seed header must be exactly `{}`",
        MIC_COLUMNS.join(",")
    );
    let columns = [0, 1, 2, 3, 4];
    finalize_mic(parse_mic_rows(rows, &columns))
}

fn parse_mic_rows(rows: &[Vec<String>], columns: &[usize; 5]) -> Vec<Market> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| parse_mic_row(row, columns, i + 2))
        .collect()
}

fn parse_mic_row(row: &[String], columns: &[usize; 5], line: usize) -> Market {
    let field = |i: usize| -> &str {
        row.get(columns[i])
            .unwrap_or_else(|| panic!("line {line}: missing `{}` column", MIC_COLUMNS[i]))
            .trim()
    };

    let mic = code4(field(0), line, MIC_COLUMNS[0]);
    let operating = code4(field(1), line, MIC_COLUMNS[1]);
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
        "EXPIRED" => false,
        other => panic!("line {line}: unknown STATUS value {other:?}"),
    };

    Market {
        mic,
        operating,
        country,
        active,
    }
}

fn finalize_mic(mut markets: Vec<Market>) -> Vec<Market> {
    assert!(!markets.is_empty(), "the registry cannot be empty");
    assert!(
        u16::try_from(markets.len()).is_ok(),
        "the table stores operating references as u16 indexes"
    );

    markets.sort_by_key(|m| m.mic);
    for pair in markets.windows(2) {
        assert_ne!(
            pair[0].mic,
            pair[1].mic,
            "duplicate MIC {:?}",
            std::str::from_utf8(&pair[0].mic).unwrap()
        );
    }

    for market in &markets {
        let mut current = *market;
        let mut hops = 0usize;
        while current.mic != current.operating {
            let index = markets
                .binary_search_by_key(&current.operating, |m| m.mic)
                .unwrap_or_else(|_| {
                    panic!(
                        "MIC {:?} references operating MIC {:?}, which is not in the registry",
                        std::str::from_utf8(&current.mic).unwrap(),
                        std::str::from_utf8(&current.operating).unwrap()
                    )
                });
            current = markets[index];
            hops += 1;
            assert!(
                hops <= markets.len(),
                "MIC {:?} starts a cycle of operating MIC references",
                std::str::from_utf8(&market.mic).unwrap()
            );
        }
    }

    markets
}

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

fn code2(s: &str, line: usize) -> [u8; 2] {
    let bytes = s.as_bytes();
    assert!(
        bytes.len() == 2 && bytes.iter().all(u8::is_ascii_uppercase),
        "line {line}: {} must be two uppercase A-Z letters, found {s:?}",
        MIC_COLUMNS[3]
    );
    [bytes[0], bytes[1]]
}

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
            '\r' => {}
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
