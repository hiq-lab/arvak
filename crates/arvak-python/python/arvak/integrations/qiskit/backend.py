"""Qiskit backend provider for Arvak.

This module implements Qiskit's provider and backend interfaces, allowing
users to execute Arvak circuits through Qiskit's familiar backend.run() API.

The simulator backend calls Arvak's built-in Rust statevector simulator
directly via PyO3, returning real simulation results.

The IBM backend compiles circuits with Arvak's Rust compiler and submits
them to IBM Quantum hardware via the IBM Cloud REST API.
"""

import os
import time
import uuid
from typing import Optional, Union


# IBM Cloud API constants
_IBM_IAM_URL = "https://iam.cloud.ibm.com/identity/token"
_IBM_API_ENDPOINT = "https://quantum.cloud.ibm.com/api"
_IBM_API_VERSION = "2026-02-01"
_USER_AGENT = "arvak/1.7.2 (quantum-sdk; +https://arvak.io)"


class ArvakProvider:
    """Qiskit provider for Arvak backends.

    This provider allows users to access Arvak execution capabilities through
    Qiskit's standard provider interface.

    Example:
        >>> from arvak.integrations.qiskit import ArvakProvider
        >>> provider = ArvakProvider()
        >>> backend = provider.get_backend('sim')
        >>> job = backend.run(qiskit_circuit, shots=1000)
        >>> result = job.result()
    """

    # IBM backend names that we support
    _IBM_BACKENDS = {
        'ibm_torino', 'ibm_fez', 'ibm_marrakesh', 'ibm_brisbane',
        'ibm_kyoto', 'ibm_osaka', 'ibm_sherbrooke', 'ibm_nazca',
    }

    def __init__(self):
        """Initialize the Arvak provider."""
        self._backends = {}

    def backends(self, name: Optional[str] = None, **filters) -> list:
        """Get list of available backends.

        Args:
            name: Optional backend name filter
            **filters: Additional filters (currently unused)

        Returns:
            List of backend instances
        """
        if not self._backends:
            self._backends['sim'] = ArvakSimulatorBackend(provider=self)

        # Lazily create IBM backends on demand
        if name and name in self._IBM_BACKENDS and name not in self._backends:
            try:
                self._backends[name] = ArvakIBMBackend(provider=self, target=name)
            except (ValueError, ImportError) as e:
                # Credentials not set or requests not available
                raise ValueError(
                    f"Cannot connect to {name}: {e}. "
                    f"Set IBM_API_KEY and IBM_SERVICE_CRN environment variables."
                ) from e

        if name:
            backend = self._backends.get(name)
            return [backend] if backend else []

        return list(self._backends.values())

    def get_backend(self, name: str = 'sim'):
        """Get a specific backend by name.

        Args:
            name: Backend name ('sim', 'ibm_torino', 'ibm_marrakesh', etc.)

        Returns:
            Backend instance

        Raises:
            ValueError: If backend name is unknown or credentials missing
        """
        backends = self.backends(name=name)
        if not backends:
            available = list(self._backends.keys()) + [
                b for b in self._IBM_BACKENDS if b not in self._backends
            ]
            raise ValueError(
                f"Unknown backend: {name}. "
                f"Available backends: {', '.join(sorted(available))}"
            )
        return backends[0]

    def __repr__(self) -> str:
        return f"<ArvakProvider(backends={list(self._backends.keys())})>"


class ArvakSimulatorBackend:
    """Arvak simulator backend with Qiskit-compatible interface.

    Wraps Arvak's built-in Rust statevector simulator. Circuits are converted
    to OpenQASM 3, compiled and simulated in Rust, and results are returned
    as standard Qiskit-compatible count dictionaries.

    Supports circuits up to ~20 qubits (exact statevector simulation).
    """

    def __init__(self, provider: ArvakProvider):
        self._provider = provider
        self.name = 'arvak_simulator'
        self.description = 'Arvak Rust statevector simulator'
        self.online_date = '2024-01-01'
        self.backend_version = '1.0.0'

    @property
    def max_circuits(self) -> Optional[int]:
        return None

    @property
    def num_qubits(self) -> int:
        return 20

    @property
    def basis_gates(self) -> list[str]:
        return ['id', 'h', 'x', 'y', 'z', 's', 't', 'sx',
                'rx', 'ry', 'rz', 'cx', 'cy', 'cz', 'swap',
                'ccx', 'measure']

    @property
    def coupling_map(self) -> Optional[list[list[int]]]:
        return None  # All-to-all connectivity

    def run(self, circuits: Union['QuantumCircuit', list['QuantumCircuit']],
            shots: int = 1024, **options) -> 'ArvakJob':
        """Run circuits on Arvak's statevector simulator.

        Args:
            circuits: Single circuit or list of circuits to execute
            shots: Number of measurement shots (default: 1024)
            **options: Additional execution options

        Returns:
            ArvakJob with real simulation results
        """
        if not isinstance(circuits, list):
            circuits = [circuits]

        import arvak

        # Execute each circuit on the simulator
        all_counts = []
        for qc in circuits:
            # Convert Qiskit circuit to QASM → Arvak circuit
            try:
                from qiskit.qasm3 import dumps
                qasm_str = dumps(qc)
            except (ImportError, AttributeError):
                from qiskit.qasm2 import dumps as dumps2
                qasm_str = dumps2(qc)

            arvak_circuit = arvak.from_qasm(qasm_str)
            counts = arvak.run_sim(arvak_circuit, shots)
            all_counts.append(counts)

        return ArvakJob(
            backend=self,
            counts=all_counts,
            shots=shots
        )

    def __repr__(self) -> str:
        return f"<ArvakSimulatorBackend('{self.name}')>"


