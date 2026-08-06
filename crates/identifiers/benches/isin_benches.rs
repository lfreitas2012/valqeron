use criterion::{Criterion, criterion_group, criterion_main};
use valqeron_identifiers::Isin;
use std::hint::black_box;

// Amazon: an all-numeric NSIN (no letter expansion in the Luhn pass).
const NUMERIC_NSIN: &str = "US0231351067";
// Petrobras ON: an all-letter NSIN (every character expands to two Luhn digits).
const ALPHANUMERIC_NSIN: &str = "BRPETRACNOR9";
// A single wrong check digit, to measure the validation early-exit path.
const INVALID_CHECKSUM: &str = "US0378331006";

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("Isin::parse");

    group.bench_function("numeric NSIN", |b| {
        b.iter(|| Isin::parse(black_box(NUMERIC_NSIN)))
    });
    group.bench_function("alphanumeric NSIN", |b| {
        b.iter(|| Isin::parse(black_box(ALPHANUMERIC_NSIN)))
    });
    // Tests the "early exit" performance when the checksum fails.
    group.bench_function("invalid checksum (early exit)", |b| {
        b.iter(|| Isin::parse(black_box(INVALID_CHECKSUM)))
    });

    group.finish();
}

fn bench_validation_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("Isin::from_bytes (pre-normalized)");

    let numeric_bytes = *Isin::parse(NUMERIC_NSIN).unwrap().as_bytes();
    let alpha_bytes = *Isin::parse(ALPHANUMERIC_NSIN).unwrap().as_bytes();

    group.bench_function("numeric NSIN", |b| {
        b.iter(|| Isin::from_bytes(black_box(numeric_bytes)))
    });
    group.bench_function("alphanumeric NSIN", |b| {
        b.iter(|| Isin::from_bytes(black_box(alpha_bytes)))
    });

    group.finish();
}

fn bench_accessors(c: &mut Criterion) {
    let isin_num = Isin::parse(NUMERIC_NSIN).unwrap();
    let isin_alpha = Isin::parse(ALPHANUMERIC_NSIN).unwrap();

    let mut group = c.benchmark_group("Isin methods");

    group.bench_function("country_code", |b| {
        b.iter(|| black_box(isin_alpha.country_code()))
    });
    group.bench_function("nsin", |b| b.iter(|| black_box(isin_alpha.nsin())));
    group.bench_function("check_digit", |b| {
        b.iter(|| black_box(isin_num.check_digit()))
    });
    group.bench_function("computed_check_digit (numeric)", |b| {
        b.iter(|| black_box(isin_num.computed_check_digit()))
    });
    group.bench_function("computed_check_digit (alphanumeric)", |b| {
        b.iter(|| black_box(isin_alpha.computed_check_digit()))
    });

    group.finish();
}

criterion_group!(benches, bench_parse, bench_validation_only, bench_accessors);
criterion_main!(benches);
