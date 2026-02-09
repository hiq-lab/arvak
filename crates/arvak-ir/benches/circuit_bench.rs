//! Benchmarks for Arvak circuit operations
//!
//! Run with: cargo bench -p arvak-ir

use arvak_ir::{Circuit, ClbitId, QubitId};
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::f64::consts::PI;

/// Benchmark circuit creation
fn bench_circuit_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("circuit_creation");

    for num_qubits in &[2, 5, 10, 20, 50] {
        group.bench_with_input(
            BenchmarkId::new("with_size", num_qubits),
            num_qubits,
            |b, &n| {
                b.iter(|| Circuit::with_size(black_box("bench"), black_box(n), black_box(n)));
            },
        );
    }

    group.finish();
}

/// Benchmark adding gates to a circuit
fn bench_gate_addition(c: &mut Criterion) {
    let mut group = c.benchmark_group("gate_addition");

    // Benchmark single-qubit gates
    group.bench_function("h_gate", |b| {
        let mut circuit = Circuit::with_size("bench", 10, 0);
        b.iter(|| {
            circuit.h(black_box(QubitId(0))).unwrap();
        });
    });

    group.bench_function("rx_gate", |b| {
        let mut circuit = Circuit::with_size("bench", 10, 0);
        b.iter(|| {
            circuit
                .rx(black_box(PI / 4.0), black_box(QubitId(0)))
                .unwrap();
        });
    });

    // Benchmark two-qubit gates
    group.bench_function("cx_gate", |b| {
        let mut circuit = Circuit::with_size("bench", 10, 0);
        b.iter(|| {
            circuit
                .cx(black_box(QubitId(0)), black_box(QubitId(1)))
                .unwrap();
        });
    });

    group.bench_function("cz_gate", |b| {
        let mut circuit = Circuit::with_size("bench", 10, 0);
        b.iter(|| {
            circuit
                .cz(black_box(QubitId(0)), black_box(QubitId(1)))
                .unwrap();
        });
    });

    group.finish();
}

/// Benchmark GHZ state circuit creation
fn bench_ghz_circuit(c: &mut Criterion) {
    let mut group = c.benchmark_group("ghz_circuit");

    for num_qubits in &[3, 5, 10, 20, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("create", num_qubits),
            num_qubits,
            |b, &n| {
                b.iter(|| {
                    let mut circuit = Circuit::with_size("ghz", n, n);
                    circuit.h(QubitId(0)).unwrap();
                    for i in 0..n - 1 {
                        circuit.cx(QubitId(i), QubitId(i + 1)).unwrap();
                    }
                    for i in 0..n {
                        circuit.measure(QubitId(i), ClbitId(i)).unwrap();
                    }
                    black_box(circuit)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark circuit depth calculation
fn bench_circuit_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("circuit_depth");

    for num_qubits in &[5, 10, 20, 50] {
        // Create a circuit with some depth
        let mut circuit = Circuit::with_size("bench", *num_qubits, 0);

        // Add multiple layers
        for _layer in 0..5 {
            for i in 0..*num_qubits {
                circuit.h(QubitId(i)).unwrap();
            }
            for i in (0..*num_qubits - 1).step_by(2) {
                circuit.cx(QubitId(i), QubitId(i + 1)).unwrap();
            }
        }

        group.bench_with_input(
            BenchmarkId::new("depth", num_qubits),
            &circuit,
            |b, circuit| {
                b.iter(|| black_box(circuit.depth()));
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_circuit_creation,
    bench_gate_addition,
    bench_ghz_circuit,
    bench_circuit_depth,
);

criterion_main!(benches);
