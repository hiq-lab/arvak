"""Tests for arvak.optimize.HalBackend.

Real-hardware backend tests are skipped unless credentials are available.
The simulator path and all helper functions run fully offline.
"""

from __future__ import annotations

import pytest

import arvak
from arvak.optimize import BinaryQubo, HalBackend, PCESolver, PceResult
from arvak.optimize._backend import _arvak_to_qiskit, _normalize_counts


# ===========================================================================
# _normalize_counts
# ===========================================================================

class TestNormalizeCounts:

    def test_plain_binary_passthrough(self):
        counts = {"00": 50, "11": 50}
        result = _normalize_counts(counts)
        assert result == {"00": 50, "11": 50}

    def test_strips_register_spaces(self):
        counts = {"01 10": 30, "00 00": 70}
        result = _normalize_counts(counts)
        assert "0110" in result
        assert "0000" in result
        assert result["0110"] == 30

    def test_zero_pads_short_keys(self):
        counts = {"0": 40, "1": 60}
        result = _normalize_counts(counts, n_bits=3)
        assert "000" in result
        assert "001" in result

    def test_merges_collisions_after_normalisation(self):
        # "0 1" and "01" should merge to the same key
        counts = {"0 1": 30, "01": 20}
        result = _normalize_counts(counts)
        assert result.get("01", 0) == 50

    def test_preserves_total_counts(self):
        counts = {"00": 512, "11": 512}
        result = _normalize_counts(counts, n_bits=2)
        assert sum(result.values()) == 1024

    def test_empty_input(self):
        assert _normalize_counts({}) == {}

    def test_n_bits_zero_no_padding(self):
        counts = {"1": 100}
        result = _normalize_counts(counts, n_bits=0)
        assert result == {"1": 100}


# ===========================================================================
# _arvak_to_qiskit  (requires qiskit)
# ===========================================================================

class TestArvakToQiskit:

    def test_converts_bell_circuit(self):
        qiskit = pytest.importorskip("qiskit")
        circuit = arvak.Circuit.bell()
        qc = _arvak_to_qiskit(circuit)
        assert qc.num_qubits == 2

    def test_converts_3qubit_circuit(self):
        qiskit = pytest.importorskip("qiskit")
        c = arvak.Circuit("test", num_qubits=3)
        c.h(0).cx(0, 1).cx(1, 2)
        qc = _arvak_to_qiskit(c)
        assert qc.num_qubits == 3

    def test_preserves_qubit_count(self):
        qiskit = pytest.importorskip("qiskit")
        for n in [1, 2, 4, 8]:
            from arvak.optimize._pce import _build_ansatz
            import numpy as np
            theta = np.zeros(n * 2)
            circuit = _build_ansatz(n, 2, theta)
            qc = _arvak_to_qiskit(circuit)
            assert qc.num_qubits == n


# ===========================================================================
# HalBackend — import and repr
# ===========================================================================

class TestHalBackendImport:

    def test_import_from_optimize(self):
        from arvak.optimize import HalBackend
        assert HalBackend is not None

    def test_requires_qiskit_on_construction(self):
        """HalBackend construction succeeds when qiskit IS installed."""
        pytest.importorskip("qiskit")
        backend = HalBackend.simulator()
        assert backend is not None

    def test_repr(self):
        pytest.importorskip("qiskit")
        backend = HalBackend.simulator()
        assert "HalBackend" in repr(backend)
        assert "timeout" in repr(backend)


# ===========================================================================
# HalBackend.simulator() — full end-to-end, no credentials needed
# ===========================================================================

class TestHalBackendSimulator:

    @pytest.fixture
    def sim_backend(self):
        pytest.importorskip("qiskit")
        return HalBackend.simulator()

    def test_callable_returns_counts_dict(self, sim_backend):
        circuit = arvak.Circuit.bell()
        counts = sim_backend(circuit, shots=200)
        assert isinstance(counts, dict)
        assert sum(counts.values()) == 200

    def test_bitstrings_are_binary(self, sim_backend):
        circuit = arvak.Circuit.bell()
        counts = sim_backend(circuit, shots=100)
        for key in counts:
            assert all(c in "01" for c in key), f"non-binary key: {key!r}"

    def test_bitstring_length_matches_qubits(self, sim_backend):
        from arvak.optimize._pce import _build_ansatz
        import numpy as np
        theta = np.zeros(6)          # 2 layers × 3 qubits
        circuit = _build_ansatz(3, 2, theta)
        counts = sim_backend(circuit, shots=100)
        for key in counts:
            assert len(key) == 3, f"expected 3-bit key, got {key!r}"

    def test_pce_solver_with_hal_sim_backend(self, sim_backend):
        """End-to-end: PCESolver with HalBackend.simulator()."""
        import numpy as np
        Q = np.diag([-1.0, -1.0, -1.0])
        qubo = BinaryQubo.from_matrix(Q)
        solver = PCESolver(qubo, backend=sim_backend, shots=256, max_iter=30, seed=0)
        result = solver.solve()
        assert isinstance(result, PceResult)
        assert len(result.solution) == 3
        assert result.cost <= 0.0

    def test_sim_backend_total_shots(self, sim_backend):
        from arvak.optimize._pce import _build_ansatz
        import numpy as np
        theta = np.random.default_rng(0).uniform(0, 6.28, 4)
        circuit = _build_ansatz(2, 2, theta)
        for shots in [64, 128, 512]:
            counts = sim_backend(circuit, shots=shots)
            assert sum(counts.values()) == shots


# ===========================================================================
# HalBackend — real hardware (skipped without credentials)
# ===========================================================================

@pytest.mark.skipif(
    __import__("os").environ.get("IBM_API_KEY") is None,
    reason="IBM_API_KEY not set — skipping real hardware test",
)
class TestHalBackendIBM:

    def test_ibm_torino_availability(self):
        backend = HalBackend.ibm("ibm_torino", check_availability=False)
        assert backend is not None

    def test_ibm_run_bell(self):
        backend = HalBackend.ibm("ibm_torino")
        circuit = arvak.Circuit.bell()
        counts = backend(circuit, shots=128)
        assert sum(counts.values()) == 128
        assert all(k in ("00", "11") for k in counts)


@pytest.mark.skipif(
    __import__("os").environ.get("AQT_TOKEN") is None,
    reason="AQT_TOKEN not set — skipping AQT test",
)
class TestHalBackendAQT:

    def test_aqt_offline_sim_bell(self):
        backend = HalBackend.aqt("offline_simulator_no_noise")
        circuit = arvak.Circuit.bell()
        counts = backend(circuit, shots=100)
        assert sum(counts.values()) == 100


@pytest.mark.skipif(
    __import__("os").environ.get("QUANTINUUM_EMAIL") is None,
    reason="QUANTINUUM_EMAIL not set — skipping Quantinuum test",
)
class TestHalBackendQuantinuum:

    def test_quantinuum_h2le_bell(self):
        backend = HalBackend.quantinuum("H2-1LE")
        circuit = arvak.Circuit.bell()
        counts = backend(circuit, shots=100)
        assert sum(counts.values()) == 100
