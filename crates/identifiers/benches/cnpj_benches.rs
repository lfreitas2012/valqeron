#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use criterion::{Criterion, criterion_group, criterion_main};
use std::fmt::Write;
use std::hint::black_box;
use valqeron_identifiers::Cnpj;

const NUMERIC_PUNCTUATED: &str = "00.000.000/0001-91";
const NUMERIC_RAW: &str = "00000000000191";
const ALPHANUMERIC_PUNCTUATED: &str = "12.ABC.345/01DE-35";
const ALPHANUMERIC_RAW: &str = "12ABC34501DE35";
const INVALID_CHECKSUM: &str = "12.ABC.345/01DE-30";

/// A fixed-size stack buffer to measure pure `fmt::Display` overhead without triggering the
/// system's heap allocator.
struct StackBuffer {
    buf: [u8; 18],
    pos: usize,
}

impl StackBuffer {
    #[inline]
    fn new() -> Self {
        Self {
            buf: [0; 18],
            pos: 0,
        }
    }
}

impl Write for StackBuffer {
    #[inline]
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let bytes = s.as_bytes();
        let end = self.pos + bytes.len();
        if end > self.buf.len() {
            return Err(std::fmt::Error);
        }
        self.buf[self.pos..end].copy_from_slice(bytes);
        self.pos = end;
        Ok(())
    }
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("Cnpj::parse");

    group.bench_function("numeric, punctuated", |b| {
        b.iter(|| Cnpj::parse(black_box(NUMERIC_PUNCTUATED)))
    });
    group.bench_function("numeric, raw", |b| {
        b.iter(|| Cnpj::parse(black_box(NUMERIC_RAW)))
    });
    group.bench_function("alphanumeric, punctuated", |b| {
        b.iter(|| Cnpj::parse(black_box(ALPHANUMERIC_PUNCTUATED)))
    });
    group.bench_function("alphanumeric, raw", |b| {
        b.iter(|| Cnpj::parse(black_box(ALPHANUMERIC_RAW)))
    });
    // Tests the "early exit" performance when validation fails
    group.bench_function("invalid checksum (early exit)", |b| {
        b.iter(|| Cnpj::parse(black_box(INVALID_CHECKSUM)))
    });

    group.finish();
}

fn bench_validation_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("Cnpj::from_bytes (pre-normalized)");

    // Extract valid raw byte arrays
    let numeric_bytes = *Cnpj::parse(NUMERIC_RAW).unwrap().as_bytes();
    let alpha_bytes = *Cnpj::parse(ALPHANUMERIC_RAW).unwrap().as_bytes();

    group.bench_function("numeric", |b| {
        b.iter(|| Cnpj::from_bytes(black_box(numeric_bytes)))
    });
    group.bench_function("alphanumeric", |b| {
        b.iter(|| Cnpj::from_bytes(black_box(alpha_bytes)))
    });

    group.finish();
}

fn bench_formatting(c: &mut Criterion) {
    let cnpj_num = Cnpj::parse(NUMERIC_RAW).unwrap();
    let cnpj_alpha = Cnpj::parse(ALPHANUMERIC_RAW).unwrap();

    let mut group = c.benchmark_group("Cnpj formatting");

    // Tests full formatting + string heap allocation
    group.bench_function("to_string() (allocates)", |b| {
        b.iter(|| black_box(cnpj_num).to_string())
    });

    // Tests formatting overhead minus the String allocation
    group.bench_function("write! to String (reusable)", |b| {
        let mut buf = String::with_capacity(18);
        b.iter(|| {
            buf.clear();

            write!(&mut buf, "{}", black_box(cnpj_num).formatted()).unwrap();
            black_box(&buf);
        })
    });

    // Absolute purest measure: rendering output to a stack array (zero heap)
    group.bench_function("write! to StackBuffer (zero heap, numeric)", |b| {
        b.iter(|| {
            let mut buf = StackBuffer::new();
            write!(&mut buf, "{}", black_box(cnpj_num).formatted()).unwrap();
            black_box(buf)
        })
    });

    // Absolute purest measure: rendering output to a stack array (zero heap)
    group.bench_function("write! to StackBuffer (zero heap, alphanumeric)", |b| {
        b.iter(|| {
            let mut buf = StackBuffer::new();
            write!(&mut buf, "{}", black_box(cnpj_alpha).formatted()).unwrap();
            black_box(buf)
        })
    });

    group.finish();
}

fn bench_accessors(c: &mut Criterion) {
    let cnpj_num = Cnpj::parse(NUMERIC_RAW).unwrap();
    let cnpj_alpha = Cnpj::parse(ALPHANUMERIC_RAW).unwrap();

    let mut group = c.benchmark_group("Cnpj methods");

    group.bench_function("root", |b| b.iter(|| black_box(cnpj_alpha.root())));
    group.bench_function("branch_code", |b| {
        b.iter(|| black_box(cnpj_alpha.branch_code()))
    });

    group.bench_function("is_root (matriz)", |b| {
        b.iter(|| black_box(cnpj_num.is_root()))
    });
    group.bench_function("check_digits", |b| {
        b.iter(|| black_box(cnpj_alpha.check_digits()))
    });
    group.bench_function("branch_number (numeric)", |b| {
        b.iter(|| black_box(cnpj_num.branch_number()))
    });
    group.bench_function("branch_number (alphanumeric)", |b| {
        b.iter(|| black_box(cnpj_alpha.branch_number()))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parse,
    bench_validation_only,
    bench_formatting,
    bench_accessors
);
criterion_main!(benches);
