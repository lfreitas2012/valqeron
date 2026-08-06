//! Generates `src/cfi/table.rs` from `data/cfi.json`, the code-only ISO 10962 (CFI) seed.
//!
//! This is a build-time tool, not part of the library. It only compiles under the non-default
//! `codegen` feature (which pulls in the optional `serde_json` dependency), so downstream builds
//! never touch it; run it via `just cfi-generate` (`cargo run --bin generate_cfi_table --features
//! codegen`). The output is committed and `just cfi-check` guards against drift, and both this file
//! and the seed are excluded from the published crate.
//!
//! The seed is parsed as an untyped [`serde_json::Value`] on purpose, so the generator needs no
//! `serde` derive and is not coupled to any of the library's feature modules. Only the
//! classification *codes* are read; the emitted table contains no ISO descriptive text.

use serde_json::Value;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// A group reduced to its code plus one 26-bit letter bitmask per CFI attribute position.
struct Group {
    code: u8,
    /// For each of the four attributes, `letters[i]` is the sorted list of permitted code letters.
    letters: [Vec<u8>; 4],
    masks: [u32; 4],
}

/// A category reduced to its code and its groups (sorted by code).
struct Category {
    code: u8,
    groups: Vec<Group>,
}

fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let seed_path = Path::new(manifest_dir).join("data/cfi.json");
    let out_path = Path::new(manifest_dir).join("src/cfi/table.rs");

    let raw = fs::read_to_string(&seed_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", seed_path.display()));
    let json: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", seed_path.display()));

    let categories = parse_categories(&json);
    let rendered = render(&categories);

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("failed to create {}: {e}", parent.display()));
    }
    fs::write(&out_path, rendered)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));

    println!("wrote {}", out_path.display());
    println!("run `cargo fmt` to normalize the output (the `just` recipes do this for you)");
}

/// Reads the `categories` array into sorted-by-code `Category` values.
fn parse_categories(json: &Value) -> Vec<Category> {
    let cats = json["categories"]
        .as_array()
        .expect("top-level `categories` must be an array");

    let mut out: Vec<Category> = cats
        .iter()
        .map(|cat| {
            let code = single_letter(&cat["code"], "category code");
            let groups = cat["groups"]
                .as_array()
                .expect("`groups` must be an array")
                .iter()
                .map(parse_group)
                .collect::<Vec<_>>();
            let mut groups = groups;
            groups.sort_by_key(|g| g.code);
            Category { code, groups }
        })
        .collect();

    out.sort_by_key(|c| c.code);
    out
}

fn parse_group(group: &Value) -> Group {
    let code = single_letter(&group["code"], "group code");
    let attr_keys = ["attribute1", "attribute2", "attribute3", "attribute4"];

    let mut letters: [Vec<u8>; 4] = Default::default();
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
        assert!(
            mask != 0,
            "attribute `{key}` has no values in group {}",
            code as char
        );

        letters[i] = (0u8..26)
            .filter(|bit| (mask >> bit) & 1 == 1)
            .map(|bit| b'A' + bit)
            .collect();
        masks[i] = mask;
    }

    Group {
        code,
        letters,
        masks,
    }
}

/// Extracts a single uppercase ASCII letter from a JSON string node, or panics with context.
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

/// Renders the committed `src/cfi/table.rs` source.
fn render(categories: &[Category]) -> String {
    let mut s = String::new();

    s.push_str(HEADER);

    s.push_str("pub(crate) static CATEGORIES: &[CategoryEntry] = &[\n");
    for cat in categories {
        writeln!(s, "    CategoryEntry {{").unwrap();
        writeln!(s, "        code: b'{}',", cat.code as char).unwrap();
        writeln!(s, "        groups: &[").unwrap();
        for g in &cat.groups {
            let annotations = (0..4)
                .map(|i| {
                    let letters: String = g.letters[i].iter().map(|&b| b as char).collect();
                    format!("a{}={letters}", i + 1)
                })
                .collect::<Vec<_>>()
                .join(" ");
            writeln!(s, "            // group {}: {annotations}", g.code as char).unwrap();
            writeln!(
                s,
                "            GroupEntry {{ code: b'{}', attrs: [0x{:08X}, 0x{:08X}, 0x{:08X}, 0x{:08X}] }},",
                g.code as char, g.masks[0], g.masks[1], g.masks[2], g.masks[3]
            )
            .unwrap();
        }
        writeln!(s, "        ],").unwrap();
        writeln!(s, "    }},").unwrap();
    }
    s.push_str("];\n");

    s
}

const HEADER: &str = "\
// @generated by `just cfi-generate` from data/cfi.json — DO NOT EDIT BY HAND.
//
// Derived from the ISO 10962 (CFI) classification *code* structure only: the category, group,
// and attribute-value code letters and which combinations are valid. No ISO descriptive text is
// reproduced here. Regenerate with `just cfi-generate`; `just cfi-check` verifies there is no drift.

/// A single group within a category: its code, plus a bitmask of the permitted code letters for
/// each of the four CFI attribute positions.
///
/// In `attrs[i]`, bit `n` (from the least-significant) is set when the letter `b'A' + n` is a
/// valid code for attribute `i + 1`. Every mask has at least one bit set.
pub(crate) struct GroupEntry {
    pub code: u8,
    pub attrs: [u32; 4],
}

/// One ISO 10962 category: its code and the groups defined under it, both sorted by `code` so the
/// lookup in `super::validation` can binary-search them.
pub(crate) struct CategoryEntry {
    pub code: u8,
    pub groups: &'static [GroupEntry],
}

";
