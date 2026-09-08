use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use syn::{LitStr, parse_macro_input};

type MacroResult<T> = syn::Result<T>;

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
    match build_cfi_table(&path_lit) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn build_cfi_table(path_lit: &LitStr) -> MacroResult<proc_macro2::TokenStream> {
    let span = path_lit.span();
    let seed_path = resolve_seed_path(&path_lit.value(), span)?;
    let absolute_path_str = seed_path
        .to_str()
        .ok_or_else(|| syn::Error::new(span, "seed path must be valid UTF-8"))?;

    let raw = fs::read_to_string(&seed_path).map_err(|e| {
        syn::Error::new(span, format!("failed to read {}: {e}", seed_path.display()))
    })?;
    let json: Value = serde_json::from_str(&raw).map_err(|e| {
        syn::Error::new(
            span,
            format!("failed to parse {}: {e}", seed_path.display()),
        )
    })?;

    let categories = parse_cfi_categories(&json, span)?;

    let mut category_tokens = Vec::new();
    for cat in categories {
        let cat_code = syn::LitByte::new(cat.code, span);
        let mut group_tokens = Vec::new();

        for g in cat.groups {
            let g_code = syn::LitByte::new(g.code, span);
            let [m0, m1, m2, m3] = g.masks;

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

    Ok(quote! {
        const _: &[u8] = include_bytes!(#absolute_path_str);

        pub(crate) static CFI_CATEGORIES: &[CfiCategoryEntry] = &[
            #(#category_tokens),*
        ];
    })
}

fn resolve_seed_path(rel_path: &str, span: Span) -> MacroResult<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| syn::Error::new(span, "CARGO_MANIFEST_DIR is not set"))?;
    Ok(Path::new(&manifest_dir).join(rel_path))
}

fn parse_cfi_categories(json: &Value, span: Span) -> MacroResult<Vec<CfiCategory>> {
    let cats = json
        .get("categories")
        .and_then(Value::as_array)
        .ok_or_else(|| syn::Error::new(span, "top-level `categories` must be an array"))?;

    let mut out = cats
        .iter()
        .map(|cat| parse_cfi_category(cat, span))
        .collect::<MacroResult<Vec<_>>>()?;
    out.sort_by_key(|c| c.code);
    Ok(out)
}

fn parse_cfi_category(cat: &Value, span: Span) -> MacroResult<CfiCategory> {
    let code_node = cat
        .get("code")
        .ok_or_else(|| syn::Error::new(span, "category missing `code`"))?;
    let code = single_letter(code_node, "category code", span)?;

    let groups_json = cat
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| syn::Error::new(span, "`groups` must be an array"))?;

    let mut groups = groups_json
        .iter()
        .map(|g| parse_cfi_group(g, span))
        .collect::<MacroResult<Vec<_>>>()?;
    groups.sort_by_key(|g| g.code);

    Ok(CfiCategory { code, groups })
}

fn parse_cfi_group(group: &Value, span: Span) -> MacroResult<CfiGroup> {
    let code_node = group
        .get("code")
        .ok_or_else(|| syn::Error::new(span, "group missing `code`"))?;
    let code = single_letter(code_node, "group code", span)?;

    let attr_keys = ["attribute1", "attribute2", "attribute3", "attribute4"];
    let mut masks = [0u32; 4];

    for (slot, key) in masks.iter_mut().zip(attr_keys.iter()) {
        let values = group
            .get(*key)
            .and_then(|attr| attr.get("attributeValues"))
            .and_then(Value::as_array)
            .ok_or_else(|| {
                syn::Error::new(span, format!("`{key}.attributeValues` must be an array"))
            })?;

        let mut mask = 0u32;
        for value in values {
            let code_node = value
                .get("code")
                .ok_or_else(|| syn::Error::new(span, "attribute value missing `code`"))?;
            let letter = single_letter(code_node, "attribute value code", span)?;
            let offset = letter
                .checked_sub(b'A')
                .ok_or_else(|| syn::Error::new(span, "attribute value code out of range"))?;
            let bit = 1u32
                .checked_shl(u32::from(offset))
                .ok_or_else(|| syn::Error::new(span, "attribute value code out of range"))?;
            mask |= bit;
        }
        if mask == 0 {
            return Err(syn::Error::new(
                span,
                format!(
                    "attribute `{key}` has no values in group {}",
                    char::from(code)
                ),
            ));
        }
        *slot = mask;
    }

    Ok(CfiGroup { code, masks })
}

