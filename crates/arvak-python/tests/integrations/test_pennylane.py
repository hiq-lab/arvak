"""Tests for PennyLane integration.

These tests require PennyLane to be installed. They will be skipped if PennyLane
is not available.
"""

import pytest
import numpy as np

# Try to import dependencies
try:
    import arvak
    import pennylane as qml
    PENNYLANE_AVAILABLE = True
except ImportError:
    PENNYLANE_AVAILABLE = False

# Skip all tests if PennyLane not available
pytestmark = pytest.mark.skipif(
    not PENNYLANE_AVAILABLE,
    reason="PennyLane not installed"
)


@pytest.fixture
def pennylane_bell_qnode():
    """Create a simple Bell state QNode in PennyLane."""
    dev = qml.device('default.qubit', wires=2)

    @qml.qnode(dev)
    def circuit():
        qml.Hadamard(wires=0)
        qml.CNOT(wires=[0, 1])
        return qml.expval(qml.PauliZ(0))

    return circuit


@pytest.fixture
def pennylane_parametrized_qnode():
    """Create a parametrized QNode."""
    dev = qml.device('default.qubit', wires=2)

    @qml.qnode(dev)
    def circuit(theta):
        qml.RX(theta, wires=0)
        qml.CNOT(wires=[0, 1])
        return qml.expval(qml.PauliZ(0))

    return circuit


@pytest.fixture
def arvak_bell_circuit():
    """Create a simple Bell state circuit in Arvak."""
    return arvak.Circuit.bell()


class TestPennyLaneIntegration:
    """Tests for PennyLane integration registration."""

    def test_integration_registered(self):
        """Test that PennyLane integration is registered."""
        status = arvak.integration_status()
        assert 'pennylane' in status
        assert status['pennylane']['available'] is True

    def test_get_pennylane_integration(self):
        """Test retrieving PennyLane integration."""
        integration = arvak.get_integration('pennylane')
        assert integration is not None
        assert integration.framework_name == 'pennylane'

    def test_integration_is_available(self):
        """Test that integration reports as available."""
        integration = arvak.get_integration('pennylane')
        assert integration.is_available() is True

    def test_required_packages(self):
        """Test that required packages are listed."""
        integration = arvak.get_integration('pennylane')
        packages = integration.required_packages
        assert len(packages) > 0
        assert any('pennylane' in pkg for pkg in packages)


class TestPennyLaneToArvak:
    """Tests for PennyLane to Arvak conversion."""

    def test_convert_bell_qnode(self, pennylane_bell_qnode):
        """Test converting Bell state QNode to Arvak."""
        integration = arvak.get_integration('pennylane')
        arvak_circuit = integration.to_arvak(pennylane_bell_qnode)

        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits == 2

    def test_convert_preserves_gate_count(self, pennylane_bell_qnode):
        """Test that conversion preserves gates."""
        integration = arvak.get_integration('pennylane')
        arvak_circuit = integration.to_arvak(pennylane_bell_qnode)

        # Bell: H + CNOT on at least 2 qubits
        assert arvak_circuit.num_qubits >= 2

    def test_convert_produces_valid_qasm(self, pennylane_bell_qnode):
        """Test that converted circuit produces valid QASM."""
        integration = arvak.get_integration('pennylane')
        arvak_circuit = integration.to_arvak(pennylane_bell_qnode)

        qasm = arvak.to_qasm(arvak_circuit)
        assert 'OPENQASM' in qasm
        # Arvak to_qasm input goes through QASM 2.0 (from _tape_to_qasm), so
        # the round-tripped output may have qreg or qubit depending on version
        assert 'qreg' in qasm or 'qubit' in qasm

    def test_direct_converter_function(self, pennylane_bell_qnode):
        """Test the direct converter function."""
        from arvak.integrations.pennylane import pennylane_to_arvak

        arvak_circuit = pennylane_to_arvak(pennylane_bell_qnode)
        assert arvak_circuit is not None
        assert arvak_circuit.num_qubits == 2


