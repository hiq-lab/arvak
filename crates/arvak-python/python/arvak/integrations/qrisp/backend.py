"""Qrisp backend client for Arvak.

This module implements Qrisp's backend interface, allowing users to execute
Arvak circuits through Qrisp's backend API.

Supported backends:
  - 'sim'      : Arvak's built-in Rust statevector simulator (no credentials)
  - 'iqm'      : IQM Resonance QPU via iqm-client / qiskit-iqm (requires IQM_TOKEN)
  - 'scaleway' : Scaleway QaaS (IQM QPU) via REST API (requires SCALEWAY credentials)
"""

import os
from typing import Optional, Union

# IQM Resonance endpoint
_IQM_RESONANCE_URL = "https://resonance.meetiqm.com/"

# IQM topology coupling maps (qubit count and connectivity style)
# Sirius  : 16-qubit  star topology  (QPU-SIRIUS-24PQ physical, 16 usable)
# Garnet  : 20-qubit  crystal topology
# Emerald : 54-qubit  crystal topology
IQM_TOPOLOGIES = {
    "sirius": {"qubits": 16, "topology": "star"},
    "garnet": {"qubits": 20, "topology": "crystal"},
    "emerald": {"qubits": 54, "topology": "crystal"},
}

# Scaleway platform identifiers for IQM QPUs
SCALEWAY_PLATFORMS = {
    "QPU-SIRIUS-24PQ": {"qubits": 16, "topology": "star"},
    "QPU-GARNET-20PQ": {"qubits": 20, "topology": "crystal"},
    "QPU-EMERALD-54PQ": {"qubits": 54, "topology": "crystal"},
}

# Scaleway QaaS REST endpoint
_SCALEWAY_API_URL = "https://api.scaleway.com/qaas/v1alpha1"


