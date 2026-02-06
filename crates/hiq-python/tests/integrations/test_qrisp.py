"""Tests for Qrisp integration.

These tests require Qrisp to be installed. They will be skipped if Qrisp
is not available.
"""

import pytest

# Try to import dependencies
try:
    import arvak
    from qrisp import QuantumCircuit, QuantumVariable, QuantumSession
    QRISP_AVAILABLE = True
except ImportError:
    QRISP_AVAILABLE = False

# Skip all tests if Qrisp not available
pytestmark = pytest.mark.skipif(
    not QRISP_AVAILABLE,
    reason="Qrisp not installed"
)


@pytest.fixture
def qrisp_bell_circuit():
    """Create a simple Bell state circuit in Qrisp."""
    qc = QuantumCircuit(2)
    qc.h(0)
    qc.cx(0, 1)
    qc.measure_all()
    return qc


@pytest.fixture
def qrisp_quantum_variable():
    """Create a QuantumVariable with simple operations."""
    from qrisp import QuantumVariable, h
    qv = QuantumVariable(2)
    h(qv[0])
    qv.cx(0, 1)
    return qv


@pytest.fixture
def hiq_bell_circuit():
    """Create a simple Bell state circuit in HIQ."""
    return hiq.Circuit.bell()


class TestQrispIntegration:
    """Tests for Qrisp integration."""

    def test_integration_registered(self):
        """Test that Qrisp integration is registered."""
        status = hiq.integration_status()
        assert 'qrisp' in status
        assert status['qrisp']['available'] is True

    def test_get_qrisp_integration(self):
        """Test retrieving Qrisp integration."""
        integration = hiq.get_integration('qrisp')
        assert integration is not None
        assert integration.framework_name == 'qrisp'

    def test_required_packages(self):
        """Test that required packages are declared."""
        integration = hiq.get_integration('qrisp')
        packages = integration.required_packages
        assert len(packages) > 0
        assert any('qrisp' in pkg.lower() for pkg in packages)


class TestQrispToHIQ:
    """Tests for Qrisp -> HIQ conversion."""

    def test_qrisp_to_hiq_via_integration(self, qrisp_bell_circuit):
        """Test converting Qrisp circuit to HIQ using integration API."""
        integration = hiq.get_integration('qrisp')
        hiq_circuit = integration.to_hiq(qrisp_bell_circuit)

        assert hiq_circuit is not None
        assert hiq_circuit.num_qubits >= 2

    def test_qrisp_to_hiq_via_qasm(self, qrisp_bell_circuit):
        """Test converting Qrisp circuit to HIQ via QASM."""
        # Export to QASM
        qasm_str = qrisp_bell_circuit.qasm()
        assert qasm_str is not None

        # Import to HIQ
        hiq_circuit = hiq.from_qasm(qasm_str)
        assert hiq_circuit is not None
        assert hiq_circuit.num_qubits >= 2

    def test_qrisp_to_hiq_preserves_qubits(self, qrisp_bell_circuit):
        """Test that qubit count is preserved."""
        integration = hiq.get_integration('qrisp')
        hiq_circuit = integration.to_hiq(qrisp_bell_circuit)

        assert hiq_circuit.num_qubits >= qrisp_bell_circuit.num_qubits()

    def test_qrisp_to_hiq_complex_circuit(self):
        """Test converting a more complex circuit."""
        # GHZ-3 state
        qc = QuantumCircuit(3)
        qc.h(0)
        qc.cx(0, 1)
        qc.cx(1, 2)
        qc.measure_all()

        integration = hiq.get_integration('qrisp')
        hiq_circuit = integration.to_hiq(qc)

        assert hiq_circuit.num_qubits >= 3

    def test_quantum_variable_to_hiq(self, qrisp_quantum_variable):
        """Test converting QuantumVariable to HIQ."""
        integration = hiq.get_integration('qrisp')

        # Get compiled circuit from QuantumVariable
        compiled = qrisp_quantum_variable.qs.compile()

        # Convert to HIQ
        hiq_circuit = integration.to_hiq(compiled)

        assert hiq_circuit is not None
        assert hiq_circuit.num_qubits >= 2

    def test_quantum_session_to_hiq(self, qrisp_quantum_variable):
        """Test converting QuantumSession to HIQ."""
        integration = hiq.get_integration('qrisp')

        # Pass QuantumSession directly
        hiq_circuit = integration.to_hiq(qrisp_quantum_variable.qs)

        assert hiq_circuit is not None
        assert hiq_circuit.num_qubits >= 2


