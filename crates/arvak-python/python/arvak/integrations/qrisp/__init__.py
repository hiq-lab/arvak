"""Qrisp integration for Arvak.

This module provides seamless integration between Qrisp and Arvak, enabling:
- Circuit conversion (Qrisp ↔ Arvak)
- Execution of Arvak circuits through Qrisp's backend API
- Support for Qrisp's high-level quantum programming model

Qrisp is a high-level quantum programming framework that emphasizes:
- Automatic uncomputation
- High-level quantum data structures (QuantumVariable)
- Quantum sessions and compilation

Example:
    >>> from qrisp import QuantumCircuit
    >>> from arvak.integrations.qrisp import qrisp_to_arvak, ArvakBackendClient
    >>>
    >>> # Convert Qrisp circuit to Arvak
    >>> qc = QuantumCircuit(2)
    >>> qc.h(0)
    >>> qc.cx(0, 1)
    >>> arvak_circuit = qrisp_to_arvak(qc)
    >>>
    >>> # Use Arvak as Qrisp backend
    >>> backend = ArvakBackendClient('sim')
    >>> results = backend.run(qc, shots=1000)
"""

from typing import List
from .._base import FrameworkIntegration


class QrispIntegration(FrameworkIntegration):
    """Qrisp framework integration for Arvak.

    This integration enables bi-directional conversion between Qrisp and Arvak
    circuits using OpenQASM as an interchange format, and provides a
    Qrisp-compatible backend client for executing circuits.

    Qrisp's unique features:
    - High-level quantum programming with QuantumVariable
    - Automatic uncomputation
    - QuantumSession for managing quantum state
    - Built-in quantum algorithms
    """

    @property
    def framework_name(self) -> str:
        """Name of the framework."""
        return "qrisp"

    @property
    def required_packages(self) -> List[str]:
        """Required packages for this integration."""
        return ["qrisp>=0.4.0"]

    def is_available(self) -> bool:
        """Check if Qrisp is installed."""
        try:
            import qrisp
            return True
        except ImportError:
            return False

    def to_arvak(self, circuit):
        """Convert Qrisp circuit to Arvak.

        Args:
            circuit: Qrisp QuantumCircuit or QuantumSession

        Returns:
            Arvak Circuit
        """
        from .converter import qrisp_to_arvak
        return qrisp_to_arvak(circuit)

    def from_arvak(self, circuit):
        """Convert Arvak circuit to Qrisp.

        Args:
            circuit: Arvak Circuit

        Returns:
            Qrisp QuantumCircuit
        """
        from .converter import arvak_to_qrisp
        return arvak_to_qrisp(circuit)

    def get_backend_provider(self):
        """Get Arvak backend provider for Qrisp.

        Returns:
            ArvakProvider instance
        """
        from .backend import ArvakProvider
        return ArvakProvider()


# Auto-register if Qrisp is available
_integration = QrispIntegration()
if _integration.is_available():
    from .. import IntegrationRegistry
    IntegrationRegistry.register(_integration)

    # Expose public API at package level
    from .backend import ArvakBackendClient, ArvakProvider
    from .converter import qrisp_to_arvak, arvak_to_qrisp

    __all__ = [
        'ArvakBackendClient',
        'ArvakProvider',
        'qrisp_to_arvak',
        'arvak_to_qrisp',
        'QrispIntegration'
    ]
else:
    __all__ = ['QrispIntegration']
