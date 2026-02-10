"""Tests for Qiskit integration.

These tests require Qiskit to be installed. They will be skipped if Qiskit
is not available.
"""

import pytest

# Try to import dependencies
try:
    import arvak
    from qiskit import QuantumCircuit
    from qiskit.qasm3 import dumps, loads
    QISKIT_AVAILABLE = True
except ImportError:
    QISKIT_AVAILABLE = False

# Skip all tests if Qiskit not available
pytestmark = pytest.mark.skipif(
    not QISKIT_AVAILABLE,
    reason="Qiskit not installed"
)


@pytest.fixture
def qiskit_bell_circuit():
    """Create a simple Bell state circuit in Qiskit."""
    qc = QuantumCircuit(2, 2)
    qc.h(0)
    qc.cx(0, 1)
    qc.measure(range(2), range(2))
    return qc


@pytest.fixture
def arvak_bell_circuit():
    """Create a simple Bell state circuit in Arvak."""
    return arvak.Circuit.bell()


class TestQiskitIntegration:
    """Tests for Qiskit integration."""

    def test_integration_registered(self):
        """Test that Qiskit integration is registered."""
        status = arvak.integration_status()
        assert 'qiskit' in status
        assert status['qiskit']['available'] is True

    def test_get_qiskit_integration(self):
        """Test retrieving Qiskit integration."""
        integration = arvak.get_integration('qiskit')
        assert integration is not None
        assert integration.framework_name == 'qiskit'

    def test_required_packages(self):
        """Test that required packages are declared."""
        integration = arvak.get_integration('qiskit')
        packages = integration.required_packages
        assert len(packages) > 0
        assert any('qiskit' in pkg.lower() for pkg in packages)


class TestQiskitToArvak:
    """Tests for Qiskit -> Arvak conversion."""

    def test_qiskit_to_arvak_via_integration(self, qiskit_bell_circuit):
        """Test converting Qiskit circuit to Arvak using integration API."""
        integration = arvak.get_integration('qiskit')
        arvak_circuit = integration.to_arvak(qiskit_bell_circuit)

        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits == 2
        assert arvak_circuit.num_clbits == 2

    def test_qiskit_to_arvak_via_qasm(self, qiskit_bell_circuit):
        """Test converting Qiskit circuit to Arvak via QASM."""
        # Export to QASM
        qasm_str = dumps(qiskit_bell_circuit)
        assert qasm_str is not None

        # Import to Arvak
        arvak_circuit = arvak.from_qasm(qasm_str)
        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits == 2

    def test_qiskit_to_arvak_preserves_qubits(self, qiskit_bell_circuit):
        """Test that qubit count is preserved."""
        integration = arvak.get_integration('qiskit')
        arvak_circuit = integration.to_arvak(qiskit_bell_circuit)

        assert arvak_circuit.num_qubits == qiskit_bell_circuit.num_qubits

    def test_qiskit_to_arvak_complex_circuit(self):
        """Test converting a more complex circuit."""
        # GHZ-3 state
        qc = QuantumCircuit(3, 3)
        qc.h(0)
        qc.cx(0, 1)
        qc.cx(1, 2)
        qc.measure(range(3), range(3))

        integration = arvak.get_integration('qiskit')
        arvak_circuit = integration.to_arvak(qc)

        assert arvak_circuit.num_qubits == 3
        assert arvak_circuit.num_clbits == 3


class TestArvakToQiskit:
    """Tests for Arvak -> Qiskit conversion."""

    def test_arvak_to_qiskit_via_integration(self, arvak_bell_circuit):
        """Test converting Arvak circuit to Qiskit using integration API."""
        integration = arvak.get_integration('qiskit')
        qiskit_circuit = integration.from_arvak(arvak_bell_circuit)

        assert qiskit_circuit is not None
        assert qiskit_circuit.num_qubits == 2

    def test_arvak_to_qiskit_via_qasm(self, arvak_bell_circuit):
        """Test converting Arvak circuit to Qiskit via QASM."""
        # Export to QASM
        qasm_str = arvak.to_qasm(arvak_bell_circuit)
        assert qasm_str is not None

        # Import to Qiskit
        qiskit_circuit = loads(qasm_str)
        assert qiskit_circuit is not None
        assert qiskit_circuit.num_qubits >= 2  # May have additional qubits

    def test_arvak_to_qiskit_preserves_structure(self):
        """Test that circuit structure is preserved."""
        # Create Arvak GHZ-3
        arvak_circuit = arvak.Circuit.ghz(3)

        integration = arvak.get_integration('qiskit')
        qiskit_circuit = integration.from_arvak(arvak_circuit)

        assert qiskit_circuit.num_qubits == 3

    def test_arvak_to_qiskit_qft(self):
        """Test converting QFT circuit."""
        arvak_circuit = arvak.Circuit.qft(4)

        integration = arvak.get_integration('qiskit')
        qiskit_circuit = integration.from_arvak(arvak_circuit)

        assert qiskit_circuit.num_qubits == 4


