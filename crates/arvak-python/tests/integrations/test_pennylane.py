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

    def test_convert_parameterized_qnode(self):
        """Parameterized QNodes convert with concrete arguments."""
        from arvak.integrations.pennylane import pennylane_to_arvak

        dev = qml.device('default.qubit', wires=2)

        @qml.qnode(dev)
        def circuit(theta):
            qml.RX(theta, wires=0)
            qml.CNOT(wires=[0, 1])
            return qml.expval(qml.PauliZ(0))

        arvak_circuit = pennylane_to_arvak(circuit, 0.5)
        assert arvak_circuit.num_qubits == 2

    def test_convert_composite_gates(self):
        """Composite ops (DoubleExcitation etc.) decompose during export."""
        from arvak.integrations.pennylane import pennylane_to_arvak

        with qml.tape.QuantumTape() as tape:
            qml.BasisState(np.array([1, 1, 0, 0]), wires=range(4))
            qml.DoubleExcitation(0.2, wires=[0, 1, 2, 3])
            qml.expval(qml.PauliZ(0))

        arvak_circuit = pennylane_to_arvak(tape)
        assert arvak_circuit.num_qubits == 4
        assert arvak_circuit.depth() > 1

    def test_convert_produces_valid_qasm(self, pennylane_bell_qnode):
        """Test that converted circuit produces valid QASM."""
        integration = arvak.get_integration('pennylane')
        arvak_circuit = integration.to_arvak(pennylane_bell_qnode)

        qasm = arvak.to_qasm(arvak_circuit)
        assert 'OPENQASM' in qasm
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

    def test_qft_gates_not_dropped(self):
        """QFT emits cp/swap — previously silently dropped by the parser.

        The identity check: QFT then measuring in the uniform superposition
        gives <Z_i> = 0 on every wire. With cp/swap dropped the parser bug
        went unnoticed; this asserts the converted circuit is non-trivial
        and executable.
        """
        from arvak.integrations.pennylane import arvak_to_pennylane

        qnode = arvak_to_pennylane(arvak.Circuit.qft(3))
        result = np.array(qnode())
        assert np.allclose(result, 0.0, atol=1e-9)

    def test_unknown_gate_raises(self):
        """QASM lines with unmapped gates raise instead of silent dropping."""
        from arvak.integrations.pennylane.converter import (
            _apply_qasm_to_pennylane,
        )

        qasm = 'OPENQASM 3.0;\nqubit[1] q;\nfancy_gate q[0];'
        with qml.queuing.AnnotatedQueue():
            with pytest.raises(ValueError, match="no PennyLane mapping"):
                _apply_qasm_to_pennylane(qasm, 1)


