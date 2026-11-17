//! Criterion benchmarks for the oxideav-core hot primitives.
//!
//! These are the paths every codec crate leans on in its inner loops:
//! the MSB/LSB bit readers and writers (`oxideav_core::bits`), the
//! rescale kernel (`oxideav_core::time`), and `Rational` arithmetic.
//! The harness gives optimisation rounds a stable, deterministic,
//! fixture-free baseline to A/B against.
//!
//! Every input is synthesised in-bench from a fixed-seed xorshift64
//! stream — no `docs/` fixtures or external files are read. Run with:
//!
//!     cargo bench -p oxideav-core --bench primitives

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

use oxideav_core::bits::{BitReader, BitReaderLsb, BitWriter, BitWriterLsb};
use oxideav_core::rational::Rational;
use oxideav_core::time::{Rounding, TimeBase, rescale, rescale_rnd};

/// Fixed-seed xorshift64 — deterministic synthetic input.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// A deterministic mixed-width field schedule (the shape codec headers
/// and residual streams actually have: mostly narrow fields, a few
/// wide ones).
fn field_schedule(n: usize) -> Vec<(u32, u32)> {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    (0..n)
        .map(|_| {
            let w = match rng.next() % 10 {
                0..=4 => 1 + (rng.next() % 4) as u32, // 1..=4 bits
                5..=7 => 5 + (rng.next() % 8) as u32, // 5..=12 bits
                8 => 13 + (rng.next() % 12) as u32,   // 13..=24 bits
                _ => 25 + (rng.next() % 8) as u32,    // 25..=32 bits
            };
            (rng.next() as u32, w)
        })
        .collect()
}

fn bench_bits(c: &mut Criterion) {
    const FIELDS: usize = 64 * 1024;
    let schedule = field_schedule(FIELDS);
    let total_bits: u64 = schedule.iter().map(|&(_, w)| w as u64).sum();

    let mut group = c.benchmark_group("bits");
    group.throughput(Throughput::Bytes(total_bits / 8));

    group.bench_function("write_msb", |b| {
        b.iter(|| {
            let mut w = BitWriter::with_capacity((total_bits / 8 + 1) as usize);
            for &(v, n) in &schedule {
                w.write_u32(v, n);
            }
            black_box(w.finish())
        })
    });

    group.bench_function("write_lsb", |b| {
        b.iter(|| {
            let mut w = BitWriterLsb::with_capacity((total_bits / 8 + 1) as usize);
            for &(v, n) in &schedule {
                w.write_u32(v, n);
            }
            black_box(w.finish())
        })
    });

    let mut w = BitWriter::new();
    for &(v, n) in &schedule {
        w.write_u32(v, n);
    }
    let msb_bytes = w.finish();
    group.bench_function("read_msb", |b| {
        b.iter(|| {
            let mut r = BitReader::new(&msb_bytes);
            let mut acc = 0u32;
            for &(_, n) in &schedule {
                acc = acc.wrapping_add(r.read_u32(n).unwrap());
            }
            black_box(acc)
        })
    });

    let mut w = BitWriterLsb::new();
    for &(v, n) in &schedule {
        w.write_u32(v, n);
    }
    let lsb_bytes = w.finish();
    group.bench_function("read_lsb", |b| {
        b.iter(|| {
            let mut r = BitReaderLsb::new(&lsb_bytes);
            let mut acc = 0u32;
            for &(_, n) in &schedule {
                acc = acc.wrapping_add(r.read_u32(n).unwrap());
            }
            black_box(acc)
        })
    });

    // Rice/unary-style stream: short runs dominate, occasional long.
    let mut rng = Rng(0xFEED_FACE_CAFE_BEEF);
    let counts: Vec<u32> = (0..FIELDS)
        .map(|_| {
            if rng.next().is_multiple_of(32) {
                (rng.next() % 200) as u32
            } else {
                (rng.next() % 8) as u32
            }
        })
        .collect();
    let mut w = BitWriter::new();
    for &cnt in &counts {
        w.write_unary(cnt);
    }
    let unary_bytes = w.finish();
    group.throughput(Throughput::Elements(FIELDS as u64));
    group.bench_function("read_unary", |b| {
        b.iter(|| {
            let mut r = BitReader::new(&unary_bytes);
            let mut acc = 0u32;
            for _ in 0..counts.len() {
                acc = acc.wrapping_add(r.read_unary().unwrap());
            }
            black_box(acc)
        })
    });

    group.finish();
}

