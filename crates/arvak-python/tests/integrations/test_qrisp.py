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
def arvak_bell_circuit():
    """Create a simple Bell state circuit in Arvak."""
    return arvak.Circuit.bell()


class TestQrispIntegration:
    """Tests for Qrisp integration."""

    def test_integration_registered(self):
        """Test that Qrisp integration is registered."""
        status = arvak.integration_status()
        assert 'qrisp' in status
        assert status['qrisp']['available'] is True

    def test_get_qrisp_integration(self):
        """Test retrieving Qrisp integration."""
        integration = arvak.get_integration('qrisp')
        assert integration is not None
        assert integration.framework_name == 'qrisp'

    def test_required_packages(self):
        """Test that required packages are declared."""
        integration = arvak.get_integration('qrisp')
        packages = integration.required_packages
        assert len(packages) > 0
        assert any('qrisp' in pkg.lower() for pkg in packages)


class TestQrispToArvak:
    """Tests for Qrisp -> Arvak conversion."""

    def test_qrisp_to_arvak_via_integration(self, qrisp_bell_circuit):
        """Test converting Qrisp circuit to Arvak using integration API."""
        integration = arvak.get_integration('qrisp')
        arvak_circuit = integration.to_arvak(qrisp_bell_circuit)

        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits >= 2

    def test_qrisp_to_arvak_via_qasm(self, qrisp_bell_circuit):
        """Test converting Qrisp circuit to Arvak via QASM."""
        # Export to QASM
        qasm_str = qrisp_bell_circuit.qasm()
        assert qasm_str is not None

        # Import to Arvak
        arvak_circuit = arvak.from_qasm(qasm_str)
        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits >= 2

    def test_qrisp_to_arvak_preserves_qubits(self, qrisp_bell_circuit):
        """Test that qubit count is preserved."""
        integration = arvak.get_integration('qrisp')
        arvak_circuit = integration.to_arvak(qrisp_bell_circuit)

        assert arvak_circuit.num_qubits >= qrisp_bell_circuit.num_qubits()

    def test_qrisp_to_arvak_complex_circuit(self):
        """Test converting a more complex circuit."""
        # GHZ-3 state
        qc = QuantumCircuit(3)
        qc.h(0)
        qc.cx(0, 1)
        qc.cx(1, 2)
        qc.measure_all()

        integration = arvak.get_integration('qrisp')
        arvak_circuit = integration.to_arvak(qc)

        assert arvak_circuit.num_qubits >= 3

    def test_quantum_variable_to_arvak(self, qrisp_quantum_variable):
        """Test converting QuantumVariable to Arvak."""
        integration = arvak.get_integration('qrisp')

        # Get compiled circuit from QuantumVariable
        compiled = qrisp_quantum_variable.qs.compile()

        # Convert to Arvak
        arvak_circuit = integration.to_arvak(compiled)

        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits >= 2

    def test_quantum_session_to_arvak(self, qrisp_quantum_variable):
        """Test converting QuantumSession to Arvak."""
        integration = arvak.get_integration('qrisp')

        # Pass QuantumSession directly
        arvak_circuit = integration.to_arvak(qrisp_quantum_variable.qs)

        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits >= 2


class TestArvakToQrisp:
    """Tests for Arvak -> Qrisp conversion."""

    def test_arvak_to_qrisp_via_integration(self, arvak_bell_circuit):
        """Test converting Arvak circuit to Qrisp using integration API."""
        integration = arvak.get_integration('qrisp')
        qrisp_circuit = integration.from_arvak(arvak_bell_circuit)

        assert qrisp_circuit is not None
        assert isinstance(qrisp_circuit, QuantumCircuit)
        assert qrisp_circuit.num_qubits() >= 2

    def test_arvak_to_qrisp_via_qasm(self, arvak_bell_circuit):
        """Test converting Arvak circuit to Qrisp via QASM."""
        # Export to QASM
        qasm_str = arvak.to_qasm(arvak_bell_circuit)
        assert qasm_str is not None

        # Import to Qrisp
        qrisp_circuit = QuantumCircuit.from_qasm_str(qasm_str)
        assert qrisp_circuit is not None
        assert qrisp_circuit.num_qubits() >= 2

    def test_arvak_to_qrisp_preserves_structure(self):
        """Test that circuit structure is preserved."""
        # Create Arvak GHZ-3
        arvak_circuit = arvak.Circuit.ghz(3)

        integration = arvak.get_integration('qrisp')
        qrisp_circuit = integration.from_arvak(arvak_circuit)

        assert qrisp_circuit.num_qubits() >= 3

    def test_arvak_to_qrisp_qft(self):
        """Test converting QFT circuit."""
        arvak_circuit = arvak.Circuit.qft(4)

        integration = arvak.get_integration('qrisp')
        qrisp_circuit = integration.from_arvak(arvak_circuit)

        assert qrisp_circuit.num_qubits() >= 4


