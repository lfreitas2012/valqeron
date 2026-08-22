use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use valqeron_core::identifiers::Mic;

const NYSE: &str = "XNYS";
const SEGMENT: &str = "ARCX";
const DIGIT_FIRST: &str = "360T";
const UNREGISTERED: &str = "ZZZZ";
const BAD_CHARACTER: &str = "XN.S";
const WRONG_LENGTH: &str = "XNYSE";

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("Mic::parse");

    group.bench_function("registered operating (XNYS)", |b| {
        b.iter(|| Mic::parse(black_box(NYSE)))
    });
    group.bench_function("registered segment (ARCX)", |b| {
        b.iter(|| Mic::parse(black_box(SEGMENT)))
    });
    group.bench_function("registered digit first (360T)", |b| {
        b.iter(|| Mic::parse(black_box(DIGIT_FIRST)))
    });
    group.bench_function("unregistered", |b| {
        b.iter(|| Mic::parse(black_box(UNREGISTERED)))
    });
    group.bench_function("invalid character", |b| {
        b.iter(|| Mic::parse(black_box(BAD_CHARACTER)))
    });
    group.bench_function("wrong length", |b| {
        b.iter(|| Mic::parse(black_box(WRONG_LENGTH)))
    });

    group.finish();
}

fn bench_validation_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("Mic::from_bytes (pre-normalized)");

    let nyse_bytes = *Mic::parse(NYSE).unwrap().as_bytes();
    let segment_bytes = *Mic::parse(SEGMENT).unwrap().as_bytes();

    group.bench_function("registered operating (XNYS)", |b| {
        b.iter(|| Mic::from_bytes(black_box(nyse_bytes)))
    });
    group.bench_function("registered segment (ARCX)", |b| {
        b.iter(|| Mic::from_bytes(black_box(segment_bytes)))
    });

    group.finish();
}

fn bench_accessors(c: &mut Criterion) {
    let mic = Mic::parse(SEGMENT).unwrap();

    let mut group = c.benchmark_group("Mic methods");

    group.bench_function("as_str", |b| b.iter(|| black_box(mic.as_str())));
    group.bench_function("as_bytes", |b| b.iter(|| black_box(mic.as_bytes())));
    group.bench_function("is_active", |b| b.iter(|| black_box(mic.is_active())));
    group.bench_function("is_operating", |b| b.iter(|| black_box(mic.is_operating())));
    group.bench_function("operating_mic", |b| {
        b.iter(|| black_box(mic.operating_mic()))
    });
    group.bench_function("country_code", |b| b.iter(|| black_box(mic.country_code())));

    group.finish();
}

criterion_group!(benches, bench_parse, bench_validation_only, bench_accessors);
criterion_main!(benches);