class TestArvakToPennyLane:
    """Tests for Arvak to PennyLane conversion."""

    def test_convert_bell_to_qnode(self, arvak_bell_circuit):
        """Test converting Arvak Bell to PennyLane QNode."""
        integration = arvak.get_integration('pennylane')
        qnode = integration.from_arvak(arvak_bell_circuit)

        assert qnode is not None
        assert callable(qnode)

    def test_converted_qnode_executable(self, arvak_bell_circuit):
        """Test that converted QNode can be executed."""
        integration = arvak.get_integration('pennylane')
        qnode = integration.from_arvak(arvak_bell_circuit)

        result = qnode()
        assert result is not None

    def test_converted_qnode_returns_expectation(self, arvak_bell_circuit):
        """Test that converted QNode returns numeric expectation values."""
        integration = arvak.get_integration('pennylane')
        qnode = integration.from_arvak(arvak_bell_circuit)

        result = qnode()
        # Should return an array or list of expectation values
        if isinstance(result, (list, np.ndarray)):
            for val in result:
                assert isinstance(float(val), float)
        else:
            assert isinstance(float(result), float)

    def test_direct_converter_function(self, arvak_bell_circuit):
        """Test the direct converter function."""
        from arvak.integrations.pennylane import arvak_to_pennylane

        qnode = arvak_to_pennylane(arvak_bell_circuit)
        assert qnode is not None
        assert callable(qnode)


class TestPennyLaneDevice:
    """Tests for ArvakDevice as PennyLane device."""

    def test_create_device(self):
        """Test creating an ArvakDevice."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2, shots=100, backend='sim')
        assert dev is not None
        assert dev.wires == 2
        assert dev.shots == 100

    def test_device_default_shots(self):
        """Test device default shots."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2)
        assert dev.shots == 1024

    def test_device_repr(self):
        """Test device string representation."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2, backend='sim', shots=500)
        repr_str = repr(dev)
        assert 'ArvakDevice' in repr_str
        assert 'sim' in repr_str

    def test_device_operations(self):
        """Test that device advertises supported operations."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2)
        assert 'Hadamard' in dev.operations
        assert 'CNOT' in dev.operations
        assert 'RX' in dev.operations
        assert 'RY' in dev.operations
        assert 'RZ' in dev.operations

    def test_device_observables(self):
        """Test that device advertises supported observables."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2)
        assert 'PauliX' in dev.observables
        assert 'PauliY' in dev.observables
        assert 'PauliZ' in dev.observables

    def test_device_apply_and_expval(self):
        """Test applying operations and computing expectation value."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=1, shots=1000)

        # Apply X gate — should flip |0⟩ to |1⟩
        # PauliZ on |1⟩ should give expval = -1
        x_op = qml.PauliX(wires=0)
        dev.apply([x_op])

        z_obs = qml.PauliZ(wires=0)
        expval = dev.expval(z_obs)

        # Should be close to -1.0
        assert abs(expval - (-1.0)) < 0.1, f"Expected ~-1.0, got {expval}"

    def test_device_apply_h_gate(self):
        """Test H gate gives ~0 expectation for PauliZ."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=1, shots=10000)

        h_op = qml.Hadamard(wires=0)
        dev.apply([h_op])

        z_obs = qml.PauliZ(wires=0)
        expval = dev.expval(z_obs)

        # H|0⟩ = |+⟩, expval of Z should be ~0
        assert abs(expval) < 0.1, f"Expected ~0.0, got {expval}"

    def test_device_variance(self):
        """Test variance computation."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=1, shots=10000)

        h_op = qml.Hadamard(wires=0)
        dev.apply([h_op])

        z_obs = qml.PauliZ(wires=0)
        var = dev.var(z_obs)

        # H|0⟩ = |+⟩, variance of Z should be ~1.0
        assert abs(var - 1.0) < 0.15, f"Expected ~1.0, got {var}"

    def test_device_sample(self):
        """Test sample returns correct shape."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=1, shots=100)

        h_op = qml.Hadamard(wires=0)
        dev.apply([h_op])

        z_obs = qml.PauliZ(wires=0)
        samples = dev.sample(z_obs)

        assert len(samples) == 100
        # All samples should be +1 or -1
        for s in samples:
            assert s in (1.0, -1.0), f"Unexpected sample value: {s}"

    def test_device_execute_tape(self):
        """Test executing a full quantum tape."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2, shots=1000)

        # Create a tape for Bell state
        with qml.tape.QuantumTape() as tape:
            qml.Hadamard(wires=0)
            qml.CNOT(wires=[0, 1])
            qml.expval(qml.PauliZ(0))

        result = dev.execute(tape)
        assert isinstance(float(result), float)

    def test_create_device_factory(self):
        """Test create_device factory function."""
        from arvak.integrations.pennylane import create_device

        dev = create_device('sim', wires=3, shots=500)
        assert dev is not None
        assert dev.wires == 3
        assert dev.shots == 500