class ArvakIBMBackend:
    """Arvak IBM Quantum backend with Qiskit-compatible interface.

    Compiles circuits with Arvak's Rust compiler and submits them to
    IBM Quantum hardware via the IBM Cloud REST API.

    Requires IBM_API_KEY and IBM_SERVICE_CRN environment variables.
    """

    def __init__(self, provider: ArvakProvider, target: str = 'ibm_torino'):
        import requests as _requests
        self._requests = _requests

        self._provider = provider
        self.name = target
        self.description = f'IBM Quantum {target} (via Arvak)'
        self.backend_version = '1.0.0'

        # Read credentials from environment
        self._api_key = os.environ.get('IBM_API_KEY')
        self._service_crn = os.environ.get('IBM_SERVICE_CRN')

        if not self._api_key:
            raise ValueError("IBM_API_KEY environment variable not set")
        if not self._service_crn:
            raise ValueError("IBM_SERVICE_CRN environment variable not set")

        # IAM token (lazy — exchanged on first API call)
        self._iam_token = None
        self._iam_expiry = 0

        # Cached backend info
        self._backend_info = None
        self._info_fetched_at = 0

    def _get_token(self) -> str:
        """Exchange API key for IAM bearer token, caching until expiry."""
        now = time.time()
        if self._iam_token and now < self._iam_expiry - 60:
            return self._iam_token

        resp = self._requests.post(
            _IBM_IAM_URL,
            headers={"Content-Type": "application/x-www-form-urlencoded"},
            data=f"grant_type=urn:ibm:params:oauth:grant-type:apikey&apikey={self._api_key}",
            timeout=30,
        )
        resp.raise_for_status()
        data = resp.json()
        self._iam_token = data["access_token"]
        self._iam_expiry = now + data.get("expires_in", 3600)
        return self._iam_token

    def _headers(self) -> dict:
        """Build request headers with auth and service CRN."""
        return {
            "Authorization": f"Bearer {self._get_token()}",
            "Content-Type": "application/json",
            "Accept": "application/json",
            "Service-CRN": self._service_crn,
            "ibm-api-version": _IBM_API_VERSION,
            "User-Agent": _USER_AGENT,
        }

    def _get_backend_info(self) -> dict:
        """Fetch backend configuration and status from IBM Cloud API."""
        now = time.time()
        if self._backend_info and (now - self._info_fetched_at) < 300:
            return self._backend_info

        headers = self._headers()

        # Fetch configuration
        config_url = f"{_IBM_API_ENDPOINT}/v1/backends/{self.name}/configuration"
        resp = self._requests.get(config_url, headers=headers, timeout=30)
        resp.raise_for_status()
        config = resp.json()

        # Fetch status
        status_url = f"{_IBM_API_ENDPOINT}/v1/backends/{self.name}/status"
        resp = self._requests.get(status_url, headers=headers, timeout=30)
        status = {}
        if resp.ok:
            status = resp.json()

        self._backend_info = {
            "name": config.get("backend_name", self.name),
            "num_qubits": config.get("n_qubits", 133),
            "basis_gates": config.get("basis_gates", []),
            "coupling_map": config.get("coupling_map", []),
            "operational": status.get("state", True),
            "status_msg": status.get("status", "unknown"),
            "queue_length": status.get("length_queue", 0),
        }
        self._info_fetched_at = now
        return self._backend_info

    @property
    def num_qubits(self) -> int:
        return self._get_backend_info()["num_qubits"]

    @property
    def basis_gates(self) -> list[str]:
        return self._get_backend_info()["basis_gates"]

    @property
    def coupling_map(self) -> list[list[int]]:
        return self._get_backend_info()["coupling_map"]

    def status(self) -> str:
        info = self._get_backend_info()
        if info["operational"]:
            return f"online (queue: {info['queue_length']})"
        return f"offline: {info['status_msg']}"

    def run(self, circuits: Union['QuantumCircuit', list['QuantumCircuit']],
            shots: int = 4096, **options) -> 'ArvakIBMJob':
        """Compile and submit circuits to IBM Quantum hardware.

        Circuits are:
        1. Converted from Qiskit to Arvak's native representation
        2. Compiled with Arvak's Rust compiler for the target topology
        3. Submitted to IBM Quantum via the Cloud REST API

        Args:
            circuits: Single circuit or list of circuits to execute
            shots: Number of measurement shots (default: 4096)
            **options: Additional execution options

        Returns:
            ArvakIBMJob with job ID for polling
        """
        if not isinstance(circuits, list):
            circuits = [circuits]

        import arvak

        # Fetch real topology for compilation
        info = self._get_backend_info()
        edges = [(int(e[0]), int(e[1])) for e in info["coupling_map"]]
        coupling = arvak.CouplingMap.from_edge_list(info["num_qubits"], edges)
        basis = arvak.BasisGates.heron()

        qasm_circuits = []
        for qc in circuits:
            # Convert Qiskit → QASM → Arvak
            try:
                from qiskit.qasm3 import dumps
                qasm_str = dumps(qc)
            except (ImportError, AttributeError):
                from qiskit.qasm2 import dumps as dumps2
                qasm_str = dumps2(qc)

            arvak_circuit = arvak.from_qasm(qasm_str)

            # Compile with Arvak for the target topology
            compiled = arvak.compile(
                arvak_circuit,
                coupling_map=coupling,
                basis_gates=basis,
                optimization_level=1,
            )

            # Emit QASM3 with stdgates include for IBM
            qasm_out = arvak.to_qasm(compiled)
            qasm_out = qasm_out.replace(
                "OPENQASM 3.0;",
                'OPENQASM 3.0;\ninclude "stdgates.inc";',
                1,
            )
            qasm_circuits.append(qasm_out)

        # Submit to IBM Cloud API
        headers = self._headers()
        pubs = [[qasm, {}, shots] for qasm in qasm_circuits]
        body = {
            "program_id": "sampler",
            "backend": self.name,
            "params": {
                "version": 2,
                "pubs": pubs,
                "options": {"optimization_level": 1},
            },
        }

        resp = self._requests.post(
            f"{_IBM_API_ENDPOINT}/v1/jobs",
            headers=headers,
            json=body,
            timeout=60,
        )
        if not resp.ok:
            raise RuntimeError(f"IBM job submission failed: {resp.text}")

        job_data = resp.json()
        job_id = job_data.get("id", str(uuid.uuid4()))

        return ArvakIBMJob(
            backend=self,
            job_id=job_id,
            shots=shots,
            num_circuits=len(qasm_circuits),
        )

    def __repr__(self) -> str:
        return f"<ArvakIBMBackend('{self.name}')>"


