"""Qiskit backend provider for Arvak.

This module implements Qiskit's provider and backend interfaces, allowing
users to execute Arvak circuits through Qiskit's familiar backend.run() API.

All hardware routing now goes through the native HAL-backed
:class:`ArvakBackend`, which delegates submission, status polling, and
result decoding to the Rust adapters via PyO3. Per-vendor backend
classes (e.g. ``ArvakIBMBackend``) were removed in 2.0; use
``ArvakProvider().get_backend("ibm_marrakesh")`` and friends.
"""

from typing import Optional


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


# AQT Arnica cloud API constants
_AQT_API_BASE = "https://arnica.aqt.eu/api/v1"

# IonQ API constants
_IONQ_API_BASE = "https://api.ionq.co/v0.4"

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

    # AQT backend names (ion trap — offline simulators and IBEX Q1 hardware)
    # Note: all AQT resources require a real AQT_TOKEN — the Arnica cloud API
    # validates tokens even for offline simulators (confirmed 2026-02-21).
    _AQT_BACKENDS = {
        'aqt_offline_sim',   # offline_simulator_no_noise (requires AQT_TOKEN)
        'aqt_noise_sim',     # offline_simulator_noise (requires AQT_TOKEN)
        'aqt_cloud_sim',     # cloud simulator_noise (requires AQT_TOKEN)
    }

    _AQT_RESOURCE_MAP = {
        'aqt_offline_sim': ('default', 'offline_simulator_no_noise'),
        'aqt_noise_sim':   ('default', 'offline_simulator_noise'),
        'aqt_cloud_sim':   ('aqt_simulators', 'simulator_noise'),
    }

    # IonQ backend names (trapped-ion — simulator + QPU hardware)
    _IONQ_BACKENDS = {
        'ionq_simulator',   # cloud simulator (29q, free tier)
        'ionq_aria_1',      # qpu.aria-1 (25q)
        'ionq_aria_2',      # qpu.aria-2 (25q)
        'ionq_forte_1',     # qpu.forte-1 (36q)
    }

    _IONQ_DEVICE_MAP = {
        'ionq_simulator': 'simulator',
        'ionq_aria_1':    'qpu.aria-1',
        'ionq_aria_2':    'qpu.aria-2',
        'ionq_forte_1':   'qpu.forte-1',
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
            # Native HAL-backed simulator.
            self._backends['sim'] = ArvakBackend(provider=self, backend_name='sim')

        # Lazily create IBM backends on demand — native arvak-adapter-ibm
        # via PyO3. Construction triggers an HTTP call to fetch real backend
        # topology / qubit count from IBM Cloud.
        if name and name in self._IBM_BACKENDS and name not in self._backends:
            try:
                self._backends[name] = ArvakBackend(
                    provider=self, backend_name=name,
                )
            except (ValueError, RuntimeError, ConnectionError, PermissionError) as e:
                raise ValueError(
                    f"Cannot connect to {name}: {e}. "
                    f"Set IBM_API_KEY and IBM_SERVICE_CRN environment variables. "
                    f"For EU backends (brussels/strasbourg/aachen), also set "
                    f"IBM_SERVICE_CRN_EU."
                ) from e

        # Lazily create Scaleway backends — Phase 3: native arvak-adapter-scaleway
        # via PyO3. No more in-Python `requests` HTTP path.
        # See RFC-0001 Phase 3.
        if name and name in self._SCALEWAY_BACKENDS and name not in self._backends:
            try:
                self._backends[name] = ArvakBackend(
                    provider=self, backend_name=name,
                )
            except (ValueError, RuntimeError, ConnectionError) as e:
                raise ValueError(
                    f"Cannot connect to {name}: {e}. "
                    f"Set SCALEWAY_SECRET_KEY, SCALEWAY_PROJECT_ID, and "
                    f"SCALEWAY_SESSION_ID environment variables."
                ) from e

        # Lazily create IQM Resonance backends on demand — Phase 2a:
        # routes through the native arvak-adapter-iqm via PyO3, no longer
        # imports iqm-client[qiskit]. See RFC-0001 Phase 2.
        if name and name in self._IQM_RESONANCE_BACKENDS and name not in self._backends:
            try:
                self._backends[name] = ArvakBackend(
                    provider=self, backend_name=name,
                )
            except (ValueError, RuntimeError, ConnectionError) as e:
                raise ValueError(
                    f"Cannot connect to {name}: {e}. "
                    f"Set IQM_TOKEN environment variable (from resonance.meetiqm.com)."
                ) from e

        # Lazily create Quantinuum backends — Phase 5: native
        # arvak-adapter-quantinuum via PyO3. The registry maps the
        # arvak-side names (quantinuum_h2 / _h1_emulator / _h2_emulator)
        # to Quantinuum machine identifiers (H2-1 / H1-1E / H2-1E).
        # ionshuttler integration evaluated and skipped per RFC §Phase 5
        # gate eval on 2026-06-25 — Quantinuum's commercial pipeline
        # owns shuttle scheduling, not user-controllable via REST.
        if name and name in self._QUANTINUUM_BACKENDS and name not in self._backends:
            try:
                self._backends[name] = ArvakBackend(
                    provider=self, backend_name=name,
                )
            except (ValueError, RuntimeError, ConnectionError, PermissionError) as e:
                raise ValueError(
                    f"Cannot connect to {name}: {e}. "
                    f"Set QUANTINUUM_EMAIL and QUANTINUUM_PASSWORD environment variables."
                ) from e

        # Lazily create AQT backends on demand — Phase 6: native
        # arvak-adapter-aqt via PyO3.
        if name and name in self._AQT_BACKENDS and name not in self._backends:
            try:
                self._backends[name] = ArvakBackend(
                    provider=self, backend_name=name,
                )
            except (ValueError, RuntimeError, ConnectionError, PermissionError) as e:
                raise ValueError(
                    f"Cannot connect to {name}: {e}. "
                    f"Set AQT_TOKEN environment variable."
                ) from e

        # Lazily create IonQ backends on demand — Phase 6: native
        # arvak-adapter-ionq via PyO3.
        if name and name in self._IONQ_BACKENDS and name not in self._backends:
            try:
                self._backends[name] = ArvakBackend(
                    provider=self, backend_name=name,
                )
            except (ValueError, RuntimeError, ConnectionError, PermissionError) as e:
                raise ValueError(
                    f"Cannot connect to {name}: {e}. "
                    f"Set IONQ_API_KEY environment variable."
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
            ] + [
                b for b in self._AQT_BACKENDS if b not in self._backends
            ] + [
                b for b in self._IONQ_BACKENDS if b not in self._backends
            ]
            raise ValueError(
                f"Unknown backend: {name}. "
                f"Available backends: {', '.join(sorted(available))}"
            )
        return backends[0]

    def __repr__(self) -> str:
        return f"<ArvakProvider(backends={list(self._backends.keys())})>"


def _qiskit_to_qasm3(qc) -> str:
    """Serialize a Qiskit circuit to QASM3 (with QASM2 fallback)."""
    try:
        from qiskit.qasm3 import dumps
        return dumps(qc)
    except (ImportError, AttributeError):
        from qiskit.qasm2 import dumps as dumps2
        return dumps2(qc)


class ArvakBackend:
    """Native Arvak backend, Qiskit-compatible.

    Wraps an ``arvak.Backend`` from the PyO3 layer (which in turn wraps any
    ``arvak_hal::Backend`` adapter — sim, IQM, IBM, etc.). The Qiskit-shaped
    surface (``.name``, ``.num_qubits``, ``.basis_gates``, ``.coupling_map``,
    ``.run()``) is provided here so existing callers continue to work; all
    actual work happens in the native Rust adapter through a single code
    path.

    Use ``ArvakProvider().get_backend(name)`` to obtain instances — do not
    construct directly.

    See ``docs/RFC/0001-native-backend-unification.md`` for the architecture.
    """

    def __init__(self, provider: ArvakProvider, backend_name: str):
        import arvak  # imported lazily — arvak.Backend is from the PyO3 layer

        self._provider = provider
        self._native = arvak.backend_for(backend_name)
        # Qiskit-compatible metadata
        self.name = self._native.name
        self.description = f"Arvak native backend '{self.name}'"
        self.backend_version = '2.0.0'
        # Qiskit-shaped callers introspect this; no semantic meaning beyond
        # "this backend type exists".
        self.online_date = '2024-01-01'

    @property
    def max_circuits(self) -> Optional[int]:
        return None

    @property
    def num_qubits(self) -> int:
        return self._native.num_qubits

    @property
    def basis_gates(self) -> list[str]:
        return list(self._native.basis_gates)

    @property
    def coupling_map(self) -> Optional[list[list[int]]]:
        cm = self._native.coupling_map
        if cm is None:
            return None
        return [[a, b] for (a, b) in cm]

    def capabilities(self):
        """Return the underlying ``arvak.Capabilities`` (HAL view)."""
        return self._native.capabilities()

    def availability(self):
        """Query backend availability (returns ``arvak.Availability``)."""
        return self._native.availability()

    def validate(self, circuit) -> 'arvak.ValidationResult':
        """Validate a Qiskit circuit against backend constraints.

        Returns an ``arvak.ValidationResult`` from the HAL layer with three
        possible states: valid, invalid (with reasons), or requires
        transpilation. Use ``bool(result)`` for a quick valid/invalid check.

        Args:
            circuit: A single Qiskit ``QuantumCircuit``.

        Returns:
            ``arvak.ValidationResult`` — supports ``bool()``, ``.valid``,
            ``.reasons``, ``.requires_transpilation``, ``.details``.
        """
        qasm = _qiskit_to_qasm3(circuit)
        return self._native.validate(qasm)

    def run(self, circuits, shots: int = 1024, **options) -> 'ArvakJob':
        """Submit one or many Qiskit circuits; return a deferred job handle.

        This method submits each circuit and **returns immediately** with
        a job wrapper. Results are fetched on ``job.result()``, which
        blocks until the underlying HAL backends report completion. This
        matches Qiskit ``JobV1`` semantics and is important for cloud
        backends where jobs can sit in queue for hours.

        For sim, ``submit`` + ``result`` are effectively instant so the
        deferred-vs-eager distinction is invisible.

        Args:
            circuits: A single Qiskit ``QuantumCircuit`` or a list of them.
            shots: Number of measurement shots (default: 1024).
            **options: ``parameters`` is forwarded to HAL ``submit()``.

        Returns:
            An ``ArvakJob`` in deferred mode. Use ``job.result()`` to
            block on completion, ``job.status()`` to poll, ``job.cancel()``
            to abort.
        """
        if not isinstance(circuits, list):
            circuits = [circuits]

        parameters = options.get('parameters')  # forwarded to HAL submit()

        handles = [
            self._native.submit(_qiskit_to_qasm3(qc), shots, parameters)
            for qc in circuits
        ]

        return ArvakJob(backend=self, shots=shots, handles=handles)

    def __repr__(self) -> str:
        return f"<ArvakBackend('{self.name}')>"


class ArvakJob:
    """Job returned by an Arvak backend's ``run()``.

    Operates in one of two modes:

    - **Eager mode** — constructed with ``counts=[...]``. Results are
      already computed (caller pre-computed counts upstream).
    - **Deferred mode** — constructed with ``handles=[...]``, where
      each handle is an ``arvak.JobHandle`` from
      ``backend.submit()``. Results are fetched lazily on
      ``.result()``, allowing the caller to do other work between
      submission and result retrieval. This is the path
      ``ArvakBackend`` (the native HAL-backed class) uses.

    Either ``counts`` *or* ``handles`` must be passed; passing both is
    a programming error.
    """

    def __init__(self, backend, shots, counts=None, handles=None):
        if (counts is None) == (handles is None):
            raise ValueError(
                "ArvakJob requires exactly one of `counts` (eager) or "
                "`handles` (deferred)"
            )
        self._backend = backend
        self._shots = shots
        # Eager-mode state:
        self._counts = counts  # list[dict[str,int]] or None
        # Deferred-mode state:
        self._handles = handles  # list[arvak.JobHandle] or None
        self._cached_counts: list[dict[str, int]] | None = None

    def _is_deferred(self) -> bool:
        return self._handles is not None

    def result(self, timeout: float | None = None,
               poll_interval_ms: int = 500) -> 'ArvakResult':
        """Block until all circuits have completed; return aggregated result.

        Args:
            timeout: Maximum seconds to wait, applied per-handle. ``None``
                waits forever (matches Qiskit ``JobV1.result()`` semantics
                for cloud-vendor jobs with long queue times). Ignored in
                eager mode (results are already available).
            poll_interval_ms: Polling cadence forwarded to each handle's
                ``result()`` call. Default 500 ms. Ignored in eager mode.

        Returns:
            ``ArvakResult`` whose ``.get_counts(idx)`` returns the counts
            dict for circuit at index ``idx``.
        """
        if self._is_deferred():
            if self._cached_counts is None:
                self._cached_counts = [
                    dict(h.result(timeout=timeout,
                                  poll_interval_ms=poll_interval_ms).counts)
                    for h in self._handles
                ]
            counts = self._cached_counts
        else:
            counts = self._counts

        return ArvakResult(
            backend_name=self._backend.name,
            counts=counts,
            shots=self._shots,
        )

    def status(self) -> str:
        """Aggregate status across all submitted circuits.

        Eager mode returns ``"DONE"`` (results were computed at construction).
        Deferred mode polls each handle's status and aggregates:
        - ``"DONE"`` — every handle is completed.
        - ``"ERROR"`` — any handle reports failed.
        - ``"CANCELLED"`` — any handle reports cancelled.
        - ``"RUNNING"`` — at least one handle is running, none failed.
        - ``"QUEUED"`` — all handles are still queued.
        """
        if not self._is_deferred():
            return "DONE"

        states = [h.status().state for h in self._handles]
        if any(s == "failed" for s in states):
            return "ERROR"
        if any(s == "cancelled" for s in states):
            return "CANCELLED"
        if all(s == "completed" for s in states):
            return "DONE"
        if any(s == "running" for s in states):
            return "RUNNING"
        return "QUEUED"

    def cancel(self) -> None:
        """Cancel all in-flight handles. No-op in eager mode."""
        if not self._is_deferred():
            return
        for h in self._handles:
            try:
                h.cancel()
            except Exception:  # noqa: BLE001 — best-effort cancellation
                pass

    def job_id(self) -> str | list[str]:
        """Return the underlying HAL job id(s). Empty string in eager mode.

        Returns a single string when only one circuit was submitted,
        a list of strings for multi-circuit batches.
        """
        if not self._is_deferred():
            return ""
        ids = [h.job_id for h in self._handles]
        return ids[0] if len(ids) == 1 else ids

    def __repr__(self) -> str:
        if self._is_deferred():
            return (f"<ArvakJob(deferred, circuits={len(self._handles)}, "
                    f"shots={self._shots}, status={self.status()})>")
        return (f"<ArvakJob(eager, circuits={len(self._counts)}, "
                f"shots={self._shots})>")


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

