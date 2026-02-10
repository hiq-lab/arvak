"""Qrisp backend client for Arvak.

This module implements Qrisp's backend interface, allowing users to execute
Arvak circuits through Qrisp's backend API.

The backend calls Arvak's built-in Rust statevector simulator directly
via PyO3, returning real simulation results.
"""

from typing import List, Optional, Union, TYPE_CHECKING, Dict

if TYPE_CHECKING:
    from qrisp import QuantumCircuit, QuantumSession


class ArvakBackendClient:
    """Arvak backend client for Qrisp.

    Executes Qrisp circuits on Arvak's built-in Rust statevector simulator.
    Circuits are converted to OpenQASM, parsed in Rust, and simulated with
    exact statevector simulation (up to ~20 qubits).

    Example:
        >>> from arvak.integrations.qrisp import ArvakBackendClient
        >>> from qrisp import QuantumCircuit
        >>> backend = ArvakBackendClient('sim')
        >>> qc = QuantumCircuit(2)
        >>> qc.h(0)
        >>> qc.cx(0, 1)
        >>> qc.measure_all()
        >>> counts = backend.run(qc, shots=1000)
        >>> print(counts)  # {'00': 512, '11': 488}
    """

    def __init__(self, backend_name: str = 'sim'):
        """Initialize the Arvak backend client.

        Args:
            backend_name: Name of the backend to use (default: 'sim')
        """
        self.backend_name = backend_name
        self.name = f'arvak_{backend_name}'
        self.description = f'Arvak Rust statevector simulator ({backend_name})'

    def run(self, circuit: Union['QuantumCircuit', 'QuantumSession'],
            shots: int = 1024, **options) -> Dict[str, int]:
        """Run a Qrisp circuit on Arvak's statevector simulator.

        Args:
            circuit: Qrisp QuantumCircuit or QuantumSession
            shots: Number of measurement shots (default: 1024)
            **options: Additional execution options

        Returns:
            Dictionary mapping bitstrings to measurement counts
        """
        from .converter import qrisp_to_arvak
        import arvak

        arvak_circuit = qrisp_to_arvak(circuit)
        counts = arvak.run_sim(arvak_circuit, shots)
        return counts

    def __repr__(self) -> str:
        return f"<ArvakBackendClient('{self.name}')>"


class ArvakProvider:
    """Arvak backend provider for Qrisp.

    Allows Qrisp programs to discover and use Arvak backends.

    Example:
        >>> from arvak.integrations.qrisp import ArvakProvider
        >>> provider = ArvakProvider()
        >>> backend = provider.get_backend('sim')
    """

    def __init__(self):
        self._backends = {}

    def get_backend(self, name: str = 'sim') -> ArvakBackendClient:
        """Get a specific backend by name.

        Args:
            name: Backend name (default: 'sim')

        Returns:
            ArvakBackendClient instance
        """
        if name not in self._backends:
            self._backends[name] = ArvakBackendClient(name)
        return self._backends[name]

    def backends(self, name: Optional[str] = None, **filters) -> List[ArvakBackendClient]:
        """Get list of available backends."""
        if not self._backends:
            self._backends = {
                'sim': ArvakBackendClient('sim'),
            }

        if name:
            backend = self._backends.get(name)
            return [backend] if backend else []

        return list(self._backends.values())

    def __repr__(self) -> str:
        return f"<ArvakProvider(backends={list(self._backends.keys())})>"
