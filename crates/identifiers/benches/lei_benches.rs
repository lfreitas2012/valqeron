#![allow(clippy::unwrap_used)]

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use valqeron_identifiers::Lei;

const MIXED_BASE: &str = "5493000IBP32UQZ0KL24";
const LETTER_HEAVY_BASE: &str = "54930084UKLVMY22DS16";
const INVALID_CHECKSUM: &str = "5493000IBP32UQZ0KL25";

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("Lei::parse");

    group.bench_function("mixed base", |b| {
        b.iter(|| Lei::parse(black_box(MIXED_BASE)))
    });
    group.bench_function("letter-heavy base", |b| {
        b.iter(|| Lei::parse(black_box(LETTER_HEAVY_BASE)))
    });
    // Tests the "early exit" performance when the checksum fails.
    group.bench_function("invalid checksum (early exit)", |b| {
        b.iter(|| Lei::parse(black_box(INVALID_CHECKSUM)))
    });

    group.finish();
}

fn bench_validation_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("Lei::from_bytes (pre-normalized)");

    let mixed_bytes = *Lei::parse(MIXED_BASE).unwrap().as_bytes();
    let letter_bytes = *Lei::parse(LETTER_HEAVY_BASE).unwrap().as_bytes();

    group.bench_function("mixed base", |b| {
        b.iter(|| Lei::from_bytes(black_box(mixed_bytes)))
    });
    group.bench_function("letter-heavy base", |b| {
        b.iter(|| Lei::from_bytes(black_box(letter_bytes)))
    });

    group.finish();
}

fn bench_accessors(c: &mut Criterion) {
    let lei = Lei::parse(MIXED_BASE).unwrap();

    let mut group = c.benchmark_group("Lei methods");

    group.bench_function("lou_prefix", |b| b.iter(|| black_box(lei.lou_prefix())));
    group.bench_function("entity_id", |b| b.iter(|| black_box(lei.entity_id())));
    group.bench_function("check_digits", |b| b.iter(|| black_box(lei.check_digits())));
    group.bench_function("computed_check_digits", |b| {
        b.iter(|| black_box(lei.computed_check_digits()))
    });

    group.finish();
}

criterion_group!(benches, bench_parse, bench_validation_only, bench_accessors);
criterion_main!(benches);
