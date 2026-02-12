"""Qiskit integration for Arvak.

This module provides seamless integration between Qiskit and Arvak, enabling:
- Circuit conversion (Qiskit ↔ Arvak)
- Execution of Arvak circuits through Qiskit's backend API
- Access to Arvak's advanced compilation capabilities from Qiskit

Example:
    >>> from qiskit import QuantumCircuit
    >>> from arvak.integrations.qiskit import qiskit_to_arvak, ArvakProvider
    >>>
    >>> # Convert Qiskit circuit to Arvak
    >>> qc = QuantumCircuit(2)
    >>> qc.h(0)
    >>> qc.cx(0, 1)
    >>> arvak_circuit = qiskit_to_arvak(qc)
    >>>
    >>> # Use Arvak as Qiskit backend
    >>> provider = ArvakProvider()
    >>> backend = provider.get_backend('sim')
    >>> job = backend.run(qc, shots=1000)
    >>> result = job.result()
"""

from .._base import FrameworkIntegration


class QiskitIntegration(FrameworkIntegration):
    """Qiskit framework integration for Arvak.

    This integration enables bi-directional conversion between Qiskit and Arvak
    circuits using OpenQASM 3.0 as an interchange format, and provides a
    Qiskit-compatible backend provider for executing circuits.
    """

    @property
    def framework_name(self) -> str:
        """Name of the framework."""
        return "qiskit"

    @property
    def required_packages(self) -> list[str]:
        """Required packages for this integration."""
        return ["qiskit>=1.0.0"]

    def is_available(self) -> bool:
        """Check if Qiskit is installed."""
        try:
            import qiskit
            return True
        except ImportError:
            return False

    def to_arvak(self, circuit):
        """Convert Qiskit circuit to Arvak.

        Args:
            circuit: Qiskit QuantumCircuit

        Returns:
            Arvak Circuit
        """
        from .converter import qiskit_to_arvak
        return qiskit_to_arvak(circuit)

    def from_arvak(self, circuit):
        """Convert Arvak circuit to Qiskit.

        Args:
            circuit: Arvak Circuit

        Returns:
            Qiskit QuantumCircuit
        """
        from .converter import arvak_to_qiskit
        return arvak_to_qiskit(circuit)

    def get_backend_provider(self):
        """Get Arvak backend provider for Qiskit.

        Returns:
            ArvakProvider instance
        """
        from .backend import ArvakProvider
        return ArvakProvider()


# Auto-register if Qiskit is available
_integration = QiskitIntegration()
if _integration.is_available():
    from .. import IntegrationRegistry
    IntegrationRegistry.register(_integration)

    # Expose public API at package level
    from .backend import ArvakProvider
    from .converter import qiskit_to_arvak, arvak_to_qiskit

    __all__ = ['ArvakProvider', 'qiskit_to_arvak', 'arvak_to_qiskit', 'QiskitIntegration']
else:
    __all__ = ['QiskitIntegration']
