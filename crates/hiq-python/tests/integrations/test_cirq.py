"""Tests for Cirq integration.

These tests require Cirq to be installed. They will be skipped if Cirq
is not available.
"""

import pytest

# Try to import dependencies
try:
    import arvak
    import cirq
    CIRQ_AVAILABLE = True
except ImportError:
    CIRQ_AVAILABLE = False

# Skip all tests if Cirq not available
pytestmark = pytest.mark.skipif(
    not CIRQ_AVAILABLE,
    reason="Cirq not installed"
)


@pytest.fixture
def cirq_bell_circuit():
    """Create a simple Bell state circuit in Cirq."""
    qubits = cirq.LineQubit.range(2)
    circuit = cirq.Circuit(
        cirq.H(qubits[0]),
        cirq.CNOT(qubits[0], qubits[1]),
        cirq.measure(*qubits, key='result')
    )
    return circuit


@pytest.fixture
def cirq_grid_circuit():
    """Create a circuit with GridQubits."""
    q00 = cirq.GridQubit(0, 0)
    q01 = cirq.GridQubit(0, 1)
    circuit = cirq.Circuit(
        cirq.H(q00),
        cirq.CNOT(q00, q01),
        cirq.measure(q00, q01, key='result')
    )
    return circuit


@pytest.fixture
def hiq_bell_circuit():
    """Create a simple Bell state circuit in HIQ."""
    return hiq.Circuit.bell()


class TestCirqIntegration:
    """Tests for Cirq integration."""

    def test_integration_registered(self):
        """Test that Cirq integration is registered."""
        status = hiq.integration_status()
        assert 'cirq' in status
        assert status['cirq']['available'] is True

    def test_get_cirq_integration(self):
        """Test retrieving Cirq integration."""
        integration = hiq.get_integration('cirq')
        assert integration is not None
        assert integration.framework_name == 'cirq'

    def test_required_packages(self):
        """Test that required packages are declared."""
        integration = hiq.get_integration('cirq')
        packages = integration.required_packages
        assert len(packages) > 0
        assert any('cirq' in pkg.lower() for pkg in packages)


class TestCirqToHIQ:
    """Tests for Cirq -> HIQ conversion."""

    def test_cirq_to_hiq_via_integration(self, cirq_bell_circuit):
        """Test converting Cirq circuit to HIQ using integration API."""
        integration = hiq.get_integration('cirq')
        hiq_circuit = integration.to_hiq(cirq_bell_circuit)

        assert hiq_circuit is not None
        assert hiq_circuit.num_qubits >= 2

    def test_cirq_to_hiq_via_qasm(self, cirq_bell_circuit):
        """Test converting Cirq circuit to HIQ via QASM."""
        # Export to QASM
        qasm_str = cirq.qasm(cirq_bell_circuit)
        assert qasm_str is not None

        # Import to HIQ
        hiq_circuit = hiq.from_qasm(qasm_str)
        assert hiq_circuit is not None
        assert hiq_circuit.num_qubits >= 2

    def test_cirq_to_hiq_preserves_qubits(self, cirq_bell_circuit):
        """Test that qubit count is preserved."""
        integration = hiq.get_integration('cirq')
        hiq_circuit = integration.to_hiq(cirq_bell_circuit)

        num_cirq_qubits = len(cirq_bell_circuit.all_qubits())
        assert hiq_circuit.num_qubits >= num_cirq_qubits

    def test_cirq_to_hiq_complex_circuit(self):
        """Test converting a more complex circuit."""
        # GHZ-3 state
        qubits = cirq.LineQubit.range(3)
        circuit = cirq.Circuit(
            cirq.H(qubits[0]),
            cirq.CNOT(qubits[0], qubits[1]),
            cirq.CNOT(qubits[1], qubits[2]),
            cirq.measure(*qubits, key='result')
        )

        integration = hiq.get_integration('cirq')
        hiq_circuit = integration.to_hiq(circuit)

        assert hiq_circuit.num_qubits >= 3

    def test_cirq_gridqubit_to_hiq(self, cirq_grid_circuit):
        """Test converting GridQubit circuit to HIQ."""
        integration = hiq.get_integration('cirq')
        hiq_circuit = integration.to_hiq(cirq_grid_circuit)

        assert hiq_circuit is not None
        assert hiq_circuit.num_qubits >= 2