class ArvakBackendClient:
    """Arvak backend client for Qrisp.

    Executes Qrisp circuits on Arvak backends:
      - 'sim'      : Rust statevector simulator (up to ~20 qubits)
      - 'iqm'      : IQM Resonance real QPU hardware
      - 'scaleway' : Scaleway QaaS (IQM QPU) real hardware

    Example (simulator)::

        >>> from arvak.integrations.qrisp import ArvakBackendClient
        >>> from qrisp import QuantumCircuit
        >>> backend = ArvakBackendClient('sim')
        >>> qc = QuantumCircuit(2)
        >>> qc.h(0)
        >>> qc.cx(0, 1)
        >>> qc.measure_all()
        >>> counts = backend.run(qc, shots=1000)
        >>> print(counts)  # {'00': 512, '11': 488}

    Example (IQM Resonance — requires credentials)::

        >>> import os
        >>> os.environ['IQM_TOKEN'] = '<your-resonance-token>'
        >>> backend = ArvakBackendClient('iqm')
        >>> counts = backend.run(qc, shots=1024)
    """

    # Backends that route to real quantum hardware
    _HARDWARE_BACKENDS = {'iqm', 'scaleway'}

    def __init__(self, backend_name: str = 'sim'):
        """Initialize the Arvak backend client.

        Args:
            backend_name: Name of the backend to use.
                          One of: 'sim', 'iqm', 'scaleway'. (default: 'sim')
        """
        self.backend_name = backend_name
        self.name = f'arvak_{backend_name}'

        if backend_name == 'sim':
            self.description = 'Arvak Rust statevector simulator'
        elif backend_name == 'iqm':
            self.description = 'IQM Resonance QPU (via Arvak)'
        elif backend_name == 'scaleway':
            self.description = 'Scaleway QaaS / IQM QPU (via Arvak)'
        else:
            self.description = f'Arvak backend ({backend_name})'

    def run(self, circuit: Union['QuantumCircuit', 'QuantumSession'],
            shots: int = 1024, **options) -> dict[str, int]:
        """Run a Qrisp circuit on the configured Arvak backend.

        For the simulator ('sim') the circuit is executed locally in Rust with
        no network calls.  For real hardware backends ('iqm', 'scaleway') the
        circuit is compiled with Arvak's Rust compiler (routing + gate
        translation) and then submitted via the appropriate API.

        Args:
            circuit: Qrisp QuantumCircuit or QuantumSession.
            shots:   Number of measurement shots (default: 1024).
            **options: Additional execution options passed to the backend.

        Returns:
            Dictionary mapping bitstrings to measurement counts.

        Raises:
            RuntimeError: For hardware backends if required credentials are
                          not set in the environment.
            ImportError:  For IQM backend if ``iqm-client`` / ``qiskit-iqm``
                          are not installed.
        """
        from .converter import qrisp_to_arvak
        import arvak

        arvak_circuit = qrisp_to_arvak(circuit)

        if self.backend_name == 'sim':
            return self._run_sim(arvak_circuit, shots)
        elif self.backend_name == 'iqm':
            return self._run_iqm(arvak_circuit, shots, **options)
        elif self.backend_name == 'scaleway':
            return self._run_scaleway(arvak_circuit, shots, **options)
        else:
            raise ValueError(
                f"Unknown backend: {self.backend_name!r}. "
                f"Available backends: sim, iqm, scaleway"
            )

    # ------------------------------------------------------------------
    # Private helpers
    # ------------------------------------------------------------------

    def _run_sim(self, arvak_circuit, shots: int) -> dict[str, int]:
        """Execute locally on Arvak's Rust statevector simulator."""
        import arvak
        return arvak.run_sim(arvak_circuit, shots)

    def _run_iqm(self, arvak_circuit, shots: int, **options) -> dict[str, int]:
        """Compile with Arvak and submit to IQM Resonance.

        Requires:
          - ``IQM_TOKEN``    env var (Resonance bearer token)
          - ``IQM_COMPUTER`` env var (default: 'sirius'; options: garnet, emerald)
          - ``iqm-client`` and ``qiskit-iqm`` Python packages

        The circuit is routed and gate-translated by Arvak's Rust compiler
        for the selected IQM topology, then submitted via ``qiskit-iqm``.
        """
        import arvak

        token = os.environ.get('IQM_TOKEN')
        if not token:
            raise RuntimeError(
                "IQM_TOKEN environment variable not set.\n"
                "Get your token from https://resonance.meetiqm.com (account drawer)."
            )

        computer = options.get('computer', os.environ.get('IQM_COMPUTER', 'sirius'))
        topo_info = IQM_TOPOLOGIES.get(computer, IQM_TOPOLOGIES['sirius'])
        num_qubits = topo_info['qubits']
        topology = topo_info['topology']

        # Compile circuit with Arvak for the target IQM topology
        if topology == 'star':
            coupling = arvak.CouplingMap.star(num_qubits)
        else:
            # crystal topology — use linear approximation until CouplingMap.crystal
            # is available; the compiler will handle remapping
            coupling = arvak.CouplingMap.linear(num_qubits)

        basis = arvak.BasisGates.iqm()
        compiled = arvak.compile(
            arvak_circuit,
            coupling_map=coupling,
            basis_gates=basis,
            optimization_level=options.get('optimization_level', 1),
        )

        # Export compiled circuit to QASM3
        qasm_str = arvak.to_qasm(compiled)

        # Submit to IQM Resonance via qiskit-iqm
        try:
            from iqm.qiskit_iqm import IQMProvider
            from qiskit import QuantumCircuit as QiskitCircuit, transpile
            from qiskit.qasm3 import loads as qasm3_loads
        except ImportError as exc:
            raise ImportError(
                "IQM backend requires 'iqm-client' and 'qiskit-iqm' packages.\n"
                "Install with: pip install iqm-client qiskit-iqm"
            ) from exc

        # Parse compiled QASM back to Qiskit for submission
        qiskit_circuit = qasm3_loads(qasm_str)

        provider = IQMProvider(
            _IQM_RESONANCE_URL,
            quantum_computer=computer,
        )
        backend = provider.get_backend()
        qc_transpiled = transpile(qiskit_circuit, backend=backend, optimization_level=0)

        job = backend.run(qc_transpiled, shots=shots)
        result = job.result()
        counts = result.get_counts()
        return dict(counts)

    def _run_scaleway(self, arvak_circuit, shots: int, **options) -> dict[str, int]:
        """Compile with Arvak and submit to Scaleway QaaS (IQM QPU).

        Requires:
          - ``SCALEWAY_SECRET_KEY``  env var
          - ``SCALEWAY_PROJECT_ID``  env var
          - ``SCALEWAY_SESSION_ID``  env var (pre-created QaaS session)
          - ``SCALEWAY_PLATFORM``    env var (optional; default: QPU-GARNET-20PQ)

        The circuit is compiled by Arvak for the selected IQM topology and
        submitted as QASM3 to the Scaleway QaaS REST API.
        """
        import arvak
        import json

        secret_key = os.environ.get('SCALEWAY_SECRET_KEY')
        project_id = os.environ.get('SCALEWAY_PROJECT_ID')
        session_id = os.environ.get('SCALEWAY_SESSION_ID')

        if not secret_key:
            raise RuntimeError("SCALEWAY_SECRET_KEY environment variable not set.")
        if not project_id:
            raise RuntimeError("SCALEWAY_PROJECT_ID environment variable not set.")
        if not session_id:
            raise RuntimeError(
                "SCALEWAY_SESSION_ID environment variable not set.\n"
                "Create a session at console.scaleway.com > Quantum Computing > Sessions."
            )

        platform = options.get(
            'platform',
            os.environ.get('SCALEWAY_PLATFORM', 'QPU-GARNET-20PQ')
        )
        plat_info = SCALEWAY_PLATFORMS.get(platform, SCALEWAY_PLATFORMS['QPU-GARNET-20PQ'])
        num_qubits = plat_info['qubits']
        topology = plat_info['topology']

        # Compile circuit with Arvak for the target IQM topology
        if topology == 'star':
            coupling = arvak.CouplingMap.star(num_qubits)
        else:
            coupling = arvak.CouplingMap.linear(num_qubits)

        basis = arvak.BasisGates.iqm()
        compiled = arvak.compile(
            arvak_circuit,
            coupling_map=coupling,
            basis_gates=basis,
            optimization_level=options.get('optimization_level', 1),
        )

        # Export compiled circuit to QASM3 with stdgates include for Scaleway
        qasm_str = arvak.to_qasm(compiled)
        qasm_str = qasm_str.replace(
            "OPENQASM 3.0;",
            'OPENQASM 3.0;\ninclude "stdgates.inc";',
            1,
        )

        # Submit to Scaleway QaaS REST API
        try:
            import requests
        except ImportError as exc:
            raise ImportError(
                "Scaleway backend requires the 'requests' package.\n"
                "Install with: pip install requests"
            ) from exc

        headers = {
            "X-Auth-Token": secret_key,
            "Content-Type": "application/json",
        }

        # Create a job in the existing session
        body = {
            "session_id": session_id,
            "circuit": {
                "type": "QASM",
                "content": qasm_str,
            },
            "shots": shots,
        }

        resp = requests.post(
            f"{_SCALEWAY_API_URL}/jobs",
            headers=headers,
            json=body,
            timeout=60,
        )
        if not resp.ok:
            raise RuntimeError(
                f"Scaleway job submission failed ({resp.status_code}): {resp.text}"
            )

        job_data = resp.json()
        job_id = job_data.get("id")

        # Poll for completion
        import time
        timeout = options.get('timeout', 600)
        poll_interval = options.get('poll_interval', 5)
        start = time.time()

        while True:
            elapsed = time.time() - start
            if elapsed > timeout:
                raise RuntimeError(
                    f"Scaleway job {job_id} timed out after {timeout}s"
                )

            resp = requests.get(
                f"{_SCALEWAY_API_URL}/jobs/{job_id}",
                headers=headers,
                timeout=30,
            )
            if not resp.ok:
                raise RuntimeError(f"Failed to poll job status: {resp.text}")

            data = resp.json()
            status = data.get("status", "").upper()

            if status in ("COMPLETED", "SUCCEEDED", "DONE"):
                break
            elif status in ("FAILED", "ERROR", "CANCELLED"):
                raise RuntimeError(
                    f"Scaleway job {job_id} {status.lower()}: "
                    f"{data.get('error', {}).get('message', 'unknown error')}"
                )

            time.sleep(poll_interval)

        # Fetch results
        resp = requests.get(
            f"{_SCALEWAY_API_URL}/jobs/{job_id}/results",
            headers=headers,
            timeout=60,
        )
        if not resp.ok:
            raise RuntimeError(f"Failed to fetch Scaleway results: {resp.text}")

        results_data = resp.json()
        counts: dict[str, int] = {}
        for result in results_data.get("results", []):
            for bitstring, count in result.get("counts", {}).items():
                counts[bitstring] = counts.get(bitstring, 0) + count

        return counts

    def __repr__(self) -> str:
        return f"<ArvakBackendClient('{self.name}')>"


