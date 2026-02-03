//! Benchmarks for DoubleBuffer O(1) swap (bd-1rz0.4.4)
//!
//! Performance budgets:
//! - Swap: < 10ns (O(1) index flip)
//! - Clone (baseline): ~70,000ns for 120x40
//! - Clear: ~15,000ns for 120x40
//!
//! Expected improvement: swap is ~10,000x faster than clone.
//!
//! Run with: cargo bench -p ftui-render --bench double_buffer_bench

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use ftui_render::buffer::{Buffer, DoubleBuffer};
use std::hint::black_box;

// =============================================================================
// DoubleBuffer swap vs Buffer clone
// =============================================================================

fn bench_swap_vs_clone(c: &mut Criterion) {
    let mut group = c.benchmark_group("double_buffer/transition");

    for (w, h) in [(80, 24), (120, 40), (200, 60)] {
        let cells = w as u64 * h as u64;
        group.throughput(Throughput::Elements(cells));

        // Baseline: Buffer::clone (the old approach)
        let buf = Buffer::new(w, h);
        group.bench_with_input(
            BenchmarkId::new("clone", format!("{w}x{h}")),
            &buf,
            |b, buf| b.iter(|| black_box(buf.clone())),
        );

        // New approach: DoubleBuffer::swap
        let mut db = DoubleBuffer::new(w, h);
        group.bench_with_input(BenchmarkId::new("swap", format!("{w}x{h}")), &(), |b, _| {
            b.iter(|| {
                db.swap();
                black_box(&db);
            })
        });

        // Clear (still needed after swap)
        let mut clear_buf = Buffer::new(w, h);
        group.bench_with_input(
            BenchmarkId::new("clear", format!("{w}x{h}")),
            &(),
            |b, _| {
                b.iter(|| {
                    clear_buf.clear();
                    black_box(&clear_buf);
                })
            },
        );

        // Combined: swap + clear (actual frame transition cost)
        let mut db_combined = DoubleBuffer::new(w, h);
        group.bench_with_input(
            BenchmarkId::new("swap_and_clear", format!("{w}x{h}")),
            &(),
            |b, _| {
                b.iter(|| {
                    db_combined.swap();
                    db_combined.current_mut().clear();
                    black_box(&db_combined);
                })
            },
        );
    }

    group.finish();
}

// =============================================================================
// DoubleBuffer operations
// =============================================================================

fn bench_double_buffer_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("double_buffer/ops");

    // New allocation
    for (w, h) in [(80, 24), (120, 40), (200, 60)] {
        let cells = w as u64 * h as u64;
        group.throughput(Throughput::Elements(cells));
        group.bench_with_input(
            BenchmarkId::new("new", format!("{w}x{h}")),
            &(w, h),
            |b, &(w, h)| b.iter(|| black_box(DoubleBuffer::new(w, h))),
        );
    }

    // Resize
    let mut db = DoubleBuffer::new(80, 24);
    group.bench_function("resize_80x24_to_120x40", |b| {
        b.iter(|| {
            db.resize(120, 40);
            db.resize(80, 24); // Reset for next iteration
            black_box(&db);
        })
    });

    // Access patterns
    let db = DoubleBuffer::new(120, 40);
    group.bench_function("current_ref_120x40", |b| b.iter(|| black_box(db.current())));

    group.bench_function("previous_ref_120x40", |b| {
        b.iter(|| black_box(db.previous()))
    });

    let mut db_mut = DoubleBuffer::new(120, 40);
    group.bench_function("current_mut_120x40", |b| {
        b.iter(|| {
            let _ = black_box(db_mut.current_mut());
        })
    });

    // Dimension queries
    let db = DoubleBuffer::new(120, 40);
    group.bench_function("dimensions_match", |b| {
        b.iter(|| black_box(db.dimensions_match(120, 40)))
    });

    group.finish();
}

// =============================================================================
// Frame transition simulation
// =============================================================================

fn bench_frame_transition_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("double_buffer/frame_sim");

    for (w, h) in [(80, 24), (120, 40)] {
        let cells = w as u64 * h as u64;
        group.throughput(Throughput::Elements(cells));

        // Old approach: clone buffer each frame
        group.bench_with_input(
            BenchmarkId::new("clone_per_frame", format!("{w}x{h}")),
            &(w, h),
            |b, &(w, h)| {
                let mut current = Buffer::new(w, h);
                b.iter(|| {
                    // Simulate: prev = current.clone(), then render to current
                    let _prev = current.clone();
                    current.clear();
                    black_box(&current);
                })
            },
        );

        // New approach: swap + clear
        group.bench_with_input(
            BenchmarkId::new("swap_per_frame", format!("{w}x{h}")),
            &(w, h),
            |b, &(w, h)| {
                let mut db = DoubleBuffer::new(w, h);
                b.iter(|| {
                    db.swap();
                    db.current_mut().clear();
                    black_box(&db);
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_swap_vs_clone,
    bench_double_buffer_operations,
    bench_frame_transition_simulation,
);
criterion_main!(benches);
