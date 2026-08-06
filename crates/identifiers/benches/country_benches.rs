use criterion::{Criterion, criterion_group, criterion_main};
use valqeron_identifiers::CountryCode;
use std::hint::black_box;

// A common assigned code.
const US: &str = "US";
// A different assigned code, later in byte order.
const BRAZIL: &str = "BR";
// Well formed but not assigned: exercises the membership rejection path.
const UNASSIGNED: &str = "ZZ";
// A non letter in the second position: exercises the character class rejection path.
const BAD_CHARACTER: &str = "U1";
// Wrong length: exercises the earliest exit in the parser.
const WRONG_LENGTH: &str = "USA";

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("CountryCode::parse");

    group.bench_function("assigned (US)", |b| {
        b.iter(|| CountryCode::parse(black_box(US)))
    });
    group.bench_function("assigned (BR)", |b| {
        b.iter(|| CountryCode::parse(black_box(BRAZIL)))
    });
    group.bench_function("unassigned", |b| {
        b.iter(|| CountryCode::parse(black_box(UNASSIGNED)))
    });
    group.bench_function("invalid character", |b| {
        b.iter(|| CountryCode::parse(black_box(BAD_CHARACTER)))
    });
    group.bench_function("wrong length", |b| {
        b.iter(|| CountryCode::parse(black_box(WRONG_LENGTH)))
    });

    group.finish();
}

fn bench_validation_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("CountryCode::from_bytes (pre-normalized)");

    let us_bytes = *CountryCode::parse(US).unwrap().as_bytes();
    let br_bytes = *CountryCode::parse(BRAZIL).unwrap().as_bytes();

    group.bench_function("assigned (US)", |b| {
        b.iter(|| CountryCode::from_bytes(black_box(us_bytes)))
    });
    group.bench_function("assigned (BR)", |b| {
        b.iter(|| CountryCode::from_bytes(black_box(br_bytes)))
    });

    group.finish();
}

fn bench_accessors(c: &mut Criterion) {
    let code = CountryCode::parse(US).unwrap();

    let mut group = c.benchmark_group("CountryCode methods");

    group.bench_function("as_str", |b| b.iter(|| black_box(code.as_str())));
    group.bench_function("as_bytes", |b| b.iter(|| black_box(code.as_bytes())));

    group.finish();
}

criterion_group!(benches, bench_parse, bench_validation_only, bench_accessors);
criterion_main!(benches);