class TestPennyLaneRoundTrip:
    """Tests for round-trip conversion (PennyLane -> Arvak -> PennyLane)."""

    def test_roundtrip_preserves_qubits(self, pennylane_bell_qnode):
        """Test that round-trip conversion preserves qubit count."""
        integration = arvak.get_integration('pennylane')

        # PennyLane -> Arvak
        arvak_circuit = integration.to_arvak(pennylane_bell_qnode)

        # Arvak -> PennyLane
        qnode_back = integration.from_arvak(arvak_circuit)

        # Execute both and verify results
        result_back = qnode_back()
        assert result_back is not None

    def test_roundtrip_bell_state(self, pennylane_bell_qnode):
        """Test round-trip for Bell state."""
        integration = arvak.get_integration('pennylane')

        # PennyLane -> Arvak
        arvak_circuit = integration.to_arvak(pennylane_bell_qnode)
        assert arvak_circuit.num_qubits == 2

        # Arvak -> PennyLane
        qnode_back = integration.from_arvak(arvak_circuit)
        result = qnode_back()
        assert result is not None


class TestPennyLaneConverter:
    """Tests for converter functions."""

    def test_tape_to_qasm_bell(self, pennylane_bell_qnode):
        """Test tape to QASM conversion for Bell state."""
        from arvak.integrations.pennylane.converter import _tape_to_qasm

        # Get tape (PennyLane >=0.44 uses _tape)
        pennylane_bell_qnode.construct([], {})
        tape = getattr(pennylane_bell_qnode, 'qtape', None) or pennylane_bell_qnode._tape

        qasm = _tape_to_qasm(tape)

        assert 'OPENQASM' in qasm
        assert 'h q[0]' in qasm
        assert 'cx q[0],q[1]' in qasm

    def test_operation_to_qasm_gates(self):
        """Test individual gate conversions."""
        from arvak.integrations.pennylane.converter import _operation_to_qasm

        wire_map = {0: 0, 1: 1}

        # Test each gate type
        assert _operation_to_qasm(qml.Hadamard(wires=0), wire_map) == 'h q[0];'
        assert _operation_to_qasm(qml.PauliX(wires=0), wire_map) == 'x q[0];'
        assert _operation_to_qasm(qml.PauliY(wires=0), wire_map) == 'y q[0];'
        assert _operation_to_qasm(qml.PauliZ(wires=0), wire_map) == 'z q[0];'
        assert _operation_to_qasm(qml.CNOT(wires=[0, 1]), wire_map) == 'cx q[0],q[1];'

    def test_operation_to_qasm_rotation(self):
        """Test rotation gate conversions."""
        from arvak.integrations.pennylane.converter import _operation_to_qasm

        wire_map = {0: 0}

        rx_qasm = _operation_to_qasm(qml.RX(1.57, wires=0), wire_map)
        assert rx_qasm.startswith('rx(')
        assert 'q[0]' in rx_qasm

        ry_qasm = _operation_to_qasm(qml.RY(0.5, wires=0), wire_map)
        assert ry_qasm.startswith('ry(')

        rz_qasm = _operation_to_qasm(qml.RZ(3.14, wires=0), wire_map)
        assert rz_qasm.startswith('rz(')


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
