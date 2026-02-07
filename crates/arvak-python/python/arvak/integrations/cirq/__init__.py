"""Cirq integration for Arvak.

This module provides seamless integration between Cirq and Arvak, enabling:
- Circuit conversion (Cirq ↔ Arvak)
- Execution of Arvak circuits through Cirq's Sampler API
- Support for Cirq's qubit types (LineQubit, GridQubit)

Cirq is Google's quantum computing framework that emphasizes:
- Hardware-native approaches
- Noise modeling and characterization
- GridQubit for 2D qubit layouts
- NISQ (Noisy Intermediate-Scale Quantum) algorithms

Example:
    >>> import cirq
    >>> from arvak.integrations.cirq import cirq_to_arvak, ArvakSampler
    >>>
    >>> # Convert Cirq circuit to Arvak
    >>> qubits = cirq.LineQubit.range(2)
    >>> circuit = cirq.Circuit(
    ...     cirq.H(qubits[0]),
    ...     cirq.CNOT(qubits[0], qubits[1]),
    ...     cirq.measure(*qubits, key='result')
    ... )
    >>> arvak_circuit = cirq_to_arvak(circuit)
    >>>
    >>> # Use Arvak as Cirq sampler
    >>> sampler = ArvakSampler('sim')
    >>> result = sampler.run(circuit, repetitions=1000)
"""

from typing import List
from .._base import FrameworkIntegration


class CirqIntegration(FrameworkIntegration):
    """Cirq framework integration for Arvak.

    This integration enables bi-directional conversion between Cirq and Arvak
    circuits using OpenQASM as an interchange format, and provides a
    Cirq-compatible sampler for executing circuits.

    Cirq's unique features:
    - LineQubit and GridQubit for different topologies
    - Hardware-native gate sets
    - Noise models and error mitigation
    - QAOA and VQE implementations
    """

    @property
    def framework_name(self) -> str:
        """Name of the framework."""
        return "cirq"

    @property
    def required_packages(self) -> List[str]:
        """Required packages for this integration."""
        return ["cirq>=1.0.0", "cirq-core>=1.0.0"]

    def is_available(self) -> bool:
        """Check if Cirq is installed."""
        try:
            import cirq
            return True
        except ImportError:
            return False

    def to_arvak(self, circuit):
        """Convert Cirq circuit to Arvak.

        Args:
            circuit: Cirq Circuit

        Returns:
            Arvak Circuit
        """
        from .converter import cirq_to_arvak
        return cirq_to_arvak(circuit)

    def from_arvak(self, circuit):
        """Convert Arvak circuit to Cirq.

        Args:
            circuit: Arvak Circuit

        Returns:
            Cirq Circuit
        """
        from .converter import arvak_to_cirq
        return arvak_to_cirq(circuit)

    def get_backend_provider(self):
        """Get Arvak sampler for Cirq.

        Returns:
            ArvakEngine instance that provides samplers
        """
        from .backend import ArvakEngine
        return ArvakEngine()


# Auto-register if Cirq is available
_integration = CirqIntegration()
if _integration.is_available():
    from .. import IntegrationRegistry
    IntegrationRegistry.register(_integration)

    # Expose public API at package level
    from .backend import ArvakSampler, ArvakEngine
    from .converter import cirq_to_arvak, arvak_to_cirq

    __all__ = [
        'ArvakSampler',
        'ArvakEngine',
        'cirq_to_arvak',
        'arvak_to_cirq',
        'CirqIntegration'
    ]
else:
    __all__ = ['CirqIntegration']
