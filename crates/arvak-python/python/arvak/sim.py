"""arvak.sim — Hamiltonian time-evolution circuit synthesis.

Exposes Trotter-Suzuki and QDrift product-formula synthesisers backed by
the Rust ``arvak-sim`` crate. All methods return ``arvak.Circuit`` objects
that can be passed directly to any backend or compilation pass.

Example::

    from arvak.sim import Hamiltonian, HamiltonianTerm, TrotterEvolution

    # Transverse-field Ising: H = -ZZ - 0.5*X0 - 0.5*X1
    h = Hamiltonian.from_terms([
        HamiltonianTerm.zz(0, 1, -1.0),
        HamiltonianTerm.x(0, -0.5),
        HamiltonianTerm.x(1, -0.5),
    ])
    circuit = TrotterEvolution(h, t=1.0, n_steps=4).first_order()
    print(circuit)  # Circuit('trotter1', num_qubits=2, ...)
"""

import arvak._native as _native  # noqa: E402

_sim = _native.sim

PauliOp = _sim.PauliOp
PauliString = _sim.PauliString
HamiltonianTerm = _sim.HamiltonianTerm
Hamiltonian = _sim.Hamiltonian
TrotterEvolution = _sim.TrotterEvolution
QDriftEvolution = _sim.QDriftEvolution

__all__ = [
    "PauliOp",
    "PauliString",
    "HamiltonianTerm",
    "Hamiltonian",
    "TrotterEvolution",
    "QDriftEvolution",
]
