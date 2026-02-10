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
def arvak_bell_circuit():
    """Create a simple Bell state circuit in Arvak."""
    return arvak.Circuit.bell()


class TestCirqIntegration:
    """Tests for Cirq integration."""

    def test_integration_registered(self):
        """Test that Cirq integration is registered."""
        status = arvak.integration_status()
        assert 'cirq' in status
        assert status['cirq']['available'] is True

    def test_get_cirq_integration(self):
        """Test retrieving Cirq integration."""
        integration = arvak.get_integration('cirq')
        assert integration is not None
        assert integration.framework_name == 'cirq'

    def test_required_packages(self):
        """Test that required packages are declared."""
        integration = arvak.get_integration('cirq')
        packages = integration.required_packages
        assert len(packages) > 0
        assert any('cirq' in pkg.lower() for pkg in packages)


class TestCirqToArvak:
    """Tests for Cirq -> Arvak conversion."""

    def test_cirq_to_arvak_via_integration(self, cirq_bell_circuit):
        """Test converting Cirq circuit to Arvak using integration API."""
        integration = arvak.get_integration('cirq')
        arvak_circuit = integration.to_arvak(cirq_bell_circuit)

        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits >= 2

    def test_cirq_to_arvak_via_qasm(self, cirq_bell_circuit):
        """Test converting Cirq circuit to Arvak via QASM."""
        # Export to QASM
        qasm_str = cirq.qasm(cirq_bell_circuit)
        assert qasm_str is not None

        # Import to Arvak
        arvak_circuit = arvak.from_qasm(qasm_str)
        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits >= 2

    def test_cirq_to_arvak_preserves_qubits(self, cirq_bell_circuit):
        """Test that qubit count is preserved."""
        integration = arvak.get_integration('cirq')
        arvak_circuit = integration.to_arvak(cirq_bell_circuit)

        num_cirq_qubits = len(cirq_bell_circuit.all_qubits())
        assert arvak_circuit.num_qubits >= num_cirq_qubits

    def test_cirq_to_arvak_complex_circuit(self):
        """Test converting a more complex circuit."""
        # GHZ-3 state
        qubits = cirq.LineQubit.range(3)
        circuit = cirq.Circuit(
            cirq.H(qubits[0]),
            cirq.CNOT(qubits[0], qubits[1]),
            cirq.CNOT(qubits[1], qubits[2]),
            cirq.measure(*qubits, key='result')
        )

        integration = arvak.get_integration('cirq')
        arvak_circuit = integration.to_arvak(circuit)

        assert arvak_circuit.num_qubits >= 3

    def test_cirq_gridqubit_to_arvak(self, cirq_grid_circuit):
        """Test converting GridQubit circuit to Arvak."""
        integration = arvak.get_integration('cirq')
        arvak_circuit = integration.to_arvak(cirq_grid_circuit)

        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits >= 2


class TestArvakToCirq:
    """Tests for Arvak -> Cirq conversion."""

    def test_arvak_to_cirq_via_integration(self, arvak_bell_circuit):
        """Test converting Arvak circuit to Cirq using integration API."""
        integration = arvak.get_integration('cirq')
        cirq_circuit = integration.from_arvak(arvak_bell_circuit)

        assert cirq_circuit is not None
        assert isinstance(cirq_circuit, cirq.Circuit)
        assert len(cirq_circuit.all_qubits()) >= 2

    def test_arvak_to_cirq_via_qasm(self, arvak_bell_circuit):
        """Test converting Arvak circuit to Cirq via QASM."""
        # Export to QASM
        qasm_str = arvak.to_qasm(arvak_bell_circuit)
        assert qasm_str is not None

        # Import to Cirq
        cirq_circuit = cirq.circuits.qasm_input.circuit_from_qasm(qasm_str)
        assert cirq_circuit is not None
        assert len(cirq_circuit.all_qubits()) >= 2

    def test_arvak_to_cirq_preserves_structure(self):
        """Test that circuit structure is preserved."""
        # Create Arvak GHZ-3
        arvak_circuit = arvak.Circuit.ghz(3)

        integration = arvak.get_integration('cirq')
        cirq_circuit = integration.from_arvak(arvak_circuit)

        assert len(cirq_circuit.all_qubits()) >= 3

    def test_arvak_to_cirq_qft(self):
        """Test converting QFT circuit."""
        arvak_circuit = arvak.Circuit.qft(4)

        integration = arvak.get_integration('cirq')
        cirq_circuit = integration.from_arvak(arvak_circuit)

        assert len(cirq_circuit.all_qubits()) >= 4