class ArvakIBMJob:
    """Job submitted to IBM Quantum hardware.

    Polls the IBM Cloud API for status and results.
    """

    def __init__(self, backend: ArvakIBMBackend, job_id: str, shots: int, num_circuits: int):
        self._backend = backend
        self._job_id = job_id
        self._shots = shots
        self._num_circuits = num_circuits

    def job_id(self) -> str:
        return self._job_id

    def status(self) -> str:
        headers = self._backend._headers()
        resp = self._backend._requests.get(
            f"{_IBM_API_ENDPOINT}/v1/jobs/{self._job_id}",
            headers=headers,
            timeout=30,
        )
        if not resp.ok:
            return "UNKNOWN"
        return resp.json().get("status", "UNKNOWN").upper()

    def result(self, timeout: int = 600, poll_interval: int = 5) -> 'ArvakResult':
        """Wait for job completion and return results.

        Args:
            timeout: Maximum wait time in seconds (default: 600)
            poll_interval: Seconds between status checks (default: 5)

        Returns:
            ArvakResult with measurement counts

        Raises:
            RuntimeError: If job fails or times out
        """
        headers = self._backend._headers()
        start = time.time()

        while True:
            elapsed = time.time() - start
            if elapsed > timeout:
                raise RuntimeError(
                    f"Job {self._job_id} timed out after {timeout}s"
                )

            resp = self._backend._requests.get(
                f"{_IBM_API_ENDPOINT}/v1/jobs/{self._job_id}",
                headers=headers,
                timeout=30,
            )
            if not resp.ok:
                raise RuntimeError(f"Failed to check job status: {resp.text}")

            data = resp.json()
            status = data.get("status", "").upper()

            if status == "COMPLETED":
                break
            elif status in ("FAILED", "ERROR", "CANCELLED"):
                reason = ""
                if "state" in data and "reason" in (data["state"] or {}):
                    reason = data["state"]["reason"]
                elif "error" in data:
                    reason = data.get("error", {}).get("message", "")
                raise RuntimeError(
                    f"Job {self._job_id} {status.lower()}: {reason}"
                )

            # Refresh token if needed (long-running polls)
            if time.time() > self._backend._iam_expiry - 120:
                headers = self._backend._headers()

            print(f"\r  Status: {status.lower()}, elapsed: {elapsed:.0f}s", end="", flush=True)
            time.sleep(poll_interval)

        print(f"\r  Status: completed, elapsed: {time.time() - start:.0f}s")

        # Fetch results
        resp = self._backend._requests.get(
            f"{_IBM_API_ENDPOINT}/v1/jobs/{self._job_id}/results",
            headers=headers,
            timeout=60,
        )
        if not resp.ok:
            raise RuntimeError(f"Failed to fetch results: {resp.text}")

        results_data = resp.json()
        all_counts = []

        for result in results_data.get("results", []):
            counts = {}

            # V2 Sampler: raw samples in data.<register>.samples
            if "data" in result and result["data"]:
                for register_data in result["data"].values():
                    samples = register_data.get("samples", [])
                    bit_width = _infer_bit_width(samples)

                    for sample in samples:
                        binary = _hex_to_binary(sample, bit_width)
                        counts[binary] = counts.get(binary, 0) + 1

            # V1: pre-aggregated counts
            elif "counts" in result and result["counts"]:
                for bitstring, count in result["counts"].items():
                    binary = _hex_to_binary(bitstring, 0)
                    counts[binary] = count

            all_counts.append(counts)

        # Pad to expected number of circuits
        while len(all_counts) < self._num_circuits:
            all_counts.append({})

        return ArvakResult(
            backend_name=self._backend.name,
            counts=all_counts,
            shots=self._shots,
        )

    def __repr__(self) -> str:
        return f"<ArvakIBMJob(id='{self._job_id}', circuits={self._num_circuits})>"


