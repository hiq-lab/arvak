"""Pulser integration for Arvak.

This module provides integration between Pulser (Pasqal) and Arvak, enabling:
- Circuit conversion (Arvak → Pulser Sequence)
- Gate-to-pulse decomposition for neutral-atom hardware
- Support for DigitalAnalogDevice

Pulser is Pasqal's SDK for programming neutral-atom quantum processors using:
- Analog pulse sequences on Rydberg and Raman channels
- Digital mode for gate-based operations
- Register layouts for atom positioning

Example:
    >>> import arvak
    >>> from arvak.integrations.pulser import arvak_to_pulser
    >>>
    >>> # Compile and convert to Pulser
    >>> circuit = arvak.Circuit("bell", num_qubits=2)
    >>> circuit.h(0).cx(0, 1)
    >>> compiled = arvak.compile(circuit)
    >>> sequence = arvak_to_pulser(compiled)
"""

from .._base import FrameworkIntegration


class PulserIntegration(FrameworkIntegration):
    """Pulser framework integration for Arvak.

    This integration converts compiled Arvak gate circuits to Pulser
    Sequences using gate-to-pulse decomposition on the DigitalAnalogDevice.

    Single-qubit gates map to raman_local pulses.
    Two-qubit gates map to rydberg_global pulses (blockade-based entanglement).
    """

    @property
    def framework_name(self) -> str:
        """Name of the framework."""
        return "pulser"

    @property
    def required_packages(self) -> list[str]:
        """Required packages for this integration."""
        return ["pulser-core>=1.0.0"]

    def is_available(self) -> bool:
        """Check if Pulser is installed."""
        try:
            import pulser
            return True
        except ImportError:
            return False

    def to_arvak(self, circuit):
        """Convert Pulser Sequence to Arvak Circuit.

        Args:
            circuit: Pulser Sequence (digital mode)

        Returns:
            Arvak Circuit
        """
        from .converter import pulser_to_arvak
        return pulser_to_arvak(circuit)

    def from_arvak(self, circuit):
        """Convert Arvak Circuit to Pulser Sequence.

        Args:
            circuit: Arvak Circuit

        Returns:
            Pulser Sequence
        """
        from .converter import arvak_to_pulser
        return arvak_to_pulser(circuit)

    def get_backend_provider(self):
        """Get Pulser backend provider.

        Returns:
            Pulser QPUBackend class (user must configure with device address)
        """
        from pulser.backends import QPUBackend
        return QPUBackend


# Auto-register if Pulser is available
_integration = PulserIntegration()
if _integration.is_available():
    from .. import IntegrationRegistry
    IntegrationRegistry.register(_integration)

    from .converter import arvak_to_pulser, pulser_to_arvak

    __all__ = [
        'arvak_to_pulser',
        'pulser_to_arvak',
        'PulserIntegration',
    ]
else:
    __all__ = ['PulserIntegration']
