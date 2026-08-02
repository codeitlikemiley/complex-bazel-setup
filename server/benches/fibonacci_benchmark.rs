// Criterion benchmarks for the four fibonacci implementations in the server
// library. The algorithms and their correctness tests live in src/lib.rs --
// `harness = false` means cargo compiles #[test] functions here and then never
// runs them, so assertions in a bench file are invisible to `cargo test`.
//
// Declared in Cargo.toml as `[[bench]] name = "fibonacci_benchmark", harness = false`.
// Under Bazel this is //server:bench, tagged "manual". Run it with:
//     cargo bench
//     bazel run -c opt //server:bench -- --bench

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use server::{fib_iterative, fib_matrix, fib_memoized, fib_recursive};

fn benchmark_recursive(c: &mut Criterion) {
    let mut group = c.benchmark_group("fibonacci_recursive");

    for n in [10, 15, 20, 25, 30].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(n), n, |b, &n| {
            b.iter(|| fib_recursive(black_box(n)));
        });
    }

    group.finish();
}

fn benchmark_iterative(c: &mut Criterion) {
    let mut group = c.benchmark_group("fibonacci_iterative");

    for n in [10, 20, 30, 40, 50, 60, 70, 80, 90].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(n), n, |b, &n| {
            b.iter(|| fib_iterative(black_box(n)));
        });
    }

    group.finish();
}

fn benchmark_memoized(c: &mut Criterion) {
    let mut group = c.benchmark_group("fibonacci_memoized");

    for n in [10, 20, 30, 40, 50, 60, 70, 80, 90].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(n), n, |b, &n| {
            b.iter(|| fib_memoized(black_box(n)));
        });
    }

    group.finish();
}

fn benchmark_matrix(c: &mut Criterion) {
    let mut group = c.benchmark_group("fibonacci_matrix");

    for n in [10, 20, 30, 40, 50, 60, 70, 80, 90].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(n), n, |b, &n| {
            b.iter(|| fib_matrix(black_box(n)));
        });
    }

    group.finish();
}

fn benchmark_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("fibonacci_comparison");

    let n = 30;

    group.bench_function("recursive_30", |b| {
        b.iter(|| fib_recursive(black_box(n)));
    });

    group.bench_function("iterative_30", |b| {
        b.iter(|| fib_iterative(black_box(n)));
    });

    group.bench_function("memoized_30", |b| {
        b.iter(|| fib_memoized(black_box(n)));
    });

    group.bench_function("matrix_30", |b| {
        b.iter(|| fib_matrix(black_box(n)));
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_recursive,
    benchmark_iterative,
    benchmark_memoized,
    benchmark_matrix,
    benchmark_comparison
);
criterion_main!(benches);