class TestQrispBackendProvider:
    """Tests for Arvak backend provider."""

    def test_get_backend_provider(self):
        """Test retrieving backend provider."""
        integration = arvak.get_integration('qrisp')
        provider = integration.get_backend_provider()

        assert provider is not None

    def test_provider_has_backends(self):
        """Test that provider has available backends."""
        integration = arvak.get_integration('qrisp')
        provider = integration.get_backend_provider()

        backends = provider.backends()
        assert len(backends) > 0

    def test_get_simulator_backend(self):
        """Test getting simulator backend."""
        integration = arvak.get_integration('qrisp')
        provider = integration.get_backend_provider()

        backend = provider.get_backend('sim')
        assert backend is not None
        assert backend.name is not None

    def test_backend_run(self, qrisp_bell_circuit):
        """Test that backend can run circuits."""
        integration = arvak.get_integration('qrisp')
        provider = integration.get_backend_provider()
        backend = provider.get_backend('sim')

        # Run circuit
        results = backend.run(qrisp_bell_circuit, shots=100)

        assert results is not None
        assert isinstance(results, dict)
        assert len(results) > 0

    def test_backend_run_with_quantum_variable(self, qrisp_quantum_variable):
        """Test running QuantumVariable on backend."""
        integration = arvak.get_integration('qrisp')
        provider = integration.get_backend_provider()
        backend = provider.get_backend('sim')

        # Get compiled circuit
        compiled = qrisp_quantum_variable.qs.compile()

        # Run circuit
        results = backend.run(compiled, shots=100)

        assert results is not None
        assert isinstance(results, dict)


class TestQrispRoundTrip:
    """Tests for round-trip conversion (Qrisp -> Arvak -> Qrisp)."""

    def test_roundtrip_preserves_qubits(self, qrisp_bell_circuit):
        """Test that round-trip conversion preserves qubit count."""
        integration = arvak.get_integration('qrisp')

        # Qrisp -> Arvak
        arvak_circuit = integration.to_arvak(qrisp_bell_circuit)

        # Arvak -> Qrisp
        qrisp_circuit_back = integration.from_arvak(arvak_circuit)

        # May have additional qubits due to QASM conversion
        assert qrisp_circuit_back.num_qubits() >= qrisp_bell_circuit.num_qubits()

    def test_roundtrip_ghz(self):
        """Test round-trip with GHZ state."""
        # Create in Qrisp
        qc = QuantumCircuit(3)
        qc.h(0)
        qc.cx(0, 1)
        qc.cx(1, 2)

        integration = arvak.get_integration('qrisp')

        # Round-trip
        arvak_circuit = integration.to_arvak(qc)
        qc_back = integration.from_arvak(arvak_circuit)

        assert qc_back.num_qubits() >= qc.num_qubits()


class TestQrispConverter:
    """Tests for Qrisp converter functions."""

    def test_qrisp_to_arvak_function(self, qrisp_bell_circuit):
        """Test qrisp_to_arvak converter function."""
        from arvak.integrations.qrisp import qrisp_to_arvak

        arvak_circuit = qrisp_to_arvak(qrisp_bell_circuit)
        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits >= 2

    def test_arvak_to_qrisp_function(self, arvak_bell_circuit):
        """Test arvak_to_qrisp converter function."""
        from arvak.integrations.qrisp import arvak_to_qrisp

        qrisp_circuit = arvak_to_qrisp(arvak_bell_circuit)
        assert qrisp_circuit is not None
        assert isinstance(qrisp_circuit, QuantumCircuit)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
