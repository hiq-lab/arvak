# Python API Reference

The `arvak` package is a thin PyO3 layer over the Rust core. This page
covers the stable, user-facing surface. Docstrings (`help(arvak.compile)`)
carry the full parameter details.

> Every Python snippet on this page is executed by CI
> (`crates/arvak-python/tests/test_doc_snippets.py`).

## Circuits

`Circuit(name, num_qubits=0)` builds circuits with a chainable gate API.
Gate methods take parameters first, qubit indices last.

```python
import arvak

qc = arvak.Circuit("demo", num_qubits=3)
qc.h(0).cx(0, 1)                 # single- and two-qubit gates chain
qc.rz(0.5, 2)                    # parameters first: rz(theta, qubit)
qc.u(0.1, 0.2, 0.3, 0)           # u(theta, phi, lam, qubit)
qc.prx(0.5, 0.25, 1)             # IQM native phased-RX: prx(theta, phi, qubit)
qc.ccx(0, 1, 2)
qc.measure_all()

assert qc.num_qubits == 3        # property
assert qc.depth() > 0            # method
```

Available gates: `x y z h s sdg t tdg sx p rx ry rz u prx` (single-qubit),
`cx cy cz ch cp crz swap iswap` (two-qubit), `ccx cswap` (three-qubit),
plus `barrier_all`, `reset`, `delay`, `measure(qubit, clbit)`,
`measure_all`. Classical bits are added implicitly by `measure_all` or
explicitly via `add_clbit()`.

Prebuilt circuits: `Circuit.bell()`, `Circuit.ghz(n)`, `Circuit.qft(n)`.

## Simulation

`run_sim(circuit, shots)` runs the built-in statevector simulator
(≤ 20 qubits, no network) and returns a `{bitstring: count}` dict:

```python
import arvak

counts = arvak.run_sim(arvak.Circuit.bell(), shots=1000)
assert set(counts) == {"00", "11"}
```

## Compilation

`compile(circuit, coupling_map=None, basis_gates=None, optimization_level=1)`
runs the full pipeline: layout, SWAP routing, basis translation, gate
optimization. Gates outside the target basis are decomposed automatically.

| Level | Layout | Routing |
|-------|--------------|---------------------------|
| 0 | trivial | greedy shortest-path SWAPs |
| 1 | trivial | SABRE (default) |
| 2–3 | dense region | SABRE + more optimization |

```python
import arvak

qc = arvak.Circuit("toffoli", num_qubits=3)
qc.ccx(0, 1, 2)

compiled = arvak.compile(
    qc,
    coupling_map=arvak.CouplingMap.linear(5),
    basis_gates=arvak.BasisGates.iqm(),
    optimization_level=2,
)
# After routing the circuit spans the whole device.
assert compiled.num_qubits == 5
```

**`CouplingMap`** — device connectivity. Constructors: `linear(n)`,
`star(n)`, `full(n)`, `from_edge_list(n, edges)`, or `CouplingMap(n)` plus
`add_edge(a, b)` calls.

**`BasisGates`** — target native gate sets: `ibm()` (rz/sx/x/cx),
`iqm()` (prx/cz), `heron()`, `universal()`, or any custom set via the
constructor. `gates()` and `contains(name)` inspect a set.

## Backends

One entry point for all eleven vendors — the HAL contract makes them
interchangeable:

```python
import arvak

print(arvak.list_backends())     # names + availability
```

```python
# doc-test: skip  (hardware backends need vendor credentials)
import arvak

backend = arvak.backend_for("iqm_garnet")
result = backend.run(qasm_string, shots=1024)
print(result.counts)
```

`backend.run()` takes an OpenQASM 3 string. Vendor adapters handle
authentication, submission, polling, and result decoding.

## QASM 3 I/O

`to_qasm(circuit)` emits standalone-valid OpenQASM 3 (includes
`stdgates.inc`, defines non-standard gates like `prx` inline, uses
`dt`-unit delays). `from_qasm(source)` parses it back — including
Qiskit's QASM 3 exports.

```python
import arvak

qasm = arvak.to_qasm(arvak.Circuit.bell())
assert 'include "stdgates.inc";' in qasm

roundtrip = arvak.from_qasm(qasm)
assert roundtrip.num_qubits == 2
```

## Framework integrations

Installed extras register automatically:

```python
import arvak

print(arvak.list_integrations())   # e.g. {'qiskit': True}
```

With `pip install arvak[qiskit]`, Arvak acts as a Qiskit provider, and
circuits convert in both directions:

```python
# doc-test: skip  (requires the qiskit extra)
from qiskit import QuantumCircuit
from arvak.integrations.qiskit import ArvakProvider

qc = QuantumCircuit(2, 2)
qc.h(0); qc.cx(0, 1); qc.measure_all()

backend = ArvakProvider().get_backend("sim")
print(backend.run(qc, shots=1000).result().get_counts())

# Direct conversion without the provider:
import arvak
arvak_qc = arvak.get_integration("qiskit").to_arvak(qc)
```

Qrisp, Cirq, and PennyLane integrations follow the same pattern; see
[INTEGRATION_GUIDE.md](INTEGRATION_GUIDE.md).

## Optional modules

- `arvak.nathan` — literature-grounded circuit analysis
  (`pip install arvak[nathan]`)
- `arvak.optimize` — VQE/QAOA workflows (see module docstrings)
