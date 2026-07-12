"""Qrisp backend client for Arvak.

This module implements Qrisp's :class:`~qrisp.interface.VirtualBackend`
interface on top of Arvak's native HAL backends, allowing Qrisp programs
(including the high-level ``QuantumVariable`` / ``get_measurement`` API)
to execute on any backend known to ``arvak.backend_for()``.

All submission, compilation (routing + gate translation), status polling,
and result decoding happen in the native Rust adapters via PyO3 — there is
no per-vendor Python code here. See
``docs/RFC/0001-native-backend-unification.md`` for the architecture.

Backend names are the same as on the Qiskit side (``arvak.list_backends()``):
``sim``, ``iqm_sirius``, ``scaleway_garnet``, ``ibm_marrakesh``,
``quantinuum_h2_emulator``, ``aqt_offline_sim``, ``ionq_simulator``, …

Two legacy aliases remain supported:
  - ``'iqm'``      → ``iqm_<IQM_COMPUTER>``          (default: ``iqm_sirius``)
  - ``'scaleway'`` → ``scaleway_<SCALEWAY_PLATFORM>`` (default: ``scaleway_garnet``)

Hardware backends read their credentials from the environment exactly like
the Qiskit integration (``IQM_TOKEN``, ``SCALEWAY_SECRET_KEY`` /
``SCALEWAY_PROJECT_ID`` / ``SCALEWAY_SESSION_ID``, ``IBM_API_KEY`` /
``IBM_SERVICE_CRN``, ``QUANTINUUM_EMAIL`` / ``QUANTINUUM_PASSWORD``,
``AQT_TOKEN``, ``IONQ_API_KEY``).
"""

import os
from typing import Optional

from qrisp.interface import VirtualBackend

# Shots used when the caller passes shots=None. Qrisp's high-level API
# (e.g. ``QuantumVariable.get_measurement``) passes None to mean "backend
# default"; the native HAL layer requires a concrete integer.
DEFAULT_SHOTS = 1024

# Scaleway QaaS platform identifiers → arvak registry machine names.
_SCALEWAY_PLATFORM_TO_MACHINE = {
    'QPU-GARNET-20PQ': 'garnet',
    'QPU-SIRIUS-24PQ': 'sirius',
    'QPU-EMERALD-54PQ': 'emerald',
}

# Aliases kept for backwards compatibility with the pre-2.0 integration.
_LEGACY_ALIASES = ('iqm', 'scaleway')


def _resolve_backend_name(name: str) -> str:
    """Map legacy alias names to arvak registry names.

    Reads ``IQM_COMPUTER`` / ``SCALEWAY_PLATFORM`` from the environment for
    the legacy aliases; registry names pass through unchanged.
    """
    if name == 'iqm':
        return f"iqm_{os.environ.get('IQM_COMPUTER', 'sirius')}"
    if name == 'scaleway':
        platform = os.environ.get('SCALEWAY_PLATFORM', 'QPU-GARNET-20PQ')
        machine = _SCALEWAY_PLATFORM_TO_MACHINE.get(platform, 'garnet')
        return f'scaleway_{machine}'
    return name