class TestHIQToQrisp:
    """Tests for HIQ -> Qrisp conversion."""

    def test_hiq_to_qrisp_via_integration(self, hiq_bell_circuit):
        """Test converting HIQ circuit to Qrisp using integration API."""
        integration = hiq.get_integration('qrisp')
        qrisp_circuit = integration.from_hiq(hiq_bell_circuit)

        assert qrisp_circuit is not None
        assert isinstance(qrisp_circuit, QuantumCircuit)
        assert qrisp_circuit.num_qubits() >= 2

    def test_hiq_to_qrisp_via_qasm(self, hiq_bell_circuit):
        """Test converting HIQ circuit to Qrisp via QASM."""
        # Export to QASM
        qasm_str = hiq.to_qasm(hiq_bell_circuit)
        assert qasm_str is not None

        # Import to Qrisp
        qrisp_circuit = QuantumCircuit.from_qasm_str(qasm_str)
        assert qrisp_circuit is not None
        assert qrisp_circuit.num_qubits() >= 2

    def test_hiq_to_qrisp_preserves_structure(self):
        """Test that circuit structure is preserved."""
        # Create HIQ GHZ-3
        hiq_circuit = hiq.Circuit.ghz(3)

        integration = hiq.get_integration('qrisp')
        qrisp_circuit = integration.from_hiq(hiq_circuit)

        assert qrisp_circuit.num_qubits() >= 3

    def test_hiq_to_qrisp_qft(self):
        """Test converting QFT circuit."""
        hiq_circuit = hiq.Circuit.qft(4)

        integration = hiq.get_integration('qrisp')
        qrisp_circuit = integration.from_hiq(hiq_circuit)

        assert qrisp_circuit.num_qubits() >= 4


class TestQrispBackendProvider:
    """Tests for HIQ backend provider."""

    def test_get_backend_provider(self):
        """Test retrieving backend provider."""
        integration = hiq.get_integration('qrisp')
        provider = integration.get_backend_provider()

        assert provider is not None

    def test_provider_has_backends(self):
        """Test that provider has available backends."""
        integration = hiq.get_integration('qrisp')
        provider = integration.get_backend_provider()

        backends = provider.backends()
        assert len(backends) > 0

    def test_get_simulator_backend(self):
        """Test getting simulator backend."""
        integration = hiq.get_integration('qrisp')
        provider = integration.get_backend_provider()

        backend = provider.get_backend('sim')
        assert backend is not None
        assert backend.name is not None

    def test_backend_run(self, qrisp_bell_circuit):
        """Test that backend can run circuits."""
        integration = hiq.get_integration('qrisp')
        provider = integration.get_backend_provider()
        backend = provider.get_backend('sim')

        # Run circuit
        results = backend.run(qrisp_bell_circuit, shots=100)

        assert results is not None
        assert isinstance(results, dict)
        assert len(results) > 0

    def test_backend_run_with_quantum_variable(self, qrisp_quantum_variable):
        """Test running QuantumVariable on backend."""
        integration = hiq.get_integration('qrisp')
        provider = integration.get_backend_provider()
        backend = provider.get_backend('sim')

        # Get compiled circuit
        compiled = qrisp_quantum_variable.qs.compile()

        # Run circuit
        results = backend.run(compiled, shots=100)

        assert results is not None
        assert isinstance(results, dict)


class TestQrispRoundTrip:
    """Tests for round-trip conversion (Qrisp -> HIQ -> Qrisp)."""

    def test_roundtrip_preserves_qubits(self, qrisp_bell_circuit):
        """Test that round-trip conversion preserves qubit count."""
        integration = hiq.get_integration('qrisp')

        # Qrisp -> HIQ
        hiq_circuit = integration.to_hiq(qrisp_bell_circuit)

        # HIQ -> Qrisp
        qrisp_circuit_back = integration.from_hiq(hiq_circuit)

        # May have additional qubits due to QASM conversion
        assert qrisp_circuit_back.num_qubits() >= qrisp_bell_circuit.num_qubits()

    def test_roundtrip_ghz(self):
        """Test round-trip with GHZ state."""
        # Create in Qrisp
        qc = QuantumCircuit(3)
        qc.h(0)
        qc.cx(0, 1)
        qc.cx(1, 2)

        integration = hiq.get_integration('qrisp')

        # Round-trip
        hiq_circuit = integration.to_hiq(qc)
        qc_back = integration.from_hiq(hiq_circuit)

        assert qc_back.num_qubits() >= qc.num_qubits()


class TestQrispConverter:
    """Tests for Qrisp converter functions."""

    def test_qrisp_to_hiq_function(self, qrisp_bell_circuit):
        """Test qrisp_to_hiq converter function."""
        from arvak.integrations.qrisp import qrisp_to_hiq

        hiq_circuit = qrisp_to_hiq(qrisp_bell_circuit)
        assert hiq_circuit is not None
        assert hiq_circuit.num_qubits >= 2

    def test_hiq_to_qrisp_function(self, hiq_bell_circuit):
        """Test hiq_to_qrisp converter function."""
        from arvak.integrations.qrisp import hiq_to_qrisp

        qrisp_circuit = hiq_to_qrisp(hiq_bell_circuit)
        assert qrisp_circuit is not None
        assert isinstance(qrisp_circuit, QuantumCircuit)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