class TestCirqSampler:
    """Tests for Arvak sampler."""

    def test_get_backend_provider(self):
        """Test retrieving backend provider (engine)."""
        integration = arvak.get_integration('cirq')
        engine = integration.get_backend_provider()

        assert engine is not None

    def test_get_sampler(self):
        """Test getting sampler from engine."""
        integration = arvak.get_integration('cirq')
        engine = integration.get_backend_provider()

        sampler = engine.get_sampler()
        assert sampler is not None

    def test_sampler_run(self, cirq_bell_circuit):
        """Test that sampler can run circuits."""
        integration = arvak.get_integration('cirq')
        engine = integration.get_backend_provider()
        sampler = engine.get_sampler()

        # Run circuit
        result = sampler.run(cirq_bell_circuit, repetitions=100)

        assert result is not None
        assert result.repetitions == 100

    def test_sampler_histogram(self, cirq_bell_circuit):
        """Test getting histogram from results."""
        integration = arvak.get_integration('cirq')
        engine = integration.get_backend_provider()
        sampler = engine.get_sampler()

        # Run circuit
        result = sampler.run(cirq_bell_circuit, repetitions=100)

        # Get histogram
        histogram = result.histogram(key='result')
        assert histogram is not None
        assert isinstance(histogram, dict)
        assert len(histogram) > 0


class TestCirqSimulatorResults:
    """Tests that Cirq sampler returns correct quantum simulation results."""

    def test_bell_state_outcomes(self, cirq_bell_circuit):
        """Bell state should only produce 00 and 11 outcomes."""
        integration = arvak.get_integration('cirq')
        engine = integration.get_backend_provider()
        sampler = engine.get_sampler()

        result = sampler.run(cirq_bell_circuit, repetitions=1000)
        histogram = result.histogram(key='result')

        for outcome in histogram.keys():
            # outcome is an integer: 0=00, 3=11
            assert outcome in (0, 3), f"Unexpected outcome: {outcome} (binary: {outcome:02b})"

    def test_bell_state_total_shots(self, cirq_bell_circuit):
        """Bell state total counts should equal requested repetitions."""
        integration = arvak.get_integration('cirq')
        engine = integration.get_backend_provider()
        sampler = engine.get_sampler()

        result = sampler.run(cirq_bell_circuit, repetitions=500)
        histogram = result.histogram(key='result')

        total = sum(histogram.values())
        assert total == 500, f"Expected 500 total shots, got {total}"

    def test_ghz3_outcomes(self):
        """GHZ-3 circuit should only produce 000 and 111."""
        qubits = cirq.LineQubit.range(3)
        circuit = cirq.Circuit(
            cirq.H(qubits[0]),
            cirq.CNOT(qubits[0], qubits[1]),
            cirq.CNOT(qubits[1], qubits[2]),
            cirq.measure(*qubits, key='result')
        )

        integration = arvak.get_integration('cirq')
        engine = integration.get_backend_provider()
        sampler = engine.get_sampler()

        result = sampler.run(circuit, repetitions=1000)
        histogram = result.histogram(key='result')

        for outcome in histogram.keys():
            # 0=000, 7=111
            assert outcome in (0, 7), f"Unexpected outcome: {outcome} (binary: {outcome:03b})"


class TestCirqRoundTrip:
    """Tests for round-trip conversion (Cirq -> Arvak -> Cirq)."""

    def test_roundtrip_preserves_qubits(self, cirq_bell_circuit):
        """Test that round-trip conversion preserves qubit count."""
        integration = arvak.get_integration('cirq')

        # Cirq -> Arvak
        arvak_circuit = integration.to_arvak(cirq_bell_circuit)

        # Arvak -> Cirq
        cirq_circuit_back = integration.from_arvak(arvak_circuit)

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

        integration = arvak.get_integration('cirq')

        # Round-trip
        arvak_circuit = integration.to_arvak(circuit)
        circuit_back = integration.from_arvak(arvak_circuit)

        assert len(circuit_back.all_qubits()) >= len(circuit.all_qubits())


class TestCirqConverter:
    """Tests for Cirq converter functions."""

    def test_cirq_to_arvak_function(self, cirq_bell_circuit):
        """Test cirq_to_arvak converter function."""
        from arvak.integrations.cirq import cirq_to_arvak

        arvak_circuit = cirq_to_arvak(cirq_bell_circuit)
        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits >= 2

    def test_arvak_to_cirq_function(self, arvak_bell_circuit):
        """Test arvak_to_cirq converter function."""
        from arvak.integrations.cirq import arvak_to_cirq

        cirq_circuit = arvak_to_cirq(arvak_bell_circuit)
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

        integration = arvak.get_integration('cirq')
        arvak_circuit = integration.to_arvak(circuit)

        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits >= 2


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
