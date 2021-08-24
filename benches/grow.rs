use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use segvec::SegVec;

const SIZES: &[usize] = &[2, 32, 64, 256, 1024, 2048, 4096];
const SOME_DATA: &[&str] = &["hello world"; 4096];

fn bench_extend(c: &mut Criterion) {
    let mut group = c.benchmark_group("extend_twice");
    for i in SIZES {
        group.bench_with_input(BenchmarkId::new("Vec", i), i, |b, i| {
            let low_half = &SOME_DATA[0..i / 2];
            let high_half = &SOME_DATA[i / 2..*i];
            b.iter_with_large_drop(|| {
                let mut v = Vec::<&str>::default();
                v.extend(low_half.iter().copied());
                v.extend(high_half.iter().copied());
            })
        });
        group.bench_with_input(BenchmarkId::new("SegVec", i), i, |b, i| {
            let low_half = &SOME_DATA[0..i / 2];
            let high_half = &SOME_DATA[i / 2..*i];
            b.iter_with_large_drop(|| {
                let mut v = SegVec::default();
                v.extend(low_half.iter().copied());
                v.extend(high_half.iter().copied());
            })
        });
    }
    group.finish();
}

fn bench_collect(c: &mut Criterion) {
    let mut group = c.benchmark_group("collect");
    for i in SIZES {
        group.bench_with_input(BenchmarkId::new("Vec", i), i, |b, i| {
            let mut v = Vec::new();
            b.iter(|| {
                v = SOME_DATA[..*i].iter().copied().collect::<Vec<&str>>();
            });
            drop(v);
        });
        group.bench_with_input(BenchmarkId::new("SegVec", i), i, |b, i| {
            let mut v = SegVec::new();
            b.iter(|| {
                v = SOME_DATA[..*i].iter().copied().collect::<SegVec<&str>>();
            });
            drop(v);
        });
    }
    group.finish();
}

// This is a separate benchmark from `extend`, since both `Vec` and `SegVec`
// will try to be smart about extending from an iterator with a size hint...
fn bench_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("push");
    for i in SIZES {
        group.bench_with_input(BenchmarkId::new("Vec", i), i, |b, i| {
            b.iter_with_large_drop(|| {
                let mut v = Vec::<&str>::default();
                for &elem in &SOME_DATA[0..*i] {
                    v.push(elem);
                }
            })
        });
        group.bench_with_input(BenchmarkId::new("SegVec", i), i, |b, i| {
            b.iter_with_large_drop(|| {
                let mut v = SegVec::default();
                for &elem in &SOME_DATA[0..*i] {
                    v.push(elem);
                }
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_extend, bench_push, bench_collect);
criterion_main!(benches);
