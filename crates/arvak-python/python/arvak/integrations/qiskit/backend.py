"""Qiskit backend provider for Arvak.

This module implements Qiskit's provider and backend interfaces, allowing
users to execute Arvak circuits through Qiskit's familiar backend.run() API.

The simulator backend calls Arvak's built-in Rust statevector simulator
directly via PyO3, returning real simulation results.

The IBM backend compiles circuits with Arvak's Rust compiler and submits
them to IBM Quantum hardware via the IBM Cloud REST API.
"""

import json
import os
import time
import uuid
from typing import Optional, Union


# ---------------------------------------------------------------------------
# HAL error hierarchy (mirrors HalError variants in arvak-hal/src/error.rs)
# ---------------------------------------------------------------------------

class ArvakError(Exception):
    """Base class for Arvak HAL errors."""


class ArvakValidationError(ArvakError):
    """Circuit validation failed before submission."""


class ArvakBackendUnavailableError(ArvakError):
    """Backend is offline or temporarily unavailable."""


class ArvakAuthenticationError(ArvakError):
    """Authentication failed (bad credentials or expired token)."""


class ArvakSubmissionError(ArvakError):
    """Job submission to the backend API failed."""


class ArvakJobError(ArvakError):
    """Job failed or was rejected on the backend."""


class ArvakJobCancelledError(ArvakJobError):
    """Job was cancelled before producing results."""


class ArvakTimeoutError(ArvakError):
    """Polling for job result timed out."""


# ---------------------------------------------------------------------------
# HAL contract data structures
# ---------------------------------------------------------------------------

from dataclasses import dataclass as _dataclass, field as _field


@_dataclass
class HalValidationResult:
    """Pre-submission circuit validation result (HAL DEBT-01 fix).

    Usage::

        result = backend.validate(circuits, shots=1024)
        result.raise_if_invalid()  # raises ArvakValidationError if invalid
    """

    valid: bool
    errors: list = _field(default_factory=list)

    def __bool__(self) -> bool:
        return self.valid

    def raise_if_invalid(self) -> None:
        """Raise ArvakValidationError with all error messages if not valid."""
        if not self.valid:
            bullet_list = "\n".join(f"  - {e}" for e in self.errors)
            raise ArvakValidationError(
                f"Circuit validation failed:\n{bullet_list}"
            )


@_dataclass
class HalAvailability:
    """Backend availability status (HAL DEBT-05 fix).

    Usage::

        avail = backend.availability()
        avail.raise_if_unavailable()  # raises ArvakBackendUnavailableError
    """

    online: bool
    queue_depth: int = 0
    estimated_wait_s: Optional[int] = None
    status_message: str = ""

    def __bool__(self) -> bool:
        return self.online

    def raise_if_unavailable(self) -> None:
        """Raise ArvakBackendUnavailableError if the backend is not online."""
        if not self.online:
            raise ArvakBackendUnavailableError(
                f"Backend unavailable: {self.status_message}"
            )


# Quantinuum API constants
_QUANTINUUM_API_BASE = "https://qapi.quantinuum.com/v1"

# IBM Cloud API constants
_IBM_IAM_URL = "https://iam.cloud.ibm.com/identity/token"
_IBM_API_ENDPOINT = "https://quantum.cloud.ibm.com/api"
_IBM_EU_API_ENDPOINT = "https://eu-de.quantum-computing.cloud.ibm.com"
_IBM_API_VERSION = "2026-02-01"
_USER_AGENT = "arvak/1.8.0 (quantum-sdk; +https://arvak.io)"

# Scaleway QaaS API constants
_SCALEWAY_API_BASE = "https://api.scaleway.com/qaas/v1alpha1"
_SCALEWAY_USER_AGENT = "arvak/1.8.0 (quantum-sdk; +https://arvak.io)"

# ---------------------------------------------------------------------------
# IQM hardware coupling maps (from calibration data, 2026-02-19)
#
# Edges are 0-indexed (QB1 → 0, QB2 → 1, ...).
# Sirius: genuine star topology, centre QB2 (idx 1).
# Garnet: heavy-hex grid, 20 qubits, 30 edges.
# Emerald: larger grid, 54 qubits, 82 edges.
# ---------------------------------------------------------------------------

_IQM_COUPLING_GARNET_20Q: list[tuple[int, int]] = [
    (0,1),(0,3),(1,4),(2,3),(2,7),(3,4),(3,8),(4,5),(4,9),
    (5,6),(5,10),(6,11),(7,8),(7,12),(8,9),(8,13),(9,10),
    (9,14),(10,11),(10,15),(11,16),(12,13),(13,14),(13,17),
    (14,15),(14,18),(15,16),(15,19),(17,18),(18,19),
]