fn single_letter(node: &Value, what: &str, span: Span) -> MacroResult<u8> {
    let s = node
        .as_str()
        .ok_or_else(|| syn::Error::new(span, format!("{what} must be a string")))?;
    match s.as_bytes() {
        [b] if b.is_ascii_uppercase() => Ok(*b),
        _ => Err(syn::Error::new(
            span,
            format!("{what} must be a single uppercase A-Z letter, found {s:?}"),
        )),
    }
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
    match build_mic_table(&path_lit) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn build_mic_table(path_lit: &LitStr) -> MacroResult<proc_macro2::TokenStream> {
    let span = path_lit.span();
    let seed_path = resolve_seed_path(&path_lit.value(), span)?;
    let absolute_path_str = seed_path
        .to_str()
        .ok_or_else(|| syn::Error::new(span, "seed path must be valid UTF-8"))?;

    let raw = fs::read_to_string(&seed_path).map_err(|e| {
        syn::Error::new(span, format!("failed to read {}: {e}", seed_path.display()))
    })?;

    let markets = parse_mic_seed(&raw, span)?;

    let mut entry_tokens = Vec::new();
    for m in &markets {
        let operating_pos = markets
            .binary_search_by_key(&m.operating, |x| x.mic)
            .map_err(|_| syn::Error::new(span, "finalize checked every operating reference"))?;
        let operating_idx = u16::try_from(operating_pos)
            .map_err(|_| syn::Error::new(span, "too many entries in the MIC registry"))?;

        let [b0, b1, b2, b3] = m.mic;
        let [c0, c1] = m.country;
        let active = m.active;

        entry_tokens.push(quote! {
            MicEntry {
                code: [#b0, #b1, #b2, #b3],
                operating: #operating_idx,
                country: [#c0, #c1],
                active: #active,
            }
        });
    }

    Ok(quote! {
        const _: &[u8] = include_bytes!(#absolute_path_str);

        pub(crate) static MIC_ENTRIES: &[MicEntry] = &[
            #(#entry_tokens),*
        ];
    })
}

fn parse_mic_seed(raw: &str, span: Span) -> MacroResult<Vec<Market>> {
    let records = parse_csv(raw, span)?;
    let (header, rows) = records
        .split_first()
        .ok_or_else(|| syn::Error::new(span, "the seed must have a header row"))?;

    let header_matches = header
        .iter()
        .map(String::as_str)
        .eq(MIC_COLUMNS.iter().copied());
    if !header_matches {
        return Err(syn::Error::new(
            span,
            format!(
                "the seed header must be exactly `{}`",
                MIC_COLUMNS.join(",")
            ),
        ));
    }

    let columns = [0usize, 1, 2, 3, 4];
    finalize_mic(parse_mic_rows(rows, &columns, span)?, span)
}

fn parse_mic_rows(
    rows: &[Vec<String>],
    columns: &[usize; 5],
    span: Span,
) -> MacroResult<Vec<Market>> {
    rows.iter()
        .zip(2usize..)
        .map(|(row, line)| parse_mic_row(row, columns, line, span))
        .collect()
}

fn parse_mic_row(
    row: &[String],
    columns: &[usize; 5],
    line: usize,
    span: Span,
) -> MacroResult<Market> {
    let [c0, c1, c2, c3, c4] = *columns;
    let [n0, n1, n2, n3, n4] = MIC_COLUMNS;

    let field = |col: usize, name: &str| -> MacroResult<&str> {
        row.get(col)
            .map(|s| s.trim())
            .ok_or_else(|| syn::Error::new(span, format!("line {line}: missing `{name}` column")))
    };

    let mic = code4(field(c0, n0)?, line, n0, span)?;
    let operating = code4(field(c1, n1)?, line, n1, span)?;
    let kind = field(c2, n2)?;
    let country = code2(field(c3, n3)?, line, n3, span)?;
    let status = field(c4, n4)?;

    let self_operated = mic == operating;
    match kind {
        "OPRT" if self_operated => {}
        "OPRT" => {
            return Err(syn::Error::new(
                span,
                format!("line {line}: kind is OPRT but the OPERATING MIC differs from the MIC"),
            ));
        }
        "SGMT" if !self_operated => {}
        "SGMT" => {
            return Err(syn::Error::new(
                span,
                format!("line {line}: kind is SGMT but the OPERATING MIC equals the MIC"),
            ));
        }
        other => {
            return Err(syn::Error::new(
                span,
                format!("line {line}: unknown OPRT/SGMT value {other:?}"),
            ));
        }
    }

    let active = match status {
        "ACTIVE" => true,
        "EXPIRED" => false,
        other => {
            return Err(syn::Error::new(
                span,
                format!("line {line}: unknown STATUS value {other:?}"),
            ));
        }
    };

    Ok(Market {
        mic,
        operating,
        country,
        active,
    })
}

fn finalize_mic(mut markets: Vec<Market>, span: Span) -> MacroResult<Vec<Market>> {
    if markets.is_empty() {
        return Err(syn::Error::new(span, "the registry cannot be empty"));
    }
    if u16::try_from(markets.len()).is_err() {
        return Err(syn::Error::new(
            span,
            "the table stores operating references as u16 indexes",
        ));
    }

    markets.sort_by_key(|m| m.mic);
    for (a, b) in markets.iter().zip(markets.iter().skip(1)) {
        if a.mic == b.mic {
            return Err(syn::Error::new(
                span,
                format!("duplicate MIC {}", mic_display(a.mic)),
            ));
        }
    }

    for market in &markets {
        let mut current = *market;
        let mut hops = 0usize;
        while current.mic != current.operating {
            let idx = markets
                .binary_search_by_key(&current.operating, |m| m.mic)
                .map_err(|_| {
                    syn::Error::new(
                        span,
                        format!(
                            "MIC {} references operating MIC {}, which is not in the registry",
                            mic_display(current.mic),
                            mic_display(current.operating),
                        ),
                    )
                })?;
            current = *markets
                .get(idx)
                .ok_or_else(|| syn::Error::new(span, "internal error resolving operating MIC"))?;
            hops = hops.saturating_add(1);
            if hops > markets.len() {
                return Err(syn::Error::new(
                    span,
                    format!(
                        "MIC {} starts a cycle of operating MIC references",
                        mic_display(market.mic)
                    ),
                ));
            }
        }
    }

    Ok(markets)
}

fn mic_display(mic: [u8; 4]) -> String {
    mic.iter().map(|&byte| char::from(byte)).collect()
}

fn is_code_char(byte: u8) -> bool {
    byte.is_ascii_uppercase() || byte.is_ascii_digit()
}

fn code4(s: &str, line: usize, what: &str, span: Span) -> MacroResult<[u8; 4]> {
    match s.as_bytes() {
        [byte0, byte1, byte2, byte3]
            if is_code_char(*byte0)
                && is_code_char(*byte1)
                && is_code_char(*byte2)
                && is_code_char(*byte3) =>
        {
            Ok([*byte0, *byte1, *byte2, *byte3])
        }
        _ => Err(syn::Error::new(
            span,
            format!(
                "line {line}: {what} must be four uppercase A-Z or 0-9 characters, found {s:?}"
            ),
        )),
    }
}

fn code2(s: &str, line: usize, what: &str, span: Span) -> MacroResult<[u8; 2]> {
    match s.as_bytes() {
        [a, b] if a.is_ascii_uppercase() && b.is_ascii_uppercase() => Ok([*a, *b]),
        _ => Err(syn::Error::new(
            span,
            format!("line {line}: {what} must be two uppercase A-Z letters, found {s:?}"),
        )),
    }
}

fn parse_csv(input: &str, span: Span) -> MacroResult<Vec<Vec<String>>> {
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
    if in_quotes {
        return Err(syn::Error::new(span, "unterminated quoted field"));
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}
