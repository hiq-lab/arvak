"""PennyLane integration for Arvak.

This module provides seamless integration between PennyLane and Arvak, enabling:
- QNode conversion (PennyLane ↔ Arvak)
- Execution of Arvak circuits through PennyLane's Device API
- Support for quantum machine learning workflows

PennyLane is a quantum machine learning library that emphasizes:
- **Differentiable programming**: Automatic differentiation of quantum circuits
- **ML framework integration**: PyTorch, TensorFlow, JAX compatibility
- **QML algorithms**: Built-in quantum machine learning tools
- **Quantum gradients**: Parameter-shift rules and backpropagation

Example:
    >>> import pennylane as qml
    >>> from arvak.integrations.pennylane import ArvakDevice, pennylane_to_arvak
    >>>
    >>> # Use Arvak as PennyLane device
    >>> dev = ArvakDevice(wires=2, backend='sim')
    >>>
    >>> @qml.qnode(dev)
    >>> def circuit(x):
    ...     qml.RX(x, wires=0)
    ...     qml.CNOT(wires=[0, 1])
    ...     return qml.expval(qml.PauliZ(0))
    >>>
    >>> result = circuit(0.5)
"""

from typing import List
from .._base import FrameworkIntegration


class PennyLaneIntegration(FrameworkIntegration):
    """PennyLane framework integration for Arvak.

    This integration enables conversion between PennyLane and Arvak
    circuits using OpenQASM as an interchange format, and provides a
    PennyLane-compatible device for executing circuits.

    PennyLane's unique features:
    - Automatic differentiation of quantum circuits
    - Integration with ML frameworks (PyTorch, TensorFlow, JAX)
    - Quantum machine learning algorithms
    - Parameter-shift rules for gradients
    """

    @property
    def framework_name(self) -> str:
        """Name of the framework."""
        return "pennylane"

    @property
    def required_packages(self) -> List[str]:
        """Required packages for this integration."""
        return ["pennylane>=0.32.0"]

    def is_available(self) -> bool:
        """Check if PennyLane is installed."""
        try:
            import pennylane
            return True
        except ImportError:
            return False

    def to_arvak(self, qnode_or_tape):
        """Convert PennyLane QNode or tape to Arvak.

        Args:
            qnode_or_tape: PennyLane QNode or QuantumTape

        Returns:
            Arvak Circuit
        """
        from .converter import pennylane_to_arvak
        return pennylane_to_arvak(qnode_or_tape)

    def from_arvak(self, circuit):
        """Convert Arvak circuit to PennyLane QNode.

        Args:
            circuit: Arvak Circuit

        Returns:
            PennyLane QNode function
        """
        from .converter import arvak_to_pennylane
        return arvak_to_pennylane(circuit)

    def get_backend_provider(self):
        """Get Arvak device creator for PennyLane.

        Returns:
            Device creation function
        """
        from .backend import create_device
        return create_device


# Auto-register if PennyLane is available
_integration = PennyLaneIntegration()
if _integration.is_available():
    from .. import IntegrationRegistry
    IntegrationRegistry.register(_integration)

    # Expose public API at package level
    from .backend import ArvakDevice, create_device
    from .converter import pennylane_to_arvak, arvak_to_pennylane

    __all__ = [
        'ArvakDevice',
        'create_device',
        'pennylane_to_arvak',
        'arvak_to_pennylane',
        'PennyLaneIntegration'
    ]
else:
    __all__ = ['PennyLaneIntegration']