fn bench_rescale(c: &mut Criterion) {
    const N: usize = 64 * 1024;
    let mut rng = Rng(0x0DDB_A11D_EAD5_0DA5);
    let values: Vec<i64> = (0..N).map(|_| (rng.next() >> 20) as i64).collect();

    let mut group = c.benchmark_group("rescale");
    group.throughput(Throughput::Elements(N as u64));

    // The everyday shape: 90 kHz PTS → milliseconds.
    group.bench_function("mpegts_to_millis", |b| {
        b.iter(|| {
            let mut acc = 0i64;
            for &v in &values {
                acc = acc.wrapping_add(TimeBase::MPEG_TS.rescale(v, TimeBase::MILLIS));
            }
            black_box(acc)
        })
    });

    group.bench_function("floor_mode", |b| {
        b.iter(|| {
            let mut acc = 0i64;
            for &v in &values {
                acc = acc.wrapping_add(rescale_rnd(
                    v,
                    Rational::new(1, 90_000),
                    Rational::new(1, 1_000),
                    Rounding::Floor,
                ));
            }
            black_box(acc)
        })
    });

    // Non-trivial factor (NTSC frame rate → 48 kHz audio clock).
    group.bench_function("ntsc_to_48k", |b| {
        b.iter(|| {
            let mut acc = 0i64;
            for &v in &values {
                acc = acc.wrapping_add(rescale(
                    v,
                    Rational::new(1001, 30_000),
                    Rational::new(1, 48_000),
                ));
            }
            black_box(acc)
        })
    });

    group.finish();
}

fn bench_rational(c: &mut Criterion) {
    const N: usize = 16 * 1024;
    let mut rng = Rng(0x5EED_5EED_5EED_5EED);
    let pairs: Vec<(Rational, Rational)> = (0..N)
        .map(|_| {
            (
                Rational::new(
                    (rng.next() % 100_000) as i64 + 1,
                    (rng.next() % 100_000) as i64 + 1,
                ),
                Rational::new(
                    (rng.next() % 100_000) as i64 + 1,
                    (rng.next() % 100_000) as i64 + 1,
                ),
            )
        })
        .collect();

    let mut group = c.benchmark_group("rational");
    group.throughput(Throughput::Elements(N as u64));

    group.bench_function("add", |b| {
        b.iter_batched(
            || pairs.clone(),
            |pairs| {
                let mut acc = 0i64;
                for (a, x) in pairs {
                    let r = a + x;
                    acc = acc.wrapping_add(r.num).wrapping_add(r.den);
                }
                black_box(acc)
            },
            BatchSize::LargeInput,
        )
    });

    group.bench_function("mul", |b| {
        b.iter(|| {
            let mut acc = 0i64;
            for &(a, x) in &pairs {
                let r = a * x;
                acc = acc.wrapping_add(r.num).wrapping_add(r.den);
            }
            black_box(acc)
        })
    });

    group.bench_function("cmp_value", |b| {
        b.iter(|| {
            let mut acc = 0i64;
            for &(a, x) in &pairs {
                acc = acc.wrapping_add(a.cmp_value(&x) as i64);
            }
            black_box(acc)
        })
    });

    group.bench_function("reduced", |b| {
        b.iter(|| {
            let mut acc = 0i64;
            for &(a, _) in &pairs {
                let r = a.reduced();
                acc = acc.wrapping_add(r.num).wrapping_add(r.den);
            }
            black_box(acc)
        })
    });

    group.finish();
}

criterion_group!(benches, bench_bits, bench_rescale, bench_rational);
criterion_main!(benches);