class ArvakBackendClient(VirtualBackend):
    """Arvak backend client for Qrisp.

    Subclasses Qrisp's :class:`~qrisp.interface.VirtualBackend`, so it can be
    passed anywhere Qrisp expects a backend — most importantly to
    ``QuantumVariable.get_measurement(backend=...)``.

    Example (simulator)::

        >>> from qrisp import QuantumVariable, h, cx
        >>> from arvak.integrations.qrisp import ArvakBackendClient
        >>> backend = ArvakBackendClient('sim')
        >>> qv = QuantumVariable(2)
        >>> h(qv[0]); cx(qv[0], qv[1])
        >>> qv.get_measurement(backend=backend)
        {'00': 0.5, '11': 0.5}

    Example (IQM Resonance — requires credentials)::

        >>> import os
        >>> os.environ['IQM_TOKEN'] = '<your-resonance-token>'
        >>> backend = ArvakBackendClient('iqm_sirius')
        >>> counts = backend.run(qc, shots=1024)
    """

    def __init__(self, backend_name: str = 'sim'):
        """Initialize the Arvak backend client.

        Args:
            backend_name: Any name from ``arvak.list_backends()`` (e.g.
                          ``'sim'``, ``'iqm_sirius'``, ``'ibm_marrakesh'``)
                          or a legacy alias (``'iqm'``, ``'scaleway'``).
                          (default: ``'sim'``)

        Construction is cheap and never touches the network; the native
        backend (and its credential check) is created lazily on first use.
        """
        self.backend_name = backend_name
        self.name = f'arvak_{backend_name}'
        self._native_backend = None

        if backend_name == 'sim':
            self.description = 'Arvak Rust statevector simulator'
        else:
            self.description = (
                f'Arvak native backend ({_resolve_backend_name(backend_name)})'
            )

        super().__init__(run_func=self._run_qasm)

    @property
    def _native(self):
        """The underlying ``arvak.Backend``, created on first access.

        Lazy so that constructing a hardware client without credentials in
        the environment does not raise — the error surfaces at ``run()``,
        matching the previous behaviour of this class.
        """
        if self._native_backend is None:
            import arvak
            self._native_backend = arvak.backend_for(
                _resolve_backend_name(self.backend_name)
            )
        return self._native_backend

    def run(self, qc, shots: Optional[int] = None,
            token: str = '') -> dict[str, int]:
        """Run a Qrisp circuit on the configured Arvak backend.

        For ``'sim'`` the circuit executes locally in Rust with no network
        calls. For hardware backends the circuit is compiled (routing +
        basis translation) and submitted by the native Rust adapter.

        Args:
            qc:    Qrisp QuantumCircuit or QuantumSession.
            shots: Number of measurement shots. ``None`` (as passed by
                   Qrisp's high-level API) uses ``DEFAULT_SHOTS``.
            token: Ignored — credentials come from the environment.

        Returns:
            Dictionary mapping bitstrings to measurement counts.

        Raises:
            RuntimeError: For hardware backends if required credentials are
                          not set in the environment.
        """
        from qrisp import QuantumSession, transpile
        if isinstance(qc, QuantumSession):
            qc = qc.compile()
        # Flatten composite gates (e.g. Qrisp's QFT blocks from QuantumFloat
        # arithmetic) into elementary gates — Arvak's QASM parser does not
        # accept user-defined gate blocks.
        qc = transpile(qc)
        return super().run(qc, shots, token)

    def _run_qasm(self, qasm2_str: str, shots: Optional[int] = None,
                  token: str = '') -> dict[str, int]:
        """``run_func`` for VirtualBackend: QASM 2.0 in, counts out."""
        from .._qasm import qasm2_to_qasm3

        if not shots:
            shots = DEFAULT_SHOTS

        qasm3_str = qasm2_to_qasm3(qasm2_str)
        result = self._native.run(qasm3_str, shots)
        return dict(result.counts)

    def __repr__(self) -> str:
        return f"<ArvakBackendClient('{self.name}')>"


class ArvakProvider:
    """Arvak backend provider for Qrisp.

    Allows Qrisp programs to discover and use Arvak backends. The available
    names come directly from the native registry (``arvak.list_backends()``)
    plus the legacy aliases ``'iqm'`` and ``'scaleway'``.

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
            name: Any name from ``arvak.list_backends()`` or a legacy alias
                  (``'iqm'``, ``'scaleway'``). (default: ``'sim'``)

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
            List of ArvakBackendClient instances. Construction is lazy and
            credential-free, so this is safe without any vendor tokens set.
        """
        for backend_name in self._available_backend_names():
            if backend_name not in self._backends:
                self._backends[backend_name] = ArvakBackendClient(backend_name)

        if name:
            backend = self._backends.get(name)
            return [backend] if backend else []

        return list(self._backends.values())

    @staticmethod
    def _available_backend_names() -> list[str]:
        """All supported backend names: native registry + legacy aliases."""
        import arvak
        return list(arvak.list_backends()) + list(_LEGACY_ALIASES)

    def __repr__(self) -> str:
        return f"<ArvakProvider(backends={self._available_backend_names()})>"