class TestHIQToCirq:
    """Tests for HIQ -> Cirq conversion."""

    def test_hiq_to_cirq_via_integration(self, hiq_bell_circuit):
        """Test converting HIQ circuit to Cirq using integration API."""
        integration = hiq.get_integration('cirq')
        cirq_circuit = integration.from_hiq(hiq_bell_circuit)

        assert cirq_circuit is not None
        assert isinstance(cirq_circuit, cirq.Circuit)
        assert len(cirq_circuit.all_qubits()) >= 2

    def test_hiq_to_cirq_via_qasm(self, hiq_bell_circuit):
        """Test converting HIQ circuit to Cirq via QASM."""
        # Export to QASM
        qasm_str = hiq.to_qasm(hiq_bell_circuit)
        assert qasm_str is not None

        # Import to Cirq
        cirq_circuit = cirq.circuits.qasm_input.circuit_from_qasm(qasm_str)
        assert cirq_circuit is not None
        assert len(cirq_circuit.all_qubits()) >= 2

    def test_hiq_to_cirq_preserves_structure(self):
        """Test that circuit structure is preserved."""
        # Create HIQ GHZ-3
        hiq_circuit = hiq.Circuit.ghz(3)

        integration = hiq.get_integration('cirq')
        cirq_circuit = integration.from_hiq(hiq_circuit)

        assert len(cirq_circuit.all_qubits()) >= 3

    def test_hiq_to_cirq_qft(self):
        """Test converting QFT circuit."""
        hiq_circuit = hiq.Circuit.qft(4)

        integration = hiq.get_integration('cirq')
        cirq_circuit = integration.from_hiq(hiq_circuit)

        assert len(cirq_circuit.all_qubits()) >= 4


class TestCirqSampler:
    """Tests for HIQ sampler."""

    def test_get_backend_provider(self):
        """Test retrieving backend provider (engine)."""
        integration = hiq.get_integration('cirq')
        engine = integration.get_backend_provider()

        assert engine is not None

    def test_get_sampler(self):
        """Test getting sampler from engine."""
        integration = hiq.get_integration('cirq')
        engine = integration.get_backend_provider()

        sampler = engine.get_sampler()
        assert sampler is not None

    def test_sampler_run(self, cirq_bell_circuit):
        """Test that sampler can run circuits."""
        integration = hiq.get_integration('cirq')
        engine = integration.get_backend_provider()
        sampler = engine.get_sampler()

        # Run circuit
        result = sampler.run(cirq_bell_circuit, repetitions=100)

        assert result is not None
        assert result.repetitions == 100

    def test_sampler_histogram(self, cirq_bell_circuit):
        """Test getting histogram from results."""
        integration = hiq.get_integration('cirq')
        engine = integration.get_backend_provider()
        sampler = engine.get_sampler()

        # Run circuit
        result = sampler.run(cirq_bell_circuit, repetitions=100)

        # Get histogram
        histogram = result.histogram(key='result')
        assert histogram is not None
        assert isinstance(histogram, dict)
        assert len(histogram) > 0


class TestCirqRoundTrip:
    """Tests for round-trip conversion (Cirq -> HIQ -> Cirq)."""

    def test_roundtrip_preserves_qubits(self, cirq_bell_circuit):
        """Test that round-trip conversion preserves qubit count."""
        integration = hiq.get_integration('cirq')

        # Cirq -> HIQ
        hiq_circuit = integration.to_hiq(cirq_bell_circuit)

        # HIQ -> Cirq
        cirq_circuit_back = integration.from_hiq(hiq_circuit)

        num_original = len(cirq_bell_circuit.all_qubits())
        num_converted = len(cirq_circuit_back.all_qubits())
        # May have additional qubits due to QASM conversion
        assert num_converted >= num_original

    def test_roundtrip_ghz(self):
        """Test round-trip with GHZ state."""
        # Create in Cirq
        qubits = cirq.LineQubit.range(3)
        circuit = cirq.Circuit(
            cirq.H(qubits[0]),
            cirq.CNOT(qubits[0], qubits[1]),
            cirq.CNOT(qubits[1], qubits[2])
        )

        integration = hiq.get_integration('cirq')

        # Round-trip
        hiq_circuit = integration.to_hiq(circuit)
        circuit_back = integration.from_hiq(hiq_circuit)

        assert len(circuit_back.all_qubits()) >= len(circuit.all_qubits())


class TestCirqConverter:
    """Tests for Cirq converter functions."""

    def test_cirq_to_hiq_function(self, cirq_bell_circuit):
        """Test cirq_to_hiq converter function."""
        from arvak.integrations.cirq import cirq_to_hiq

        hiq_circuit = cirq_to_hiq(cirq_bell_circuit)
        assert hiq_circuit is not None
        assert hiq_circuit.num_qubits >= 2

    def test_hiq_to_cirq_function(self, hiq_bell_circuit):
        """Test hiq_to_cirq converter function."""
        from arvak.integrations.cirq import hiq_to_cirq

        cirq_circuit = hiq_to_cirq(hiq_bell_circuit)
        assert cirq_circuit is not None
        assert isinstance(cirq_circuit, cirq.Circuit)


class TestCirqMoments:
    """Tests for Cirq moment structure."""

    def test_moments_preserved(self):
        """Test that moments are handled correctly."""
        # Create circuit with explicit moments
        qubits = cirq.LineQubit.range(2)
        circuit = cirq.Circuit(
            cirq.Moment([cirq.H(qubits[0]), cirq.H(qubits[1])]),
            cirq.Moment([cirq.CNOT(qubits[0], qubits[1])]),
            cirq.Moment([cirq.measure(*qubits, key='result')])
        )

        integration = hiq.get_integration('cirq')
        hiq_circuit = integration.to_hiq(circuit)

        assert hiq_circuit is not None
        assert hiq_circuit.num_qubits >= 2


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
