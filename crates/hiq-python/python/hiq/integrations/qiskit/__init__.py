"""Qiskit integration for HIQ.

This module provides seamless integration between Qiskit and HIQ, enabling:
- Circuit conversion (Qiskit ↔ HIQ)
- Execution of HIQ circuits through Qiskit's backend API
- Access to HIQ's advanced compilation capabilities from Qiskit

Example:
    >>> from qiskit import QuantumCircuit
    >>> from hiq.integrations.qiskit import qiskit_to_hiq, HIQProvider
    >>>
    >>> # Convert Qiskit circuit to HIQ
    >>> qc = QuantumCircuit(2)
    >>> qc.h(0)
    >>> qc.cx(0, 1)
    >>> hiq_circuit = qiskit_to_hiq(qc)
    >>>
    >>> # Use HIQ as Qiskit backend
    >>> provider = HIQProvider()
    >>> backend = provider.get_backend('sim')
    >>> job = backend.run(qc, shots=1000)
    >>> result = job.result()
"""

from typing import List
from .._base import FrameworkIntegration


class QiskitIntegration(FrameworkIntegration):
    """Qiskit framework integration for HIQ.

    This integration enables bi-directional conversion between Qiskit and HIQ
    circuits using OpenQASM 3.0 as an interchange format, and provides a
    Qiskit-compatible backend provider for executing circuits.
    """

    @property
    def framework_name(self) -> str:
        """Name of the framework."""
        return "qiskit"

    @property
    def required_packages(self) -> List[str]:
        """Required packages for this integration."""
        return ["qiskit>=1.0.0"]

    def is_available(self) -> bool:
        """Check if Qiskit is installed."""
        try:
            import qiskit
            return True
        except ImportError:
            return False

    def to_hiq(self, circuit):
        """Convert Qiskit circuit to HIQ.

        Args:
            circuit: Qiskit QuantumCircuit

        Returns:
            HIQ Circuit
        """
        from .converter import qiskit_to_hiq
        return qiskit_to_hiq(circuit)

    def from_hiq(self, circuit):
        """Convert HIQ circuit to Qiskit.

        Args:
            circuit: HIQ Circuit

        Returns:
            Qiskit QuantumCircuit
        """
        from .converter import hiq_to_qiskit
        return hiq_to_qiskit(circuit)

    def get_backend_provider(self):
        """Get HIQ backend provider for Qiskit.

        Returns:
            HIQProvider instance
        """
        from .backend import HIQProvider
        return HIQProvider()


# Auto-register if Qiskit is available
_integration = QiskitIntegration()
if _integration.is_available():
    from .. import IntegrationRegistry
    IntegrationRegistry.register(_integration)

    # Expose public API at package level
    from .backend import HIQProvider
    from .converter import qiskit_to_hiq, hiq_to_qiskit

    __all__ = ['HIQProvider', 'qiskit_to_hiq', 'hiq_to_qiskit', 'QiskitIntegration']
else:
    __all__ = ['QiskitIntegration']