class ArvakProvider:
    """Arvak backend provider for Qrisp.

    Allows Qrisp programs to discover and use Arvak backends.

    Available backends:
      - 'sim'      : Arvak statevector simulator (always available)
      - 'iqm'      : IQM Resonance QPU (requires IQM_TOKEN + qiskit-iqm)
      - 'scaleway' : Scaleway QaaS / IQM QPU (requires SCALEWAY credentials)

    Example::

        >>> from arvak.integrations.qrisp import ArvakProvider
        >>> provider = ArvakProvider()
        >>> backend = provider.get_backend('sim')
        >>> # List all backends
        >>> all_backends = provider.backends()
    """

    def __init__(self):
        self._backends = {}

    def get_backend(self, name: str = 'sim') -> ArvakBackendClient:
        """Get a specific backend by name.

        Args:
            name: Backend name — one of 'sim', 'iqm', 'scaleway'. (default: 'sim')

        Returns:
            ArvakBackendClient instance.

        Raises:
            ValueError: If the backend name is not recognised.
        """
        available = self._available_backend_names()
        if name not in available:
            raise ValueError(
                f"Unknown backend: {name!r}. "
                f"Available backends: {', '.join(sorted(available))}"
            )
        if name not in self._backends:
            self._backends[name] = ArvakBackendClient(name)
        return self._backends[name]

    def backends(self, name: Optional[str] = None, **filters) -> list[ArvakBackendClient]:
        """Get list of available backends.

        Args:
            name: Optional filter — return only the backend with this name.
            **filters: Additional filters (currently unused).

        Returns:
            List of ArvakBackendClient instances.
        """
        # Ensure all backends are instantiated
        for backend_name in self._available_backend_names():
            if backend_name not in self._backends:
                self._backends[backend_name] = ArvakBackendClient(backend_name)

        if name:
            backend = self._backends.get(name)
            return [backend] if backend else []

        return list(self._backends.values())

    @staticmethod
    def _available_backend_names() -> list[str]:
        """Return list of all supported backend names."""
        return ['sim', 'iqm', 'scaleway']

    def __repr__(self) -> str:
        return f"<ArvakProvider(backends={self._available_backend_names()})>"
