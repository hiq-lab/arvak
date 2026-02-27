"""HAL backend adapter for PCESolver.

Bridges any Arvak HAL backend (IBM, IQM, AQT, Quantinuum, Scaleway, Simulator)
to the Backend callable protocol expected by PCESolver.

Conversion chain for real hardware::

    arvak.Circuit
        → arvak.to_qasm()       (QASM3 string)
        → qiskit.qasm3.loads()  (Qiskit QuantumCircuit)
        → backend.run(shots)    (HAL job)
        → job.result(timeout)   (ArvakResult)
        → .get_counts()         (dict[str, int])

For the local simulator, arvak.run_sim() is called directly — no Qiskit needed.

Example::

    from arvak.optimize import BinaryQubo, PCESolver, HalBackend
    from arvak.integrations.qiskit.backend import ArvakIBMBackend

    ibm = ArvakIBMBackend(backend_name="ibm_torino")
    qubo = BinaryQubo.from_matrix(Q)
    solver = PCESolver(qubo, backend=HalBackend(ibm), shots=1024)
    result = solver.solve()
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import arvak

if TYPE_CHECKING:
    pass


class HalBackend:
    """Wraps an Arvak HAL backend as a PCESolver-compatible callable.

    PCESolver expects a backend with signature::

        backend(circuit: arvak.Circuit, shots: int) -> dict[str, int]

    HalBackend adapts any HAL backend (ArvakIBMBackend, ArvakAQTBackend, etc.)
    to that interface by converting the arvak.Circuit to a Qiskit QuantumCircuit
    via QASM3 before submission.

    Args:
        backend:     Any Arvak HAL backend instance (must have .run() and accept
                     Qiskit QuantumCircuit objects).
        poll_timeout: Seconds to wait for job completion on real hardware.
                     Default 600 s (10 minutes).
        check_availability: If True (default), call backend.availability() before
                     the first circuit submission and raise if offline.

    Raises:
        ImportError:  If qiskit is not installed (required for circuit conversion).
        RuntimeError: If backend is unavailable and check_availability=True.
    """

    def __init__(
        self,
        backend,
        *,
        poll_timeout: int = 600,
        check_availability: bool = True,
    ) -> None:
        self._backend = backend
        self._poll_timeout = poll_timeout
        self._check_availability = check_availability
        self._availability_checked = False

        _require_qiskit()

    # ------------------------------------------------------------------
    # Backend protocol
    # ------------------------------------------------------------------

    def __call__(self, circuit: arvak.Circuit, shots: int) -> dict[str, int]:
        """Run circuit on the wrapped HAL backend and return shot counts.

        Args:
            circuit: Arvak circuit to execute (hardware-agnostic; the HAL
                     backend handles compilation to native gates).
            shots:   Number of measurement shots.

        Returns:
            dict[str, int] mapping binary bitstrings to counts.
        """
        if self._check_availability and not self._availability_checked:
            self._assert_available()
            self._availability_checked = True

        qiskit_circuit = _arvak_to_qiskit(circuit)
        job = self._backend.run(qiskit_circuit, shots=shots)
        # Simulator jobs don't accept timeout; real hardware jobs do.
        try:
            result = job.result(timeout=self._poll_timeout)
        except TypeError:
            result = job.result()
        counts = result.get_counts()
        return _normalize_counts(counts, n_bits=circuit.num_qubits)

    # ------------------------------------------------------------------
    # Convenience constructors
    # ------------------------------------------------------------------

    @classmethod
    def simulator(cls) -> "HalBackend":
        """Return a HalBackend wrapping the Arvak built-in statevector simulator.

        This is equivalent to the default PCESolver backend (arvak.run_sim)
        but wrapped in the HalBackend interface for consistency.

        Note: PCESolver already defaults to run_sim; this constructor is useful
        when you want to swap in a real backend later without changing the solver.
        """
        from arvak.integrations.qiskit.backend import ArvakProvider
        provider = ArvakProvider()
        return cls(provider.get_backend("sim"), check_availability=False)

    @classmethod
    def ibm(cls, backend_name: str, **kwargs) -> "HalBackend":
        """Return a HalBackend wrapping an IBM Quantum backend.

        Args:
            backend_name: IBM backend name, e.g. "ibm_torino", "ibm_strasbourg".
            **kwargs:     Passed to ArvakIBMBackend (e.g. service_crn, region).
        """
        from arvak.integrations.qiskit.backend import ArvakIBMBackend
        return cls(ArvakIBMBackend(backend_name=backend_name, **kwargs))

    @classmethod
    def iqm(cls, computer: str = "Garnet", **kwargs) -> "HalBackend":
        """Return a HalBackend wrapping an IQM Resonance backend.

        Args:
            computer: IQM quantum computer name, e.g. "Garnet", "Sirius", "Emerald".
            **kwargs: Passed to ArvakIQMResonanceBackend.
        """
        from arvak.integrations.qiskit.backend import ArvakIQMResonanceBackend
        return cls(ArvakIQMResonanceBackend(computer=computer, **kwargs))

    @classmethod
    def aqt(cls, resource: str = "offline_simulator_no_noise", **kwargs) -> "HalBackend":
        """Return a HalBackend wrapping an AQT backend.

        Args:
            resource: AQT resource ID. Default is the free offline simulator.
            **kwargs: Passed to ArvakAQTBackend.
        """
        from arvak.integrations.qiskit.backend import ArvakAQTBackend
        return cls(ArvakAQTBackend(resource=resource, **kwargs))

    @classmethod
    def quantinuum(cls, device: str = "H2-1LE", **kwargs) -> "HalBackend":
        """Return a HalBackend wrapping a Quantinuum backend.

        Args:
            device: Quantinuum device name. Default "H2-1LE" (noiseless emulator,
                    free, 32 qubits, all-to-all).
            **kwargs: Passed to ArvakQuantinuumBackend.
        """
        from arvak.integrations.qiskit.backend import ArvakQuantinuumBackend
        return cls(ArvakQuantinuumBackend(device=device, **kwargs))

    # ------------------------------------------------------------------
    # Internals
    # ------------------------------------------------------------------

    def _assert_available(self) -> None:
        """Check backend availability and raise if offline."""
        if not hasattr(self._backend, "availability"):
            return
        avail = self._backend.availability()
        if not avail.online:
            raise RuntimeError(
                f"Backend unavailable: {avail.status_message}. "
                "Pass check_availability=False to suppress this check."
            )

    def __repr__(self) -> str:
        name = getattr(self._backend, "name", type(self._backend).__name__)
        return f"HalBackend({name!r}, timeout={self._poll_timeout}s)"


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

def _require_qiskit() -> None:
    try:
        import qiskit  # noqa: F401
        from qiskit import qasm3  # noqa: F401
    except ImportError as exc:
        raise ImportError(
            "HalBackend requires qiskit. Install with:\n"
            "    pip install 'arvak[qiskit]'"
        ) from exc


def _arvak_to_qiskit(circuit: arvak.Circuit):
    """Convert an arvak.Circuit to a Qiskit QuantumCircuit via QASM3.

    Arvak's QASM3 emitter omits the stdgates include; we inject it so that
    qiskit_qasm3_import can resolve standard gate names (h, cx, ry, etc.).
    """
    from qiskit import qasm3

    qasm_str = arvak.to_qasm(circuit)

    # Inject stdgates if not already present.
    if 'include "stdgates.inc"' not in qasm_str:
        # Insert after the OPENQASM version line.
        lines = qasm_str.splitlines()
        insert_at = next(
            (i + 1 for i, l in enumerate(lines) if l.strip().startswith("OPENQASM")),
            0,
        )
        lines.insert(insert_at, 'include "stdgates.inc";')
        qasm_str = "\n".join(lines)

    return qasm3.loads(qasm_str)


def _normalize_counts(counts: dict[str, int], n_bits: int = 0) -> dict[str, int]:
    """Normalise a counts dict to plain binary string keys.

    HAL backends may return keys with spaces (Qiskit register separator),
    hex strings, or variable-length binary strings.  This function:
      - Strips spaces (joins multi-register bitstrings)
      - Zero-pads to n_bits if shorter
      - Passes through plain binary strings unchanged
    """
    normalised: dict[str, int] = {}
    for key, count in counts.items():
        # Remove register-separator spaces (Qiskit: "01 10" → "0110")
        key = key.replace(" ", "")
        # Convert hex keys (some backends return "0x..." or plain hex)
        if key.startswith("0x") or (key and all(c in "0123456789abcdefABCDEF" for c in key) and not all(c in "01" for c in key)):
            try:
                val = int(key, 16)
                width = max(n_bits, len(key.lstrip("0x")) * 4) if key.startswith("0x") else max(n_bits, len(key) * 4)
                key = format(val, f"0{width}b")
            except ValueError:
                pass
        # Zero-pad short binary strings
        if n_bits > 0 and len(key) < n_bits:
            key = key.zfill(n_bits)
        normalised[key] = normalised.get(key, 0) + count
    return normalised