class TestQiskitBackendProvider:
    """Tests for Arvak backend provider."""

    def test_get_backend_provider(self):
        """Test retrieving backend provider."""
        integration = arvak.get_integration('qiskit')
        provider = integration.get_backend_provider()

        assert provider is not None

    def test_provider_has_backends(self):
        """Test that provider has available backends."""
        integration = arvak.get_integration('qiskit')
        provider = integration.get_backend_provider()

        backends = provider.backends()
        assert len(backends) > 0

    def test_get_simulator_backend(self):
        """Test getting simulator backend."""
        integration = arvak.get_integration('qiskit')
        provider = integration.get_backend_provider()

        backend = provider.get_backend('sim')
        assert backend is not None
        assert backend.name is not None

    def test_backend_properties(self):
        """Test backend has required properties."""
        integration = arvak.get_integration('qiskit')
        provider = integration.get_backend_provider()
        backend = provider.get_backend('sim')

        # Check required properties
        assert hasattr(backend, 'num_qubits')
        assert hasattr(backend, 'basis_gates')
        assert backend.num_qubits > 0
        assert len(backend.basis_gates) > 0

    def test_backend_run_returns_job(self, qiskit_bell_circuit):
        """Test that backend.run() returns a job."""
        integration = arvak.get_integration('qiskit')
        provider = integration.get_backend_provider()
        backend = provider.get_backend('sim')

        job = backend.run(qiskit_bell_circuit, shots=100)
        assert job is not None

    def test_job_has_result(self, qiskit_bell_circuit):
        """Test that job has a result method."""
        integration = arvak.get_integration('qiskit')
        provider = integration.get_backend_provider()
        backend = provider.get_backend('sim')

        job = backend.run(qiskit_bell_circuit, shots=100)
        result = job.result()
        assert result is not None

    def test_result_has_counts(self, qiskit_bell_circuit):
        """Test that result has get_counts method."""
        integration = arvak.get_integration('qiskit')
        provider = integration.get_backend_provider()
        backend = provider.get_backend('sim')

        job = backend.run(qiskit_bell_circuit, shots=100)
        result = job.result()
        counts = result.get_counts()

        assert counts is not None
        assert isinstance(counts, dict)
        assert len(counts) > 0


class TestQiskitSimulatorResults:
    """Tests that Qiskit backend returns correct quantum simulation results."""

    def test_bell_state_only_00_and_11(self, qiskit_bell_circuit):
        """Bell state should only produce 00 and 11 outcomes."""
        integration = arvak.get_integration('qiskit')
        provider = integration.get_backend_provider()
        backend = provider.get_backend('sim')

        job = backend.run(qiskit_bell_circuit, shots=1000)
        result = job.result()
        counts = result.get_counts()

        for bitstring in counts.keys():
            assert bitstring in ('00', '11'), f"Unexpected outcome: {bitstring}"

    def test_bell_state_total_shots(self, qiskit_bell_circuit):
        """Bell state total counts should equal requested shots."""
        integration = arvak.get_integration('qiskit')
        provider = integration.get_backend_provider()
        backend = provider.get_backend('sim')

        job = backend.run(qiskit_bell_circuit, shots=500)
        result = job.result()
        counts = result.get_counts()

        total = sum(counts.values())
        assert total == 500, f"Expected 500 total shots, got {total}"

    def test_ghz3_outcomes(self):
        """GHZ-3 circuit should only produce 000 and 111."""
        qc = QuantumCircuit(3, 3)
        qc.h(0)
        qc.cx(0, 1)
        qc.cx(1, 2)
        qc.measure(range(3), range(3))

        integration = arvak.get_integration('qiskit')
        provider = integration.get_backend_provider()
        backend = provider.get_backend('sim')

        job = backend.run(qc, shots=1000)
        result = job.result()
        counts = result.get_counts()

        for bitstring in counts.keys():
            assert bitstring in ('000', '111'), f"Unexpected outcome: {bitstring}"


class TestQiskitRoundTrip:
    """Tests for round-trip conversion (Qiskit -> Arvak -> Qiskit)."""

    def test_roundtrip_preserves_qubits(self, qiskit_bell_circuit):
        """Test that round-trip conversion preserves qubit count."""
        integration = arvak.get_integration('qiskit')

        # Qiskit -> Arvak
        arvak_circuit = integration.to_arvak(qiskit_bell_circuit)

        # Arvak -> Qiskit
        qiskit_circuit_back = integration.from_arvak(arvak_circuit)

        assert qiskit_circuit_back.num_qubits == qiskit_bell_circuit.num_qubits

    def test_roundtrip_ghz(self):
        """Test round-trip with GHZ state."""
        # Create in Qiskit
        qc = QuantumCircuit(3)
        qc.h(0)
        qc.cx(0, 1)
        qc.cx(1, 2)

        integration = arvak.get_integration('qiskit')

        # Round-trip
        arvak_circuit = integration.to_arvak(qc)
        qc_back = integration.from_arvak(arvak_circuit)

        assert qc_back.num_qubits == qc.num_qubits


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