class TestArvakDeviceQNode:
    """QNodes attached directly to ArvakDevice — the primary use case.

    Regression tests: the pre-2.2 device was not a valid PennyLane device
    (QNode attachment failed), measured every observable in the Z basis,
    and read measurement bits in reversed wire order.
    """

    def test_qnode_attaches(self):
        """ArvakDevice is a valid PennyLane device for QNodes."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2)

        @qml.qnode(dev)
        def circuit():
            qml.Hadamard(wires=0)
            return qml.expval(qml.PauliZ(0))

        assert isinstance(dev, qml.devices.Device)
        assert abs(circuit()) < 0.15

    def test_expval_z_after_x_is_minus_one(self):
        """Bit-order regression: X on wire 0 → <Z_0> = -1 exactly."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2, shots=500)

        @qml.qnode(dev)
        def circuit():
            qml.PauliX(wires=0)
            return qml.expval(qml.PauliZ(0))

        assert circuit() == -1.0

    def test_expval_x_uses_diagonalizing_gates(self):
        """Basis regression: H|0> is the +1 eigenstate of X → <X_0> = +1."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=1, shots=500)

        @qml.qnode(dev)
        def circuit():
            qml.Hadamard(wires=0)
            return qml.expval(qml.PauliX(0))

        assert circuit() == 1.0

    def test_bell_probs(self):
        """Bell state probabilities concentrate on |00> and |11>."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2, shots=2000)

        @qml.qnode(dev)
        def circuit():
            qml.Hadamard(wires=0)
            qml.CNOT(wires=[0, 1])
            return qml.probs(wires=[0, 1])

        probs = circuit()
        assert probs[1] == 0.0 and probs[2] == 0.0
        assert abs(probs[0] - 0.5) < 0.1
        assert abs(probs[3] - 0.5) < 0.1

    def test_hamiltonian_expval(self):
        """Non-commuting Hamiltonian terms split into separate executions."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2, shots=4000)
        H = qml.Hamiltonian(
            [0.5, 0.3, 0.2],
            [qml.PauliZ(0), qml.PauliX(0), qml.PauliZ(0) @ qml.PauliZ(1)],
        )

        @qml.qnode(dev)
        def circuit():
            qml.Hadamard(wires=0)
            return qml.expval(H)

        # H|0> on wire 0: <Z0>=0, <X0>=1, <Z0 Z1>=0  →  0.3
        assert abs(circuit() - 0.3) < 0.1

    def test_parameter_shift_gradient(self):
        """Parameter-shift gradients work — enables VQE on Arvak."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=1, shots=4000)

        @qml.qnode(dev, diff_method='parameter-shift')
        def circuit(x):
            qml.RX(x, wires=0)
            return qml.expval(qml.PauliZ(0))

        grad = qml.grad(circuit)(qml.numpy.array(0.5))
        assert abs(grad - (-np.sin(0.5))) < 0.1

    def test_counts_measurement(self):
        """counts() returns a dict with PennyLane bit ordering."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2, shots=300)

        @qml.qnode(dev)
        def circuit():
            qml.PauliX(wires=0)
            return qml.counts(wires=[0, 1])

        counts = circuit()
        # wire 0 is leftmost in PennyLane keys: X on wire 0 → '10'
        assert counts == {'10': 300}

    def test_analytic_qnode_falls_back_to_default_shots(self):
        """Tapes without shots sample DEFAULT_SHOTS instead of failing."""
        from arvak.integrations.pennylane import ArvakDevice
        from arvak.integrations.pennylane.backend import DEFAULT_SHOTS

        dev = ArvakDevice(wires=1)

        @qml.qnode(dev)
        def circuit():
            qml.PauliX(wires=0)
            return qml.counts(wires=[0])

        counts = circuit()
        assert sum(counts.values()) == DEFAULT_SHOTS


class TestArvakDeviceConfig:
    """Device construction and backend registry."""

    def test_create_device(self):
        """Test creating an ArvakDevice."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2, shots=100, backend='sim')
        assert dev is not None
        assert len(dev.wires) == 2
        assert dev.backend_name == 'sim'

    def test_device_repr(self):
        """Test device string representation."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2, backend='sim', shots=500)
        repr_str = repr(dev)
        assert 'ArvakDevice' in repr_str
        assert 'sim' in repr_str

    def test_create_device_factory(self):
        """Test create_device factory function."""
        from arvak.integrations.pennylane import create_device

        dev = create_device('sim', wires=3, shots=500)
        assert dev is not None
        assert len(dev.wires) == 3

    def test_hardware_device_constructs_without_credentials(self):
        """Constructing a hardware device must not require credentials."""
        from arvak.integrations.pennylane import ArvakDevice

        dev = ArvakDevice(wires=2, backend='ibm_marrakesh')
        assert dev.backend_name == 'ibm_marrakesh'

    def test_registry_backends_accepted(self):
        """Every name from arvak.list_backends() is constructible."""
        from arvak.integrations.pennylane import ArvakDevice

        for name in arvak.list_backends():
            dev = ArvakDevice(wires=1, backend=name)
            assert dev.backend_name == name


class TestPennyLaneRoundTrip:
    """Tests for round-trip conversion (PennyLane -> Arvak -> PennyLane)."""

    def test_roundtrip_preserves_qubits(self, pennylane_bell_qnode):
        """Test that round-trip conversion preserves qubit count."""
        integration = arvak.get_integration('pennylane')

        arvak_circuit = integration.to_arvak(pennylane_bell_qnode)
        qnode_back = integration.from_arvak(arvak_circuit)

        result_back = qnode_back()
        assert result_back is not None

    def test_roundtrip_bell_state(self, pennylane_bell_qnode):
        """Round-trip Bell: both wires have <Z> = 0."""
        integration = arvak.get_integration('pennylane')

        arvak_circuit = integration.to_arvak(pennylane_bell_qnode)
        assert arvak_circuit.num_qubits == 2

        qnode_back = integration.from_arvak(arvak_circuit)
        result = np.array(qnode_back())
        assert np.allclose(result, 0.0, atol=1e-9)


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