class ArvakJob:
    """Job returned by ArvakSimulatorBackend.run().

    Contains real simulation results from the Rust statevector simulator.
    """

    def __init__(self, backend, counts, shots):
        self._backend = backend
        self._counts = counts  # list[Dict[str, int]], one per circuit
        self._shots = shots

    def result(self) -> 'ArvakResult':
        """Get job result."""
        return ArvakResult(
            backend_name=self._backend.name,
            counts=self._counts,
            shots=self._shots
        )

    def status(self) -> str:
        return "DONE"

    def __repr__(self) -> str:
        return f"<ArvakJob(circuits={len(self._counts)}, shots={self._shots})>"


class ArvakResult:
    """Result from Arvak execution.

    Contains real measurement counts from simulation or hardware.
    """

    def __init__(self, backend_name, counts, shots):
        self.backend_name = backend_name
        self._counts = counts  # list[Dict[str, int]]
        self._shots = shots

    def get_counts(self, circuit=None):
        """Get measurement counts for a circuit.

        Args:
            circuit: Circuit index (default: 0)

        Returns:
            Dictionary mapping bitstrings to counts
        """
        idx = 0 if circuit is None else circuit
        if idx >= len(self._counts):
            idx = 0
        return self._counts[idx]

    def __repr__(self) -> str:
        return f"<ArvakResult(backend='{self.backend_name}', circuits={len(self._counts)})>"


def _hex_to_binary(hex_str: str, bit_width: int) -> str:
    """Convert hex string to binary string."""
    hex_str = hex_str.removeprefix("0x")
    try:
        value = int(hex_str, 16)
    except ValueError:
        return hex_str  # Already binary

    width = bit_width if bit_width > 0 else max(len(hex_str) * 4, 1)
    return format(value, f"0{width}b")


def _infer_bit_width(samples: list[str]) -> int:
    """Infer classical register bit width from V2 hex samples."""
    max_val = 0
    for s in samples:
        hex_str = s.removeprefix("0x")
        try:
            val = int(hex_str, 16)
            if val > max_val:
                max_val = val
        except ValueError:
            continue

    if max_val == 0:
        return 1
    return max_val.bit_length()
