use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use valqeron_identifiers::Cfi;

// Equity / common share: a fully-populated taxonomy path (category, group, four attributes).
const EQUITY: &str = "ESVUFR";
// Debt / bond: a different category with wide attribute sets.
const DEBT: &str = "DBFTFB";
// Unknown category: exercises the earliest taxonomy exit (position 1 lookup fails).
const UNKNOWN_CATEGORY: &str = "QSVUFR";
// Valid category/group but an attribute code the standard does not permit (late exit).
const INVALID_ATTRIBUTE: &str = "ESZUFR";

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("Cfi::parse");

    group.bench_function("equity", |b| b.iter(|| Cfi::parse(black_box(EQUITY))));
    group.bench_function("debt", |b| b.iter(|| Cfi::parse(black_box(DEBT))));
    group.bench_function("unknown category (early exit)", |b| {
        b.iter(|| Cfi::parse(black_box(UNKNOWN_CATEGORY)))
    });
    group.bench_function("invalid attribute (late exit)", |b| {
        b.iter(|| Cfi::parse(black_box(INVALID_ATTRIBUTE)))
    });

    group.finish();
}

fn bench_validation_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("Cfi::from_bytes (pre-normalized)");

    let equity_bytes = *Cfi::parse(EQUITY).unwrap().as_bytes();
    let debt_bytes = *Cfi::parse(DEBT).unwrap().as_bytes();

    group.bench_function("equity", |b| {
        b.iter(|| Cfi::from_bytes(black_box(equity_bytes)))
    });
    group.bench_function("debt", |b| {
        b.iter(|| Cfi::from_bytes(black_box(debt_bytes)))
    });

    group.finish();
}

fn bench_accessors(c: &mut Criterion) {
    let cfi = Cfi::parse(EQUITY).unwrap();

    let mut group = c.benchmark_group("Cfi methods");

    group.bench_function("category", |b| b.iter(|| black_box(cfi.category())));
    group.bench_function("group", |b| b.iter(|| black_box(cfi.group())));
    group.bench_function("attributes", |b| b.iter(|| black_box(cfi.attributes())));

    group.finish();
}

criterion_group!(benches, bench_parse, bench_validation_only, bench_accessors);
criterion_main!(benches);
