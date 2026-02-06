"""Qrisp backend client for HIQ.

This module implements Qrisp's backend interface, allowing users to execute
HIQ circuits through Qrisp's backend API.
"""

from typing import List, Optional, Union, TYPE_CHECKING, Dict, Any
import warnings

if TYPE_CHECKING:
    from qrisp import QuantumCircuit, QuantumSession


class HIQBackendClient:
    """HIQ backend client for Qrisp.

    This class implements Qrisp's backend client interface, allowing Qrisp
    programs to execute on HIQ backends.

    Example:
        >>> from arvak.integrations.qrisp import HIQBackendClient
        >>> from qrisp import QuantumVariable
        >>> backend = HIQBackendClient('sim')
        >>> # Use with Qrisp QuantumSession
        >>> qv = QuantumVariable(2)
        >>> qv.h(0)
        >>> qv.cx(0, 1)
    """

    def __init__(self, backend_name: str = 'sim'):
        """Initialize the HIQ backend client.

        Args:
            backend_name: Name of the backend to use (default: 'sim')
        """
        self.backend_name = backend_name
        self.name = f'hiq_{backend_name}'
        self.description = f'HIQ backend: {backend_name}'

    def run(self, circuit: Union['QuantumCircuit', 'QuantumSession'],
            shots: int = 1024, **options) -> Dict[str, int]:
        """Run a Qrisp circuit on HIQ backend.

        Args:
            circuit: Qrisp QuantumCircuit or QuantumSession
            shots: Number of measurement shots (default: 1024)
            **options: Additional execution options

        Returns:
            Dictionary of measurement counts

        Note:
            This is a mock implementation. For actual execution, use the HIQ CLI:
            'hiq run circuit.qasm --backend sim --shots 1000'
        """
        warnings.warn(
            "HIQ backend execution through Qrisp is not yet fully implemented. "
            "For now, please use HIQ CLI for execution: "
            "'hiq run circuit.qasm --backend sim --shots 1000'. "
            "This backend interface will return mock results.",
            RuntimeWarning
        )

        # Convert to HIQ format
        from .converter import qrisp_to_hiq
        import arvak

        hiq_circuit = qrisp_to_hiq(circuit)

        # Create mock results (would execute here in real implementation)
        return self._mock_results(hiq_circuit, shots)

    def _mock_results(self, circuit, shots: int) -> Dict[str, int]:
        """Generate mock results for demonstration.

        Args:
            circuit: HIQ circuit
            shots: Number of shots

        Returns:
            Dictionary of mock measurement counts
        """
        # Return mock Bell state results
        return {
            '00': shots // 2,
            '11': shots // 2,
        }

    def __repr__(self) -> str:
        """String representation of the backend."""
        return f"<HIQBackendClient('{self.name}')>"


class HIQProvider:
    """HIQ backend provider for Qrisp.

    This provider allows Qrisp programs to discover and use HIQ backends.

    Example:
        >>> from arvak.integrations.qrisp import HIQProvider
        >>> provider = HIQProvider()
        >>> backend = provider.get_backend('sim')
    """

    def __init__(self):
        """Initialize the HIQ provider."""
        self._backends = {}

    def get_backend(self, name: str = 'sim') -> HIQBackendClient:
        """Get a specific backend by name.

        Args:
            name: Backend name (default: 'sim')

        Returns:
            HIQBackendClient instance

        Raises:
            ValueError: If backend name is unknown
        """
        if name not in self._backends:
            self._backends[name] = HIQBackendClient(name)

        return self._backends[name]

    def backends(self, name: Optional[str] = None, **filters) -> List[HIQBackendClient]:
        """Get list of available backends.

        Args:
            name: Optional backend name filter
            **filters: Additional filters (currently unused)

        Returns:
            List of HIQBackendClient instances
        """
        # Initialize default backends if not already done
        if not self._backends:
            self._backends = {
                'sim': HIQBackendClient('sim'),
            }

        if name:
            backend = self._backends.get(name)
            return [backend] if backend else []

        return list(self._backends.values())

    def __repr__(self) -> str:
        """String representation of the provider."""
        return f"<HIQProvider(backends={list(self._backends.keys())})>"
