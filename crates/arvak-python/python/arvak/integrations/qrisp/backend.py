"""Qrisp backend client for Arvak.

This module implements Qrisp's backend interface, allowing users to execute
Arvak circuits through Qrisp's backend API.
"""

from typing import List, Optional, Union, TYPE_CHECKING, Dict, Any
import warnings

if TYPE_CHECKING:
    from qrisp import QuantumCircuit, QuantumSession


class ArvakBackendClient:
    """Arvak backend client for Qrisp.

    This class implements Qrisp's backend client interface, allowing Qrisp
    programs to execute on Arvak backends.

    Example:
        >>> from arvak.integrations.qrisp import ArvakBackendClient
        >>> from qrisp import QuantumVariable
        >>> backend = ArvakBackendClient('sim')
        >>> # Use with Qrisp QuantumSession
        >>> qv = QuantumVariable(2)
        >>> qv.h(0)
        >>> qv.cx(0, 1)
    """

    def __init__(self, backend_name: str = 'sim'):
        """Initialize the Arvak backend client.

        Args:
            backend_name: Name of the backend to use (default: 'sim')
        """
        self.backend_name = backend_name
        self.name = f'arvak_{backend_name}'
        self.description = f'Arvak backend: {backend_name}'

    def run(self, circuit: Union['QuantumCircuit', 'QuantumSession'],
            shots: int = 1024, **options) -> Dict[str, int]:
        """Run a Qrisp circuit on Arvak backend.

        Args:
            circuit: Qrisp QuantumCircuit or QuantumSession
            shots: Number of measurement shots (default: 1024)
            **options: Additional execution options

        Returns:
            Dictionary of measurement counts

        Note:
            This is a mock implementation. For actual execution, use the Arvak CLI:
            'arvak run circuit.qasm --backend sim --shots 1000'
        """
        warnings.warn(
            "Arvak backend execution through Qrisp is not yet fully implemented. "
            "For now, please use Arvak CLI for execution: "
            "'arvak run circuit.qasm --backend sim --shots 1000'. "
            "This backend interface will return mock results.",
            RuntimeWarning
        )

        # Convert to Arvak format
        from .converter import qrisp_to_arvak
        import arvak

        arvak_circuit = qrisp_to_arvak(circuit)

        # Create mock results (would execute here in real implementation)
        return self._mock_results(arvak_circuit, shots)

    def _mock_results(self, circuit, shots: int) -> Dict[str, int]:
        """Generate mock results for demonstration.

        Args:
            circuit: Arvak circuit
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
        return f"<ArvakBackendClient('{self.name}')>"


class ArvakProvider:
    """Arvak backend provider for Qrisp.

    This provider allows Qrisp programs to discover and use Arvak backends.

    Example:
        >>> from arvak.integrations.qrisp import ArvakProvider
        >>> provider = ArvakProvider()
        >>> backend = provider.get_backend('sim')
    """

    def __init__(self):
        """Initialize the Arvak provider."""
        self._backends = {}

    def get_backend(self, name: str = 'sim') -> ArvakBackendClient:
        """Get a specific backend by name.

        Args:
            name: Backend name (default: 'sim')

        Returns:
            ArvakBackendClient instance

        Raises:
            ValueError: If backend name is unknown
        """
        if name not in self._backends:
            self._backends[name] = ArvakBackendClient(name)

        return self._backends[name]

    def backends(self, name: Optional[str] = None, **filters) -> List[ArvakBackendClient]:
        """Get list of available backends.

        Args:
            name: Optional backend name filter
            **filters: Additional filters (currently unused)

        Returns:
            List of ArvakBackendClient instances
        """
        # Initialize default backends if not already done
        if not self._backends:
            self._backends = {
                'sim': ArvakBackendClient('sim'),
            }

        if name:
            backend = self._backends.get(name)
            return [backend] if backend else []

        return list(self._backends.values())

    def __repr__(self) -> str:
        """String representation of the provider."""
        return f"<ArvakProvider(backends={list(self._backends.keys())})>"