_IQM_COUPLING_EMERALD_54Q: list[tuple[int, int]] = [
    (0,1),(0,4),(1,5),(2,3),(2,8),(3,4),(3,9),(4,5),(4,10),
    (5,6),(5,11),(7,8),(7,15),(8,9),(8,16),(9,10),(10,11),
    (10,18),(11,12),(11,19),(12,13),(12,20),(13,21),(14,15),
    (14,22),(15,16),(15,23),(16,17),(16,24),(17,18),(17,25),
    (18,19),(18,26),(19,20),(19,27),(20,21),(20,28),(21,29),
    (22,23),(23,24),(23,31),(24,25),(24,32),(25,26),(25,33),
    (26,27),(26,34),(27,28),(27,35),(28,29),(28,36),(29,30),
    (30,38),(31,32),(32,33),(32,40),(33,34),(33,41),(34,35),
    (34,42),(35,36),(35,43),(36,37),(36,44),(37,38),(39,40),
    (40,41),(40,46),(41,42),(41,47),(43,44),(43,49),(44,45),
    (44,50),(46,47),(47,48),(47,51),(48,52),(49,50),(49,53),
    (51,52),(52,53),
]


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
        # EU (Frankfurt) backends
        'ibm_brussels', 'ibm_strasbourg', 'ibm_aachen',
    }

    # Backends hosted in the EU region (use eu-de API endpoint)
    _IBM_EU_BACKENDS = {
        'ibm_brussels', 'ibm_strasbourg', 'ibm_aachen',
    }

    # Scaleway/IQM backend names
    _SCALEWAY_BACKENDS = {
        'scaleway_garnet', 'scaleway_sirius', 'scaleway_emerald',
    }

    _SCALEWAY_PLATFORM_MAP = {
        'scaleway_garnet': 'QPU-GARNET-20PQ',
        'scaleway_sirius': 'QPU-SIRIUS-24PQ',
        'scaleway_emerald': 'QPU-EMERALD-54PQ',
    }

    # IQM Resonance backend names (direct IQM API, no Scaleway)
    _IQM_RESONANCE_BACKENDS = {
        'iqm_sirius', 'iqm_garnet', 'iqm_emerald', 'iqm_crystal',
    }

    _IQM_RESONANCE_COMPUTER_MAP = {
        'iqm_sirius':   'sirius',
        'iqm_garnet':   'garnet',
        'iqm_emerald':  'emerald',
        'iqm_crystal':  'crystal',
    }

    # Quantinuum backend names (H1/H2 ion trap + emulators)
    _QUANTINUUM_BACKENDS = {
        'quantinuum_h2', 'quantinuum_h1_emulator', 'quantinuum_h2_emulator',
    }

    _QUANTINUUM_DEVICE_MAP = {
        'quantinuum_h2':          'H2-1',
        'quantinuum_h1_emulator': 'H1-1E',
        'quantinuum_h2_emulator': 'H2-1E',
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

        # Lazily create Scaleway backends on demand
        if name and name in self._SCALEWAY_BACKENDS and name not in self._backends:
            try:
                platform = self._SCALEWAY_PLATFORM_MAP[name]
                self._backends[name] = ArvakScalewayBackend(
                    provider=self, platform=platform,
                )
            except (ValueError, ImportError) as e:
                raise ValueError(
                    f"Cannot connect to {name}: {e}. "
                    f"Set SCALEWAY_SECRET_KEY, SCALEWAY_PROJECT_ID, and "
                    f"SCALEWAY_SESSION_ID environment variables."
                ) from e

        # Lazily create IQM Resonance backends on demand
        if name and name in self._IQM_RESONANCE_BACKENDS and name not in self._backends:
            try:
                computer = self._IQM_RESONANCE_COMPUTER_MAP[name]
                self._backends[name] = ArvakIQMResonanceBackend(
                    provider=self, computer=computer,
                )
            except (ValueError, ImportError) as e:
                raise ValueError(
                    f"Cannot connect to {name}: {e}. "
                    f"Set IQM_TOKEN environment variable (from resonance.meetiqm.com)."
                ) from e

        # Lazily create Quantinuum backends on demand
        if name and name in self._QUANTINUUM_BACKENDS and name not in self._backends:
            try:
                device = self._QUANTINUUM_DEVICE_MAP[name]
                self._backends[name] = ArvakQuantinuumBackend(
                    provider=self, device_name=device,
                )
            except (ValueError, ImportError) as e:
                raise ValueError(
                    f"Cannot connect to {name}: {e}. "
                    f"Set QUANTINUUM_EMAIL and QUANTINUUM_PASSWORD environment variables."
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
            ] + [
                b for b in self._SCALEWAY_BACKENDS if b not in self._backends
            ] + [
                b for b in self._IQM_RESONANCE_BACKENDS if b not in self._backends
            ] + [
                b for b in self._QUANTINUUM_BACKENDS if b not in self._backends
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

    # EU backends omit the /v1 path prefix; US backends include it (DEBT-07).
    # Backend info cache TTL in seconds (DEBT-10).
    _BACKEND_INFO_TTL: int = 300

    def __init__(self, provider: ArvakProvider, target: str = 'ibm_torino'):
        import requests as _requests
        self._requests = _requests

        self._provider = provider
        self.name = target
        self.description = f'IBM Quantum {target} (via Arvak)'
        self.backend_version = '1.0.0'

        # Resolve API endpoint and CRN based on region
        self._is_eu = target in ArvakProvider._IBM_EU_BACKENDS
        if self._is_eu:
            self._api_base = _IBM_EU_API_ENDPOINT
        else:
            self._api_base = _IBM_API_ENDPOINT

        # Read credentials from environment
        # EU backends check IBM_SERVICE_CRN_EU first, then fall back to IBM_SERVICE_CRN
        self._api_key = os.environ.get('IBM_API_KEY')
        if self._is_eu:
            self._service_crn = os.environ.get('IBM_SERVICE_CRN_EU') or os.environ.get('IBM_SERVICE_CRN')
        else:
            self._service_crn = os.environ.get('IBM_SERVICE_CRN')

        if not self._api_key:
            raise ValueError("IBM_API_KEY environment variable not set")
        if not self._service_crn:
            crn_var = "IBM_SERVICE_CRN_EU or IBM_SERVICE_CRN" if self._is_eu else "IBM_SERVICE_CRN"
            raise ValueError(f"{crn_var} environment variable not set")

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
        if resp.status_code == 401:
            raise ArvakAuthenticationError(
                "IBM IAM authentication failed — check IBM_API_KEY"
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

    def _api_url(self, path: str) -> str:
        """Build an IBM Cloud API URL with the correct version prefix.

        EU backends (Frankfurt) use no /v1 prefix; US backends require /v1.
        Centralises EU/US routing so callers never embed the if/else (DEBT-07).
        """
        prefix = "" if self._is_eu else "/v1"
        return f"{self._api_base}{prefix}/{path}"

    def contract_version(self) -> str:
        """Return the HAL contract version implemented by this backend (DEBT-12)."""
        return "2.0"

    def _get_backend_info(self) -> dict:
        """Fetch backend configuration and status from IBM Cloud API."""
        now = time.time()
        if self._backend_info and (now - self._info_fetched_at) < self._BACKEND_INFO_TTL:
            return self._backend_info

        headers = self._headers()

        config_url = self._api_url(f"backends/{self.name}/configuration")
        status_url = self._api_url(f"backends/{self.name}/status")

        resp = self._requests.get(config_url, headers=headers, timeout=30)
        resp.raise_for_status()
        config = resp.json()

        # Fetch status
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

    def availability(self) -> HalAvailability:
        """Return backend availability status (DEBT-05 fix).

        Uses the 5-minute TTL backend info cache — safe to call before run().
        """
        try:
            info = self._get_backend_info()
            return HalAvailability(
                online=bool(info["operational"]),
                queue_depth=info.get("queue_length", 0),
                status_message=info.get("status_msg", ""),
            )
        except ArvakAuthenticationError:
            raise
        except Exception as e:
            return HalAvailability(online=False, status_message=str(e))

    def validate(self, circuits, shots: int = 4096) -> HalValidationResult:
        """Validate circuits before submission (DEBT-01 fix).

        Checks shot count and qubit count against backend constraints.
        Gate-set validation is deferred to post-compile (gates are resolved
        during Arvak compilation, not before).
        """
        if not isinstance(circuits, list):
            circuits = [circuits]
        errors = []
        if shots <= 0:
            errors.append(f"shots must be > 0, got {shots}")
        if shots > 300_000:
            errors.append(f"shots {shots} exceeds IBM Quantum maximum of 300,000")
        try:
            max_qubits = self.num_qubits  # uses TTL-cached _get_backend_info()
        except Exception:
            max_qubits = 127  # conservative fallback
        for i, qc in enumerate(circuits):
            if qc.num_qubits > max_qubits:
                errors.append(
                    f"Circuit {i}: {qc.num_qubits} qubits exceeds "
                    f"{self.name} maximum {max_qubits}"
                )
        return HalValidationResult(valid=not errors, errors=errors)

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

        # HAL pre-submission checks (DEBT-01, DEBT-05)
        self.availability().raise_if_unavailable()
        self.validate(circuits, shots).raise_if_invalid()

        import arvak

        # Fetch real topology for compilation
        info = self._get_backend_info()
        edges = [(int(e[0]), int(e[1])) for e in info["coupling_map"]]
        coupling = arvak.CouplingMap.from_edge_list(info["num_qubits"], edges)

        # Always compile with Heron basis (Arvak handles CZ/RZZ natively).
        # For Eagle backends, a regex pass decomposes CZ → ECR + single-qubit.
        hw_gates = set(info.get("basis_gates", []))
        is_heron = "cz" in hw_gates or "rzz" in hw_gates
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

            # Emit QASM3
            qasm_out = arvak.to_qasm(compiled)

            if is_heron:
                # Heron: add stdgates include + rzz gate definition
                qasm_out = qasm_out.replace(
                    "OPENQASM 3.0;",
                    'OPENQASM 3.0;\ninclude "stdgates.inc";\n'
                    'gate rzz(theta) a, b { cx a, b; rz(theta) b; cx a, b; }',
                    1,
                )
            else:
                # Eagle: use Qiskit transpiler for Heron→Eagle basis conversion.
                # Arvak already routed for the Eagle coupling map; Qiskit handles:
                #   - CZ/RZZ → ECR decomposition
                #   - ECR gate directionality (coupling map constraints)
                #   - Single-qubit gate optimization (H, RX → RZ, SX, X)
                from qiskit.qasm3 import loads as _qasm3_loads, dumps as _qasm3_dumps
                from qiskit import transpile as _qiskit_transpile
                from qiskit.transpiler import CouplingMap as _QiskitCouplingMap

                # Prepare QASM for Qiskit parsing
                qasm_for_qiskit = qasm_out.replace("OPENQASM 3.0;",
                    'OPENQASM 3.0;\ninclude "stdgates.inc";\n'
                    'gate rzz(theta) a, b { cx a, b; rz(theta) b; cx a, b; }', 1)

                qc = _qasm3_loads(qasm_for_qiskit)

                # Transpile: basis conversion + directionality, preserve layout
                _qiskit_cm = _QiskitCouplingMap(couplinglist=edges)
                transpiled = _qiskit_transpile(
                    qc,
                    basis_gates=['ecr', 'rz', 'sx', 'x'],
                    coupling_map=_qiskit_cm,
                    optimization_level=1,
                    initial_layout=list(range(qc.num_qubits)),
                )

                qasm_out = _qasm3_dumps(transpiled)

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
                "options": {"optimization_level": 0},
            },
        }

        jobs_url = self._api_url("jobs")

        resp = self._requests.post(
            jobs_url,
            headers=headers,
            json=body,
            timeout=60,
        )
        if resp.status_code == 401:
            raise ArvakAuthenticationError(
                "IBM job submission rejected — token may have expired"
            )
        if not resp.ok:
            raise ArvakSubmissionError(f"IBM job submission failed: {resp.text}")

        job_data = resp.json()
        job_id = job_data.get("id", str(uuid.uuid4()))

        return ArvakIBMJob(
            backend=self,
            job_id=job_id,
            shots=shots,
            num_circuits=len(qasm_circuits),
        )

    # --- HAL contract split methods (DEBT-15) ---

    def submit(self, circuits, shots: int = 4096, **options) -> str:
        """Compile and submit circuits; return the job ID (DEBT-15).

        Equivalent to run() but returns the job ID string instead of a job
        object, enabling callers to track the job independently.
        """
        job = self.run(circuits, shots, **options)
        return job.job_id()

    def job_status(self, job_id: str) -> str:
        """Return the current status of a submitted job (DEBT-15)."""
        job = ArvakIBMJob(backend=self, job_id=job_id, shots=0, num_circuits=0)
        return job.status()

    def job_result(self, job_id: str, num_circuits: int = 1, shots: int = 4096,
                   timeout: int = 600, poll_interval: int = 5) -> 'ArvakResult':
        """Wait for and return results of a submitted job by ID (DEBT-15)."""
        job = ArvakIBMJob(backend=self, job_id=job_id,
                          shots=shots, num_circuits=num_circuits)
        return job.result(timeout=timeout, poll_interval=poll_interval)

    def job_cancel(self, job_id: str) -> bool:
        """Cancel a submitted job by ID (DEBT-15). Returns True if cancelled."""
        job = ArvakIBMJob(backend=self, job_id=job_id, shots=0, num_circuits=0)
        try:
            job.cancel()
            return True
        except (ArvakJobError, ArvakSubmissionError):
            return False

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

    def _job_url(self) -> str:
        """Build the correct jobs API URL via the backend's _api_url() helper (DEBT-07)."""
        return self._backend._api_url(f"jobs/{self._job_id}")

    def status(self) -> str:
        headers = self._backend._headers()
        resp = self._backend._requests.get(
            self._job_url(),
            headers=headers,
            timeout=30,
        )
        if not resp.ok:
            return "UNKNOWN"
        return resp.json().get("status", "UNKNOWN").upper()

    def cancel(self) -> None:
        """Cancel this job (DEBT-04 fix — HAL cancel() requirement).

        Raises:
            ArvakJobError: If the job cannot be found or is already terminal.
            ArvakSubmissionError: If the API request fails for other reasons.
        """
        headers = self._backend._headers()
        resp = self._backend._requests.delete(
            self._job_url(),
            headers=headers,
            timeout=30,
        )
        if resp.status_code == 404:
            raise ArvakJobError(f"Job {self._job_id} not found")
        if resp.status_code == 409:
            raise ArvakJobError(
                f"Job {self._job_id} cannot be cancelled (already in terminal state)"
            )
        if not resp.ok:
            raise ArvakSubmissionError(
                f"Failed to cancel job {self._job_id}: {resp.text}"
            )

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
                raise ArvakTimeoutError(
                    f"Job {self._job_id} timed out after {timeout}s"
                )

            resp = self._backend._requests.get(
                self._job_url(),
                headers=headers,
                timeout=30,
            )
            if not resp.ok:
                raise ArvakJobError(f"Failed to check job status: {resp.text}")

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
                raise ArvakJobError(
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
            f"{self._job_url()}/results",
            headers=headers,
            timeout=60,
        )
        if not resp.ok:
            raise ArvakSubmissionError(f"Failed to fetch results: {resp.text}")

        results_data = resp.json()
        all_counts = []

        for result in results_data.get("results", []):
            counts = {}

            # V2 Sampler: raw samples in data.<register>.samples
            if "data" in result and result["data"]:
                # Use result metadata as authoritative bit-width source (DEBT-09)
                num_clbits = result.get("metadata", {}).get("num_clbits", 0)
                if num_clbits == 0:
                    num_clbits = result.get("header", {}).get("memory_slots", 0)
                for register_data in result["data"].values():
                    samples = register_data.get("samples", [])
                    bit_width = _infer_bit_width(samples, num_clbits)

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


class ArvakScalewayBackend:
    """Arvak Scaleway/IQM backend with Qiskit-compatible interface.

    Compiles circuits with Arvak's Rust compiler (PRX + CZ basis, star topology)
    and submits them to IQM hardware via Scaleway's QaaS REST API.

    Requires SCALEWAY_SECRET_KEY, SCALEWAY_PROJECT_ID, and SCALEWAY_SESSION_ID
    environment variables.
    """

    _PLATFORM_QUBITS = {
        "QPU-GARNET-20PQ": 20,
        "QPU-SIRIUS-24PQ": 16,
        "QPU-EMERALD-54PQ": 54,
    }

    # Real hardware coupling maps extracted from IQM calibration data (2026-02-19).
    # Sirius uses a star topology (QPU-SIRIUS-24PQ has 16 active qubits).
    # Garnet and Emerald are heavy-hex/grid topologies — NOT stars.
    _PLATFORM_COUPLING: dict[str, list[tuple[int, int]] | None] = {
        "QPU-GARNET-20PQ": _IQM_COUPLING_GARNET_20Q,
        "QPU-SIRIUS-24PQ": None,   # star topology — built dynamically via CouplingMap.star()
        "QPU-EMERALD-54PQ": _IQM_COUPLING_EMERALD_54Q,
    }

    def __init__(self, provider: ArvakProvider,
                 platform: str = "QPU-GARNET-20PQ"):
        import requests as _requests
        self._requests = _requests

        self._provider = provider
        self._platform = platform
        self._num_qubits = self._PLATFORM_QUBITS.get(platform, 20)
        self.name = f"scaleway_{platform.split('-')[1].lower()}"
        self.description = f"IQM {platform} via Scaleway (compiled by Arvak)"
        self.backend_version = "1.0.0"

        self._secret_key = os.environ.get("SCALEWAY_SECRET_KEY")
        self._project_id = os.environ.get("SCALEWAY_PROJECT_ID")
        self._session_id = os.environ.get("SCALEWAY_SESSION_ID")

        if not self._secret_key:
            raise ValueError("SCALEWAY_SECRET_KEY environment variable not set")
        if not self._project_id:
            raise ValueError("SCALEWAY_PROJECT_ID environment variable not set")
        if not self._session_id:
            raise ValueError("SCALEWAY_SESSION_ID environment variable not set")

    def _headers(self) -> dict:
        return {
            "X-Auth-Token": self._secret_key,
            "Content-Type": "application/json",
            "User-Agent": _SCALEWAY_USER_AGENT,
        }

    @property
    def num_qubits(self) -> int:
        return self._num_qubits

    @property
    def basis_gates(self) -> list[str]:
        return ["prx", "cz"]

    @property
    def coupling_map(self) -> list[list[int]]:
        edges = self._PLATFORM_COUPLING.get(self._platform)
        if edges is not None:
            return [list(e) for e in edges]
        # Sirius: star topology, qubit 0 connects to all others
        return [[0, i] for i in range(1, self._num_qubits)]

    def status(self) -> str:
        resp = self._requests.get(
            f"{_SCALEWAY_API_BASE}/sessions/{self._session_id}",
            headers=self._headers(),
            timeout=30,
        )
        if not resp.ok:
            return f"unknown (HTTP {resp.status_code})"
        state = resp.json().get("status", "unknown")
        return state

    def availability(self) -> HalAvailability:
        """Return session/backend availability (DEBT-05 fix)."""
        try:
            resp = self._requests.get(
                f"{_SCALEWAY_API_BASE}/sessions/{self._session_id}",
                headers=self._headers(),
                timeout=30,
            )
            if not resp.ok:
                return HalAvailability(
                    online=False,
                    status_message=f"HTTP {resp.status_code}: {resp.text[:200]}",
                )
            session = resp.json()
            st = session.get("status", "unknown")
            online = st in ("running", "started", "ready")
            return HalAvailability(online=online, status_message=st)
        except Exception as e:
            return HalAvailability(online=False, status_message=str(e))

    def contract_version(self) -> str:
        """Return the HAL contract version implemented by this backend (DEBT-12)."""
        return "2.0"

    def validate(self, circuits, shots: int = 1024) -> HalValidationResult:
        """Validate circuits before submission (DEBT-01 fix)."""
        if not isinstance(circuits, list):
            circuits = [circuits]
        errors = []
        if shots <= 0:
            errors.append(f"shots must be > 0, got {shots}")
        if shots > 100_000:
            errors.append(f"shots {shots} exceeds IQM maximum of 100,000")
        for i, qc in enumerate(circuits):
            if qc.num_qubits > self._num_qubits:
                errors.append(
                    f"Circuit {i}: {qc.num_qubits} qubits exceeds "
                    f"{self.name} maximum {self._num_qubits}"
                )
        return HalValidationResult(valid=not errors, errors=errors)

    def run(self, circuits: Union['QuantumCircuit', list['QuantumCircuit']],
            shots: int = 4096, **options) -> 'ArvakScalewayJob':
        """Compile and submit circuits to IQM hardware via Scaleway.

        Circuits are:
        1. Converted from Qiskit to Arvak IR
        2. Compiled with Arvak for IQM (PRX + CZ basis, real hardware topology)
        3. Compressed and submitted to Scaleway's QaaS API

        Args:
            circuits: Single circuit or list of circuits to execute
            shots: Number of measurement shots (default: 4096)

        Returns:
            ArvakScalewayJob for polling results
        """
        if not isinstance(circuits, list):
            circuits = [circuits]

        # HAL pre-submission checks (DEBT-01, DEBT-05)
        self.availability().raise_if_unavailable()
        self.validate(circuits, shots).raise_if_invalid()

        import arvak

        # Use real hardware coupling map; fall back to star for Sirius
        edges = self._PLATFORM_COUPLING.get(self._platform)
        if edges is not None:
            coupling = arvak.CouplingMap.from_edge_list(self._num_qubits, edges)
        else:
            coupling = arvak.CouplingMap.star(self._num_qubits)
        basis = arvak.BasisGates.iqm()

        qasm_circuits = []
        for qc in circuits:
            try:
                from qiskit.qasm3 import dumps
                qasm_str = dumps(qc)
            except (ImportError, AttributeError):
                from qiskit.qasm2 import dumps as dumps2
                qasm_str = dumps2(qc)

            arvak_circuit = arvak.from_qasm(qasm_str)

            compiled = arvak.compile(
                arvak_circuit,
                coupling_map=coupling,
                basis_gates=basis,
                optimization_level=1,
            )

            qasm_out = arvak.to_qasm(compiled)
            qasm_out = _iqm_postprocess_qasm(qasm_out)

            qasm_circuits.append(qasm_out)

        headers = self._headers()

        # IQM gate-based QPUs use a two-step flow:
        #   1. POST /models  — upload circuit as QuantumComputationModel → get model_id
        #   2. POST /jobs    — create job referencing model_id
        #
        # The model payload matches the qio QuantumComputationModel schema used by
        # qiskit-scaleway: programs[] with QASM3 + ZLIB_BASE64_V1 compression.
        job_ids = []
        for qasm_out in qasm_circuits:
            # Step 1: create model
            # compression_format 0 = no compression (raw QASM3 string).
            # compression_format 1 (ZLIB_BASE64_V1) causes "bad argument type" on IQM.
            model_payload = json.dumps({
                "client": {"user_agent": _SCALEWAY_USER_AGENT},
                "backend": {
                    "name": self._platform,
                    "version": "1.0",
                    "options": {},
                },
                "programs": [{
                    "serialization_format": 3,   # QASM_V3
                    "compression_format": 0,     # no compression — raw QASM3 string
                    "serialization": qasm_out,
                }],
                "noise_model": None,
            })
            model_resp = self._requests.post(
                f"{_SCALEWAY_API_BASE}/models",
                headers=headers,
                json={
                    "project_id": self._project_id,
                    "payload": model_payload,
                },
                timeout=60,
            )
            if not model_resp.ok:
                raise ArvakSubmissionError(
                    f"Scaleway model creation failed ({model_resp.status_code}): {model_resp.text}"
                )
            model_id = model_resp.json()["id"]

            # Step 2: create job referencing the model
            job_name = f"arvak-{uuid.uuid4().hex[:8]}"
            resp = self._requests.post(
                f"{_SCALEWAY_API_BASE}/jobs",
                headers=headers,
                json={
                    "name": job_name,
                    "session_id": self._session_id,
                    "model_id": model_id,
                    "parameters": json.dumps({"shots": shots, "options": {"memory": True}}),
                    "tags": ["arvak"],
                },
                timeout=60,
            )
            if not resp.ok:
                raise ArvakSubmissionError(
                    f"Scaleway job submission failed ({resp.status_code}): {resp.text}"
                )
            job_ids.append(resp.json()["id"])

        return ArvakScalewayJob(
            backend=self,
            job_id=job_ids[0],
            all_job_ids=job_ids,
            shots=shots,
            num_circuits=len(qasm_circuits),
        )

    # --- HAL contract split methods (DEBT-15) ---

    def submit(self, circuits, shots: int = 4096, **options) -> str:
        """Compile and submit circuits; return the primary job ID (DEBT-15)."""
        job = self.run(circuits, shots, **options)
        return job.job_id()

    def job_status(self, job_id: str) -> str:
        """Return the current status of a submitted job by ID (DEBT-15)."""
        job = ArvakScalewayJob(backend=self, job_id=job_id,
                               all_job_ids=[job_id], shots=0, num_circuits=1)
        return job.status()

    def job_result(self, job_id: str, num_circuits: int = 1, shots: int = 4096,
                   timeout: int = 600, poll_interval: int = 5) -> 'ArvakResult':
        """Wait for and return results of a submitted job by ID (DEBT-15)."""
        job = ArvakScalewayJob(backend=self, job_id=job_id,
                               all_job_ids=[job_id], shots=shots,
                               num_circuits=num_circuits)
        return job.result(timeout=timeout, poll_interval=poll_interval)

    def job_cancel(self, job_id: str) -> bool:
        """Cancel a submitted job by ID (DEBT-15). Returns True if cancelled."""
        job = ArvakScalewayJob(backend=self, job_id=job_id,
                               all_job_ids=[job_id], shots=0, num_circuits=1)
        try:
            job.cancel()
            return True
        except (ArvakSubmissionError, Exception):
            return False

    def __repr__(self) -> str:
        return f"<ArvakScalewayBackend('{self.name}', platform='{self._platform}')>"


class ArvakIQMResonanceBackend:
    """Arvak IQM Resonance backend — submits directly to IQM's cloud API.

    Uses qiskit-on-iqm (iqm-client[qiskit]) for transpilation and submission.
    Arvak provides qubit routing and gate compilation (PRX + CZ basis);
    iqm-client handles serialisation to IQM's native JSON format and auth.

    Requires IQM_TOKEN environment variable (from resonance.meetiqm.com).
    """

    _COMPUTER_QUBITS = {
        "sirius":  16,
        "garnet":  20,
        "emerald": 54,
        "crystal": 54,
    }

    def __init__(self, provider: ArvakProvider, computer: str = "sirius"):
        try:
            from iqm.qiskit_iqm import IQMProvider as _IQMProvider
        except ImportError as exc:
            raise ImportError(
                "iqm-client[qiskit] is required for IQM Resonance backends. "
                "Install with: pip install 'iqm-client[qiskit]==33.0.3'"
            ) from exc

        self._provider = provider
        self._computer = computer
        self._num_qubits = self._COMPUTER_QUBITS.get(computer, 20)
        self.name = f"iqm_{computer}"
        self.description = f"IQM {computer.capitalize()} via IQM Resonance (compiled by Arvak)"
        self.backend_version = "1.0.0"

        token = os.environ.get("IQM_TOKEN")
        if not token:
            raise ValueError(
                "IQM_TOKEN environment variable not set. "
                "Get your token from https://resonance.meetiqm.com (account drawer)."
            )

        # IQM_TOKEN env var is read automatically by iqm-client — do not pass it again
        self._iqm_provider = _IQMProvider(
            "https://resonance.meetiqm.com/",
            quantum_computer=computer,
        )
        self._iqm_backend = self._iqm_provider.get_backend()

    @property
    def num_qubits(self) -> int:
        return self._iqm_backend.num_qubits

    @property
    def basis_gates(self) -> list[str]:
        return ["prx", "cz", "measure"]

    @property
    def coupling_map(self) -> Optional[list[list[int]]]:
        cm = self._iqm_backend.coupling_map
        if cm is not None:
            return list(cm.get_edges())
        return None

    def contract_version(self) -> str:
        """Return the HAL contract version implemented by this backend (DEBT-12)."""
        return "2.0"

    def availability(self) -> HalAvailability:
        """Return IQM Resonance backend availability (DEBT-05 fix).

        Queries the iqm-client backend status when available. Falls back to
        assuming online if the iqm-client does not expose a status endpoint
        (connection was already established at __init__).
        """
        try:
            status_fn = getattr(self._iqm_backend, 'status', None)
            if callable(status_fn):
                s = status_fn()
                operational = getattr(s, 'operational', True)
                msg = str(getattr(s, 'status', s))
                return HalAvailability(online=bool(operational), status_message=msg)
            # iqm-client Resonance backends don't expose a status endpoint;
            # assume online (auth succeeded at __init__).
            return HalAvailability(online=True, status_message="online")
        except Exception as e:
            return HalAvailability(online=False, status_message=str(e))

    def validate(self, circuits, shots: int = 1024) -> HalValidationResult:
        """Validate circuits before submission to IQM Resonance (DEBT-01 fix).

        Checks shot count and qubit count against IQM Resonance constraints.
        Gate-set validation happens after Arvak compilation inside run().
        """
        if not isinstance(circuits, list):
            circuits = [circuits]
        errors = []
        if shots <= 0:
            errors.append(f"shots must be > 0, got {shots}")
        if shots > 100_000:
            errors.append(f"shots {shots} exceeds IQM Resonance maximum of 100,000")
        max_qubits = self.num_qubits  # live from iqm_backend
        for i, qc in enumerate(circuits):
            if qc.num_qubits > max_qubits:
                errors.append(
                    f"Circuit {i}: {qc.num_qubits} qubits exceeds "
                    f"{self.name} maximum {max_qubits}"
                )
        return HalValidationResult(valid=not errors, errors=errors)

    def run(self, circuits: Union['QuantumCircuit', list['QuantumCircuit']],
            shots: int = 1024, **options) -> 'ArvakIQMResonanceJob':
        """Compile and submit circuits to IQM Resonance.

        IQM Resonance presents a virtual fully-connected coupling map to Qiskit
        and handles physical qubit routing internally.  Arvak's role here is
        gate synthesis and single-qubit gate optimisation (2-PRX H decomposition
        instead of 3 gates), NOT qubit routing.  qiskit-on-iqm's transpiler
        performs the final routing to IQM's physical topology.

        Pipeline:
        1. Qiskit circuit → Arvak IR → compile to PRX+CZ basis (fully-connected)
        2. Arvak QASM3 → Qiskit circuit (direct prx→RGate, no 3× expansion)
        3. qiskit-on-iqm transpile → routes to IQM's physical qubit topology
        4. Submit via iqm-client

        Args:
            circuits: Single circuit or list of circuits
            shots: Number of measurement shots (default: 1024)

        Returns:
            ArvakIQMResonanceJob wrapping the iqm-client job
        """
        if not isinstance(circuits, list):
            circuits = [circuits]

        # HAL pre-submission checks (DEBT-01, DEBT-05)
        self.availability().raise_if_unavailable()
        self.validate(circuits, shots).raise_if_invalid()

        import arvak
        from qiskit import transpile as _transpile

        # IQM Resonance exposes a virtual fully-connected topology to Qiskit;
        # actual routing to physical qubits is done by qiskit-on-iqm at step 3.
        n_qubits = self._iqm_backend.num_qubits
        coupling = arvak.CouplingMap.full(n_qubits)
        basis = arvak.BasisGates.iqm()

        compiled_circuits = []
        for qc in circuits:
            # Step 1: Qiskit → Arvak → compile (PRX+CZ gate synthesis + 1Q optimisation)
            try:
                from qiskit.qasm3 import dumps as _dumps3
                qasm_str = _dumps3(qc)
            except (ImportError, AttributeError):
                from qiskit.qasm2 import dumps as _dumps2
                qasm_str = _dumps2(qc)

            arvak_circuit = arvak.from_qasm(qasm_str)
            compiled = arvak.compile(
                arvak_circuit,
                coupling_map=coupling,
                basis_gates=basis,
                optimization_level=1,
            )
            qasm_out = arvak.to_qasm(compiled)

            # Step 2: Arvak QASM3 (prx+cz) → Qiskit circuit
            # Direct prx→RGate mapping; avoids 3× gate-definition expansion
            qc_arvak = _arvak_qasm_to_iqm_circuit(qasm_out)

            # Step 3: qiskit-on-iqm routes to IQM's physical qubit layout.
            # optimization_level=1 uses SABRE layout to pick a valid qubit pair.
            qc_final = _transpile(qc_arvak, backend=self._iqm_backend, optimization_level=1)
            compiled_circuits.append(qc_final)

        # Submit all circuits in a batch via iqm-client
        job = self._iqm_backend.run(compiled_circuits, shots=shots)
        return ArvakIQMResonanceJob(backend=self, iqm_job=job,
                                    shots=shots, num_circuits=len(compiled_circuits))

    # --- HAL contract split methods (DEBT-15) ---

    def submit(self, circuits, shots: int = 1024, **options) -> str:
        """Compile and submit circuits; return the job ID (DEBT-15)."""
        job = self.run(circuits, shots, **options)
        return job.job_id()

    def job_status(self, job_id: str) -> str:
        """Return the current status of a submitted IQM job by ID (DEBT-15)."""
        # Delegate to the underlying iqm-client job handle via a thin wrapper.
        # iqm-client does not reconstruct job handles from IDs in the public API;
        # use the backend's native job_id lookup if available.
        try:
            retrieve_fn = getattr(self._iqm_backend, 'retrieve_job', None)
            if callable(retrieve_fn):
                iqm_job = retrieve_fn(job_id)
                return str(iqm_job.status())
        except Exception:
            pass
        return "UNKNOWN"

    def job_result(self, job_id: str, num_circuits: int = 1, shots: int = 1024,
                   timeout: int = 600, poll_interval: int = 5) -> 'ArvakResult':
        """Wait for and return results of a submitted IQM job by ID (DEBT-15)."""
        retrieve_fn = getattr(self._iqm_backend, 'retrieve_job', None)
        if not callable(retrieve_fn):
            raise ArvakJobError(
                f"IQM backend does not support job retrieval by ID: {job_id}"
            )
        iqm_job = retrieve_fn(job_id)
        wrapper = ArvakIQMResonanceJob(backend=self, iqm_job=iqm_job,
                                       shots=shots, num_circuits=num_circuits)
        return wrapper.result(timeout=timeout, poll_interval=poll_interval)

    def job_cancel(self, job_id: str) -> bool:
        """Cancel a submitted IQM job by ID (DEBT-15). Returns True if cancelled."""
        try:
            retrieve_fn = getattr(self._iqm_backend, 'retrieve_job', None)
            if callable(retrieve_fn):
                iqm_job = retrieve_fn(job_id)
                wrapper = ArvakIQMResonanceJob(backend=self, iqm_job=iqm_job,
                                               shots=0, num_circuits=0)
                wrapper.cancel()
                return True
        except (ArvakJobError, Exception):
            pass
        return False

    def __repr__(self) -> str:
        return f"<ArvakIQMResonanceBackend('{self.name}', computer='{self._computer}')>"


class ArvakIQMResonanceJob:
    """Job submitted to IQM Resonance via iqm-client."""

    def __init__(self, backend: ArvakIQMResonanceBackend, iqm_job,
                 shots: int, num_circuits: int):
        self._backend = backend
        self._iqm_job = iqm_job
        self._shots = shots
        self._num_circuits = num_circuits

    def job_id(self) -> str:
        return self._iqm_job.job_id()

    def status(self) -> str:
        return str(self._iqm_job.status())

    def cancel(self) -> None:
        """Cancel this IQM job (DEBT-04 fix — HAL cancel() requirement).

        Delegates to the underlying iqm-client job's cancel() method.

        Raises:
            ArvakJobError: If cancellation fails or the job is already terminal.
        """
        try:
            self._iqm_job.cancel()
        except Exception as e:
            raise ArvakJobError(
                f"Failed to cancel IQM job {self.job_id()}: {e}"
            ) from e

    def result(self, timeout: int = 600, poll_interval: int = 5) -> 'ArvakResult':
        """Wait for job completion and return results."""
        iqm_result = self._iqm_job.result(timeout=timeout)
        all_counts = []
        for i in range(self._num_circuits):
            try:
                counts = iqm_result.get_counts(i)
            except Exception:
                counts = iqm_result.get_counts() if i == 0 else {}
            all_counts.append(counts)
        return ArvakResult(
            backend_name=self._backend.name,
            counts=all_counts,
            shots=self._shots,
        )

    def __repr__(self) -> str:
        return f"<ArvakIQMResonanceJob(id='{self.job_id()}', circuits={self._num_circuits})>"


class ArvakScalewayJob:
    """Job submitted to IQM hardware via Scaleway QaaS.

    For multi-circuit submissions, each circuit is a separate Scaleway job.
    This wrapper polls all job IDs and aggregates results.
    """

    def __init__(self, backend: ArvakScalewayBackend, job_id: str,
                 all_job_ids: list, shots: int, num_circuits: int):
        self._backend = backend
        self._job_id = job_id          # primary job ID (first circuit)
        self._all_job_ids = all_job_ids
        self._shots = shots
        self._num_circuits = num_circuits

    def job_id(self) -> str:
        return self._job_id

    def status(self) -> str:
        resp = self._backend._requests.get(
            f"{_SCALEWAY_API_BASE}/jobs/{self._job_id}",
            headers=self._backend._headers(),
            timeout=30,
        )
        if not resp.ok:
            return "UNKNOWN"
        return resp.json().get("status", "unknown")

    def cancel(self) -> None:
        """Cancel all submitted jobs (DEBT-04 fix — HAL cancel() requirement).

        Attempts per-job cancellation first. Falls back to terminating the
        session if the Scaleway API does not expose a per-job cancel endpoint.

        Raises:
            ArvakSubmissionError: If neither cancellation path succeeds.
        """
        headers = self._backend._headers()
        cancelled = 0
        for job_id in self._all_job_ids:
            resp = self._backend._requests.post(
                f"{_SCALEWAY_API_BASE}/jobs/{job_id}/cancel",
                headers=headers,
                timeout=30,
            )
            if resp.ok:
                cancelled += 1
        if cancelled == len(self._all_job_ids):
            return
        # Fallback: terminate the session (cancels all pending jobs in it)
        resp = self._backend._requests.post(
            f"{_SCALEWAY_API_BASE}/sessions/{self._backend._session_id}/terminate",
            headers=headers,
            timeout=30,
        )
        if not resp.ok:
            raise ArvakSubmissionError(
                f"Could not cancel jobs or terminate session: {resp.text}"
            )

    def _poll_one(self, job_id: str, timeout: int,
                  poll_interval: int, start: float) -> dict:
        """Poll a single job ID until complete; return its counts."""
        while True:
            elapsed = time.time() - start
            if elapsed > timeout:
                raise ArvakTimeoutError(f"Job {job_id} timed out after {timeout}s")

            resp = self._backend._requests.get(
                f"{_SCALEWAY_API_BASE}/jobs/{job_id}",
                headers=self._backend._headers(),
                timeout=30,
            )
            if not resp.ok:
                raise RuntimeError(f"Failed to check job status: {resp.text}")

            data = resp.json()
            status = data.get("status", "unknown")

            if status == "completed":
                break
            elif status in ("failed", "error", "cancelled"):
                raise ArvakJobError(
                    f"Job {job_id} {status}: {data.get('error_message', 'no details')}"
                )

            print(f"\r  [{job_id[:8]}] Status: {status}, elapsed: {elapsed:.0f}s",
                  end="", flush=True)
            time.sleep(poll_interval)

        print(f"\r  [{job_id[:8]}] completed, elapsed: {time.time() - start:.0f}s   ")

        # Results come from the /results endpoint as a ZLIB+Base64 encoded JSON blob.
        # The job's result_distribution field is typically null for IQM QPUs.
        resp = self._backend._requests.get(
            f"{_SCALEWAY_API_BASE}/jobs/{job_id}/results",
            headers=self._backend._headers(),
            timeout=60,
        )
        if not resp.ok:
            return {}

        job_results = resp.json().get("job_results", [])
        if not job_results:
            return {}

        counts = {}
        for jr in job_results:
            result_str = jr.get("result", "")
            if not result_str:
                continue
            try:
                result_envelope = json.loads(result_str)
            except (json.JSONDecodeError, ValueError):
                continue

            # Decode payload: compression_format 2 = ZLIB+Base64 (IQM server format)
            serialization = result_envelope.get("serialization", "")
            if result_envelope.get("compression_format", 0) in (1, 2) and serialization:
                try:
                    import zlib as _zlib_local
                    import base64 as _b64_local
                    payload = json.loads(
                        _zlib_local.decompress(_b64_local.b64decode(serialization)).decode()
                    )
                except Exception:
                    continue
            else:
                try:
                    payload = json.loads(serialization) if serialization else result_envelope
                except (json.JSONDecodeError, ValueError):
                    payload = result_envelope

            # Parse results[].data.counts (hex-encoded bitstrings → binary)
            for circuit_result in payload.get("results", [payload]):
                hex_counts = circuit_result.get("data", {}).get("counts", {})
                n_qubits = circuit_result.get("header", {}).get("n_qubits", 1)
                for hex_bits, count in hex_counts.items():
                    binary = _hex_to_binary(hex_bits, n_qubits)
                    counts[binary] = counts.get(binary, 0) + int(count)

        return counts

    def result(self, timeout: int = 600, poll_interval: int = 5) -> 'ArvakResult':
        """Wait for all submitted jobs to complete and return results.

        Args:
            timeout: Maximum total wait time in seconds (default: 600)
            poll_interval: Seconds between status checks (default: 5)

        Returns:
            ArvakResult with measurement counts

        Raises:
            RuntimeError: If any job fails or times out
        """
        start = time.time()
        all_counts = []

        for job_id in self._all_job_ids:
            counts = self._poll_one(job_id, timeout, poll_interval, start)
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
        return f"<ArvakScalewayJob(id='{self._job_id}', circuits={self._num_circuits})>"


class ArvakQuantinuumBackend:
    """Arvak Quantinuum backend — submits directly to Quantinuum's REST API.

    Accepts Qiskit circuits, converts them to QASM 2.0 (using qiskit.qasm2),
    and submits to Quantinuum's cloud API.  The Quantinuum cloud compiles
    circuits to its native ion-trap gate set (ZZMax/ZZPhase/U1q/Rz) internally.

    Requires QUANTINUUM_EMAIL and QUANTINUUM_PASSWORD environment variables.
    Uses the noiseless H2 emulator (H2-1LE) by default — free to run.
    """

    # Map of supported machine names to their qubit counts.
    _MACHINE_QUBITS = {
        'H2-1LE': 32,
        'H2-1E': 32,
        'H2-1': 32,
        'H1-1E': 20,
        'H1-1': 20,
    }

    # Backend info cache TTL in seconds.
    _MACHINE_INFO_TTL: int = 300

    def __init__(self, provider: ArvakProvider, device_name: str = 'H2-1LE'):
        import requests as _requests
        self._requests = _requests

        self._provider = provider
        self._device = device_name
        self._num_qubits = self._MACHINE_QUBITS.get(device_name, 32)
        self.name = f"quantinuum_{device_name.lower().replace('-', '_')}"
        self.description = f"Quantinuum {device_name} (via Arvak)"
        self.backend_version = "1.0.0"

        self._email = os.environ.get('QUANTINUUM_EMAIL')
        self._password = os.environ.get('QUANTINUUM_PASSWORD')

        if not self._email:
            raise ValueError("QUANTINUUM_EMAIL environment variable not set")
        if not self._password:
            raise ValueError("QUANTINUUM_PASSWORD environment variable not set")

        # JWT id-token (lazy — fetched on first API call)
        self._id_token: Optional[str] = None

        # Cached machine info
        self._machine_info: Optional[dict] = None
        self._info_fetched_at: float = 0.0

    def _login(self) -> str:
        """Exchange email + password for a JWT id-token."""
        resp = self._requests.post(
            f"{_QUANTINUUM_API_BASE}/login",
            json={"email": self._email, "password": self._password},
            timeout=30,
        )
        if resp.status_code == 401:
            raise ArvakAuthenticationError(
                "Quantinuum authentication failed — check QUANTINUUM_EMAIL and QUANTINUUM_PASSWORD"
            )
        resp.raise_for_status()
        data = resp.json()
        self._id_token = data["id-token"]
        return self._id_token

    def _get_token(self) -> str:
        """Return the cached JWT, logging in if needed."""
        if not self._id_token:
            self._login()
        return self._id_token  # type: ignore[return-value]

    def _headers(self) -> dict:
        """Build request headers with the Quantinuum JWT.

        Quantinuum uses ``Authorization: <id-token>`` (no "Bearer" prefix).
        """
        return {
            "Authorization": self._get_token(),
            "Content-Type": "application/json",
        }

    def _get_machine_info(self) -> dict:
        """Fetch machine configuration and status, using a 5-minute cache."""
        now = time.time()
        if self._machine_info and (now - self._info_fetched_at) < self._MACHINE_INFO_TTL:
            return self._machine_info

        resp = self._requests.get(
            f"{_QUANTINUUM_API_BASE}/machine/{self._device}",
            headers=self._headers(),
            timeout=30,
        )
        if resp.status_code == 401:
            self._id_token = None
            resp = self._requests.get(
                f"{_QUANTINUUM_API_BASE}/machine/{self._device}",
                headers=self._headers(),
                timeout=30,
            )
        if not resp.ok:
            return {"name": self._device, "status": "unknown", "n_qubits": self._num_qubits}

        info = resp.json()
        self._machine_info = info
        self._info_fetched_at = now
        return info

    @property
    def num_qubits(self) -> int:
        return self._num_qubits

    @property
    def basis_gates(self) -> list[str]:
        return ["rz", "rx", "ry", "h", "x", "y", "z", "s", "t", "cx", "cz", "swap", "ccx"]

    @property
    def coupling_map(self) -> Optional[list[list[int]]]:
        return None  # All-to-all connectivity

    def contract_version(self) -> str:
        """Return the HAL contract version implemented by this backend."""
        return "2.0"

    def availability(self) -> HalAvailability:
        """Return backend availability (HAL DEBT-05 fix)."""
        try:
            info = self._get_machine_info()
            status = info.get("status", "unknown")
            online = status.lower() in ("online", "available", "ready")
            return HalAvailability(online=online, status_message=status)
        except ArvakAuthenticationError:
            raise
        except Exception as e:
            return HalAvailability(online=False, status_message=str(e))

    def validate(self, circuits, shots: int = 1024) -> HalValidationResult:
        """Validate circuits before submission (HAL DEBT-01 fix)."""
        if not isinstance(circuits, list):
            circuits = [circuits]
        errors = []
        if shots <= 0:
            errors.append(f"shots must be > 0, got {shots}")
        if shots > 10_000:
            errors.append(f"shots {shots} exceeds Quantinuum maximum of 10,000")
        for i, qc in enumerate(circuits):
            if qc.num_qubits > self._num_qubits:
                errors.append(
                    f"Circuit {i}: {qc.num_qubits} qubits exceeds "
                    f"{self._device} maximum {self._num_qubits}"
                )
        return HalValidationResult(valid=not errors, errors=errors)

    def run(self, circuits: Union['QuantumCircuit', list['QuantumCircuit']],
            shots: int = 1024, **options) -> 'ArvakQuantinuumJob':
        """Submit circuits to Quantinuum hardware/emulator.

        Converts Qiskit circuits to QASM 2.0 and submits via the Quantinuum
        REST API.  The Quantinuum cloud handles native gate compilation.

        Args:
            circuits: Single circuit or list of circuits to execute
            shots: Number of measurement shots (default: 1024; max: 10,000)
            **options: Additional options passed to Quantinuum (e.g. no_opt=True)

        Returns:
            ArvakQuantinuumJob for polling results
        """
        if not isinstance(circuits, list):
            circuits = [circuits]

        # HAL pre-submission checks
        self.availability().raise_if_unavailable()
        self.validate(circuits, shots).raise_if_invalid()

        job_ids = []
        for qc in circuits:
            # Convert Qiskit circuit to QASM 2.0.
            from qiskit.qasm2 import dumps as _qasm2_dumps
            qasm_str = _qasm2_dumps(qc)

            # Build job request body.
            body: dict = {
                "name": f"arvak-{uuid.uuid4().hex[:8]}",
                "count": shots,
                "machine": self._device,
                "language": "OPENQASM 2.0",
                "program": qasm_str,
            }

            # Forward any caller-provided options (e.g. {"no-opt": True}).
            if options:
                body["options"] = {k.replace("_", "-"): v for k, v in options.items()}

            headers = self._headers()
            resp = self._requests.post(
                f"{_QUANTINUUM_API_BASE}/job",
                headers=headers,
                json=body,
                timeout=60,
            )
            if resp.status_code == 401:
                # Token expired — re-auth once and retry.
                self._id_token = None
                headers = self._headers()
                resp = self._requests.post(
                    f"{_QUANTINUUM_API_BASE}/job",
                    headers=headers,
                    json=body,
                    timeout=60,
                )
            if resp.status_code == 401:
                raise ArvakAuthenticationError(
                    "Quantinuum job submission rejected — check credentials"
                )
            if not resp.ok:
                raise ArvakSubmissionError(
                    f"Quantinuum job submission failed ({resp.status_code}): {resp.text}"
                )
            job_ids.append(resp.json()["job"])

        return ArvakQuantinuumJob(
            backend=self,
            job_id=job_ids[0],
            all_job_ids=job_ids,
            shots=shots,
            num_circuits=len(circuits),
        )

    # --- HAL contract split methods ---

    def submit(self, circuits, shots: int = 1024, **options) -> str:
        """Submit circuits and return the primary job ID."""
        job = self.run(circuits, shots, **options)
        return job.job_id()

    def job_status(self, job_id: str) -> str:
        """Return the current status of a submitted job."""
        job = ArvakQuantinuumJob(
            backend=self, job_id=job_id, all_job_ids=[job_id], shots=0, num_circuits=1
        )
        return job.status()

    def job_result(self, job_id: str, num_circuits: int = 1, shots: int = 1024,
                   timeout: int = 600, poll_interval: int = 5) -> 'ArvakResult':
        """Wait for and return results of a submitted job."""
        job = ArvakQuantinuumJob(
            backend=self, job_id=job_id, all_job_ids=[job_id],
            shots=shots, num_circuits=num_circuits,
        )
        return job.result(timeout=timeout, poll_interval=poll_interval)

    def job_cancel(self, job_id: str) -> bool:
        """Cancel a submitted job. Returns True if successful."""
        job = ArvakQuantinuumJob(
            backend=self, job_id=job_id, all_job_ids=[job_id], shots=0, num_circuits=1
        )
        try:
            job.cancel()
            return True
        except (ArvakJobError, ArvakSubmissionError):
            return False

    def __repr__(self) -> str:
        return f"<ArvakQuantinuumBackend('{self._device}')>"


class ArvakQuantinuumJob:
    """Job submitted to Quantinuum hardware or emulator.

    Polls the Quantinuum REST API for status and results.  Multi-circuit
    submissions create one Quantinuum job per circuit; this wrapper tracks
    all job IDs.
    """

    def __init__(self, backend: ArvakQuantinuumBackend, job_id: str,
                 all_job_ids: list, shots: int, num_circuits: int):
        self._backend = backend
        self._job_id = job_id
        self._all_job_ids = all_job_ids
        self._shots = shots
        self._num_circuits = num_circuits

    def job_id(self) -> str:
        return self._job_id

    def status(self) -> str:
        """Return the current status of the primary job."""
        headers = self._backend._headers()
        resp = self._backend._requests.get(
            f"{_QUANTINUUM_API_BASE}/job/{self._job_id}",
            headers=headers,
            timeout=30,
        )
        if not resp.ok:
            return "UNKNOWN"
        return resp.json().get("status", "UNKNOWN").upper()

    def cancel(self) -> None:
        """Cancel all submitted jobs.

        Raises:
            ArvakJobError: If any job cannot be found or is already terminal.
            ArvakSubmissionError: If the API request fails.
        """
        headers = self._backend._headers()
        for jid in self._all_job_ids:
            resp = self._backend._requests.post(
                f"{_QUANTINUUM_API_BASE}/job/{jid}/cancel",
                headers=headers,
                timeout=30,
            )
            if resp.status_code == 404:
                raise ArvakJobError(f"Job {jid} not found")
            if not resp.ok:
                raise ArvakSubmissionError(f"Failed to cancel job {jid}: {resp.text}")

    def _poll_one(self, job_id: str, timeout: int,
                  poll_interval: int, start: float) -> dict:
        """Poll a single Quantinuum job ID until complete; return its count dict."""
        headers = self._backend._headers()

        while True:
            elapsed = time.time() - start
            if elapsed > timeout:
                raise ArvakTimeoutError(f"Job {job_id} timed out after {timeout}s")

            resp = self._backend._requests.get(
                f"{_QUANTINUUM_API_BASE}/job/{job_id}",
                headers=headers,
                timeout=30,
            )
            if resp.status_code == 401:
                # Token expired during long poll — refresh.
                self._backend._id_token = None
                headers = self._backend._headers()
                resp = self._backend._requests.get(
                    f"{_QUANTINUUM_API_BASE}/job/{job_id}",
                    headers=headers,
                    timeout=30,
                )
            if not resp.ok:
                raise ArvakJobError(f"Failed to check job status: {resp.text}")

            data = resp.json()
            status = data.get("status", "").lower()

            if status == "completed":
                print(f"\r  [{job_id[:8]}] completed, elapsed: {elapsed:.0f}s   ")
                break
            if status in ("failed", "error"):
                error_msg = data.get("error", "unknown error")
                raise ArvakJobError(f"Job {job_id} failed: {error_msg}")
            if status in ("canceled", "cancelled"):
                raise ArvakJobCancelledError(f"Job {job_id} was cancelled")

            print(f"\r  [{job_id[:8]}] status: {status}, elapsed: {elapsed:.0f}s",
                  end="", flush=True)
            time.sleep(poll_interval)

        # Parse the results field: {register_name: [bit_shot_0, bit_shot_1, ...]}
        raw_results = data.get("results") or {}
        if not raw_results:
            return {}

        # Sort register names for consistent bit ordering.
        reg_names = sorted(raw_results.keys())
        if not reg_names:
            return {}

        n_shots = len(raw_results[reg_names[0]])
        counts: dict[str, int] = {}
        for shot in range(n_shots):
            bitstring = "".join(
                str(raw_results[reg][shot]) if shot < len(raw_results[reg]) else "0"
                for reg in reg_names
            )
            counts[bitstring] = counts.get(bitstring, 0) + 1

        return counts

    def result(self, timeout: int = 600, poll_interval: int = 5) -> 'ArvakResult':
        """Wait for all submitted jobs to complete and return aggregated results.

        Args:
            timeout: Maximum total wait time in seconds (default: 600)
            poll_interval: Seconds between status checks (default: 5)

        Returns:
            ArvakResult with measurement counts

        Raises:
            ArvakTimeoutError: If job does not complete within timeout
            ArvakJobError: If job fails on the backend
            ArvakJobCancelledError: If job is cancelled
        """
        start = time.time()
        all_counts = []

        for jid in self._all_job_ids:
            counts = self._poll_one(jid, timeout, poll_interval, start)
            all_counts.append(counts)

        while len(all_counts) < self._num_circuits:
            all_counts.append({})

        return ArvakResult(
            backend_name=self._backend.name,
            counts=all_counts,
            shots=self._shots,
        )

    def __repr__(self) -> str:
        return f"<ArvakQuantinuumJob(id='{self._job_id}', circuits={self._num_circuits})>"


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


def _infer_bit_width(samples: list[str], num_clbits: int = 0) -> int:
    """Infer classical register bit width from V2 hex samples (DEBT-09).

    When all samples are 0x0 (all-zero measurement outcomes), the hex string
    gives no width information. In this case we fall back to ``num_clbits``
    from the result metadata. If that is also unavailable we return 1 as a
    safe minimum so downstream formatting still produces valid bitstrings.
    """
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
        return max(1, num_clbits)
    return max_val.bit_length()


import math as _math
import re as _re
import zlib as _zlib
import base64 as _base64


def _arvak_qasm_to_iqm_circuit(qasm_out: str) -> 'QuantumCircuit':
    """Convert Arvak-compiled QASM3 (prx + cz basis) to a Qiskit circuit
    using IQM's native gate set directly.

    PRX(θ, φ) and IQM's native R(θ, φ) are the same operation — both equal
    exp(-i θ/2 (cos φ X + sin φ Y)).  Arvak emits them as prx; qiskit-on-iqm
    calls them r.  We map directly (1 prx → 1 RGate) instead of going through
    a QASM gate-definition expansion which causes 3× gate overhead per prx.

    Arvak sometimes emits ry/rz/rx instead of prx (compiler gap).  These are
    also converted to RGate without going through the prx definition.
    """
    from qiskit import QuantumCircuit as _QC
    from qiskit.circuit.library import RGate as _RGate

    def _eval(expr: str) -> float:
        return eval(expr.strip(), {"pi": _math.pi, "__builtins__": {}})  # noqa: S307

    def _norm(theta: float, phi: float) -> tuple[float, float]:
        """Normalise to theta >= 0, phi in [0, 2π)."""
        if theta < 0:
            theta, phi = -theta, phi + _math.pi
        return theta, phi % (2 * _math.pi)

    # Parse qubit / bit counts from header
    n_qubits = n_bits = 0
    for line in qasm_out.splitlines():
        m = _re.match(r'\s*qubit\[(\d+)\]', line)
        if m:
            n_qubits = max(n_qubits, int(m.group(1)))
        m = _re.match(r'\s*bit\[(\d+)\]', line)
        if m:
            n_bits = max(n_bits, int(m.group(1)))

    qc = _QC(n_qubits, n_bits)

    for raw in qasm_out.splitlines():
        line = raw.strip().rstrip(';')
        if not line or line.startswith(('OPENQASM', '//', 'qubit', 'bit', 'include')):
            continue

        # prx(θ, φ) q[i]  →  RGate(θ, φ)  (1:1 with IQM r gate)
        m = _re.match(r'prx\(([^,]+),\s*([^)]+)\)\s+q\[(\d+)\]', line)
        if m:
            theta, phi = _norm(_eval(m.group(1)), _eval(m.group(2)))
            qc.append(_RGate(theta, phi), [int(m.group(3))])
            continue

        # ry(θ) q[i]  →  RGate(θ, π/2)
        m = _re.match(r'ry\(([^)]+)\)\s+q\[(\d+)\]', line)
        if m:
            theta, phi = _norm(_eval(m.group(1)), _math.pi / 2)
            qc.append(_RGate(theta, phi), [int(m.group(2))])
            continue

        # rx(θ) q[i]  →  RGate(θ, 0)
        m = _re.match(r'rx\(([^)]+)\)\s+q\[(\d+)\]', line)
        if m:
            theta, phi = _norm(_eval(m.group(1)), 0.0)
            qc.append(_RGate(theta, phi), [int(m.group(2))])
            continue

        # rz(θ) q[i]  →  3-gate R decomposition: R(π/2,0) R(θ,π/2) R(-π/2,0)
        # (standard RZ → R(θ,φ) identity, avoids qiskit transpiler chain)
        m = _re.match(r'rz\(([^)]+)\)\s+q\[(\d+)\]', line)
        if m:
            angle = _eval(m.group(1))
            qi = int(m.group(2))
            qc.append(_RGate(_math.pi / 2, 0.0), [qi])
            t, p = _norm(angle, _math.pi / 2)
            qc.append(_RGate(t, p), [qi])
            qc.append(_RGate(_math.pi / 2, _math.pi), [qi])
            continue

        # cz q[i], q[j]
        m = _re.match(r'cz\s+q\[(\d+)\],\s*q\[(\d+)\]', line)
        if m:
            qc.cz(int(m.group(1)), int(m.group(2)))
            continue

        # c[i] = measure q[j]
        m = _re.match(r'c\[(\d+)\]\s*=\s*measure\s+q\[(\d+)\]', line)
        if m:
            qc.measure(int(m.group(2)), int(m.group(1)))
            continue

    return qc


def _scaleway_compress(data: str) -> str:
    """ZLIB+Base64 compress a string (Scaleway ZLIB_BASE64_V1 / compression_format=1)."""
    compressed = _zlib.compress(data.encode("utf-8"))
    return _base64.b64encode(compressed).decode("ascii")


def _iqm_postprocess_qasm(qasm: str) -> str:
    """Post-process Arvak-compiled QASM3 for IQM hardware submission via Scaleway.

    Handles any residual `ry` / `rz` instructions that the single-qubit
    optimizer may re-introduce via ZYZ decomposition after basis translation.
    This function:
      1. Strips any existing `include "stdgates.inc"` (re-added in canonical position)
      2. Converts `ry(angle) q` → `prx(angle, pi/2) q`
      3. Converts `rz(angle) q` → 3-gate prx decomposition
      4. Evaluates symbolic `pi` expressions to float literals
      5. Normalizes prx angles: theta >= 0, phi in [0, 2π)
      6. Inserts a `gate prx(theta, phi) q` definition so Scaleway's QASM3 parser
         can decompose it into standard gates (rz, rx) for IQM submission.
    """
    # Remove any existing stdgates include (re-added via _PREAMBLE below)
    qasm = _re.sub(r'\s*include\s+"stdgates\.inc"\s*;\s*', '\n', qasm)

    # Convert ry(angle) qubit; → prx(angle, pi/2) qubit;
    def _replace_ry(m):
        angle = m.group(1)
        qubit = m.group(2)
        return f"prx({angle}, pi/2) {qubit};"

    qasm = _re.sub(r'\bry\s*\(([^)]+)\)\s+([^;]+);', _replace_ry, qasm)

    # Convert rz(angle) qubit; → 3-gate prx sequence
    def _replace_rz(m):
        angle = m.group(1)
        qubit = m.group(2)
        return (f"prx(-pi/2, 0) {qubit};\n"
                f"prx({angle}, pi/2) {qubit};\n"
                f"prx(pi/2, 0) {qubit};")

    qasm = _re.sub(r'\brz\s*\(([^)]+)\)\s+([^;]+);', _replace_rz, qasm)

    # Evaluate symbolic pi expressions to floats and normalize prx angles.
    # IQM requires: theta >= 0 (use prx(-θ,φ) = prx(θ, φ+π)) and phi in [0, 2π).
    def _eval_float(expr: str) -> float:
        return eval(expr, {"pi": _math.pi, "__builtins__": {}})  # noqa: S307

    def _normalize_prx(theta: float, phi: float) -> tuple:
        if theta < 0:
            theta = -theta
            phi = phi + _math.pi
        phi = phi % (2 * _math.pi)
        return theta, phi

    def _replace_prx_angles(m):
        args = m.group(1)
        rest = m.group(2)
        parts = [a.strip() for a in args.split(",")]
        if len(parts) != 2:
            return m.group(0)
        try:
            theta = _eval_float(parts[0])
            phi = _eval_float(parts[1])
            theta, phi = _normalize_prx(theta, phi)
            return f"prx({theta:.10f}, {phi:.10f}){rest}"
        except Exception:
            return m.group(0)

    qasm = _re.sub(r'\bprx\s*\(([^)]+)\)([ \t]+[^;]+;)', _replace_prx_angles, qasm)

    # Add stdgates.inc + prx gate definition immediately after the OPENQASM header.
    # prx(theta, phi) decomposes as: RZ(-phi) RX(theta) RZ(phi)  (angles in radians)
    # This definition is required because prx is not a standard QASM3 built-in gate.
    _PREAMBLE = (
        'include "stdgates.inc";\n'
        'gate prx(theta, phi) q {\n'
        '    rz(-phi) q;\n'
        '    rx(theta) q;\n'
        '    rz(phi) q;\n'
        '}\n'
    )
    qasm = qasm.replace("OPENQASM 3.0;", f"OPENQASM 3.0;\n{_PREAMBLE}", 1)

    return qasm
