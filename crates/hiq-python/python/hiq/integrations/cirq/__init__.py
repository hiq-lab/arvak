"""Cirq integration for HIQ.

This module provides seamless integration between Cirq and HIQ, enabling:
- Circuit conversion (Cirq ↔ HIQ)
- Execution of HIQ circuits through Cirq's Sampler API
- Support for Cirq's qubit types (LineQubit, GridQubit)

Cirq is Google's quantum computing framework that emphasizes:
- Hardware-native approaches
- Noise modeling and characterization
- GridQubit for 2D qubit layouts
- NISQ (Noisy Intermediate-Scale Quantum) algorithms

Example:
    >>> import cirq
    >>> from hiq.integrations.cirq import cirq_to_hiq, HIQSampler
    >>>
    >>> # Convert Cirq circuit to HIQ
    >>> qubits = cirq.LineQubit.range(2)
    >>> circuit = cirq.Circuit(
    ...     cirq.H(qubits[0]),
    ...     cirq.CNOT(qubits[0], qubits[1]),
    ...     cirq.measure(*qubits, key='result')
    ... )
    >>> hiq_circuit = cirq_to_hiq(circuit)
    >>>
    >>> # Use HIQ as Cirq sampler
    >>> sampler = HIQSampler('sim')
    >>> result = sampler.run(circuit, repetitions=1000)
"""

from typing import List
from .._base import FrameworkIntegration


class CirqIntegration(FrameworkIntegration):
    """Cirq framework integration for HIQ.

    This integration enables bi-directional conversion between Cirq and HIQ
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

    def to_hiq(self, circuit):
        """Convert Cirq circuit to HIQ.

        Args:
            circuit: Cirq Circuit

        Returns:
            HIQ Circuit
        """
        from .converter import cirq_to_hiq
        return cirq_to_hiq(circuit)

    def from_hiq(self, circuit):
        """Convert HIQ circuit to Cirq.

        Args:
            circuit: HIQ Circuit

        Returns:
            Cirq Circuit
        """
        from .converter import hiq_to_cirq
        return hiq_to_cirq(circuit)

    def get_backend_provider(self):
        """Get HIQ sampler for Cirq.

        Returns:
            HIQEngine instance that provides samplers
        """
        from .backend import HIQEngine
        return HIQEngine()


# Auto-register if Cirq is available
_integration = CirqIntegration()
if _integration.is_available():
    from .. import IntegrationRegistry
    IntegrationRegistry.register(_integration)

    # Expose public API at package level
    from .backend import HIQSampler, HIQEngine
    from .converter import cirq_to_hiq, hiq_to_cirq

    __all__ = [
        'HIQSampler',
        'HIQEngine',
        'cirq_to_hiq',
        'hiq_to_cirq',
        'CirqIntegration'
    ]
else:
    __all__ = ['CirqIntegration']
