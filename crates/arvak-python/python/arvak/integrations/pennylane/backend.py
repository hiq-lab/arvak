"""PennyLane device for Arvak.

This module implements PennyLane's modern ``qml.devices.Device`` interface
on top of Arvak's native HAL backends. QNodes attach directly::

    dev = ArvakDevice(wires=2)

    @qml.qnode(dev)
    def circuit(x):
        qml.RX(x, wires=0)
        qml.CNOT(wires=[0, 1])
        return qml.expval(qml.PauliZ(0))

Execution is sampling-based: circuits are serialized to OpenQASM (with
diagonalizing rotations for non-Z-basis observables), run on the selected
Arvak backend via ``arvak.backend_for(name)``, and all measurement
statistics (expval / var / probs / counts / sample) are computed from the
returned counts by PennyLane's ``measurements_from_counts`` machinery.
Non-commuting observables — e.g. molecular Hamiltonians — are split into
separate executions automatically. Parameter-shift gradients work, so
VQE optimization can run entirely on Arvak backends.

Any backend from ``arvak.list_backends()`` is available: ``sim`` (default,
local Rust statevector simulator), IQM, Scaleway, IBM, Quantinuum, AQT,
IonQ, … Hardware backends read credentials from the environment exactly
like the Qiskit and Qrisp integrations.
"""

from __future__ import annotations

from typing import Optional

import pennylane as qml
from pennylane.devices import Device, ExecutionConfig
from pennylane.devices.preprocess import (
    measurements_from_counts,
    validate_device_wires,
)
from pennylane.transforms import split_non_commuting
from pennylane.transforms.core import TransformProgram

# Shots used when neither the tape nor the device specifies a count.
# This is a sampling device — there is no analytic mode; "no shots"
# falls back to this default.
DEFAULT_SHOTS = 1024


@qml.transform
def _ensure_shots(tape, default_shots: int = DEFAULT_SHOTS):
    """Give analytic tapes (shots=None) a concrete shot count.

    This device always samples. The downstream transforms
    (``split_non_commuting``, ``measurements_from_counts``) treat
    analytic tapes as pass-through, so shots must be set before they
    run. Tapes that already carry shots are left untouched.
    """
    if tape.shots.total_shots is None:
        tape = tape.copy(shots=default_shots)

    def null_postprocessing(results):
        return results[0]

    return [tape], null_postprocessing


class ArvakDevice(Device):
    """Arvak device implementing PennyLane's modern Device interface.

    Args:
        wires: Number of wires or an iterable of wire labels. ``None``
            (default) accepts any wires used by the circuit.
        shots: Default shot count for tapes that carry none. This device
            always samples; ``None`` means ``DEFAULT_SHOTS``.
        backend: Arvak backend name from ``arvak.list_backends()``
            (default: ``'sim'``).

    Example:
        >>> import pennylane as qml
        >>> from arvak.integrations.pennylane import ArvakDevice
        >>> dev = ArvakDevice(wires=2)
        >>> @qml.qnode(dev)
        ... def circuit(x):
        ...     qml.RX(x, wires=0)
        ...     qml.CNOT(wires=[0, 1])
        ...     return qml.expval(qml.PauliZ(0))
        >>> circuit(0.5)
    """

    name = "arvak.qpu"

    def __init__(self, wires=None, shots: Optional[int] = None,
                 backend: str = 'sim'):
        # `shots` is managed here, not by the base class — passing it to
        # Device.__init__ is deprecated since PennyLane 0.45.
        super().__init__(wires=wires)
        self._default_shots = shots if shots else DEFAULT_SHOTS
        self.backend_name = backend
        self._native_backend = None

    @property
    def _native(self):
        """The underlying ``arvak.Backend``, created on first access.

        Lazy so that constructing a hardware device without credentials in
        the environment does not raise — the error surfaces on execution.
        """
        if self._native_backend is None:
            import arvak
            self._native_backend = arvak.backend_for(self.backend_name)
        return self._native_backend

    def preprocess(self, execution_config: Optional[ExecutionConfig] = None):
        """Transform program: validate wires, split non-commuting
        observables (molecular Hamiltonians become multiple executions),
        and reduce every measurement to counts."""
        config = execution_config or ExecutionConfig()
        program = TransformProgram()
        program.add_transform(validate_device_wires, self.wires, name=self.name)
        program.add_transform(_ensure_shots, default_shots=self._default_shots)
        program.add_transform(split_non_commuting)
        program.add_transform(measurements_from_counts)
        return program, config

    def execute(self, circuits, execution_config: Optional[ExecutionConfig] = None):
        is_single = isinstance(circuits, qml.tape.QuantumScript)
        if is_single:
            circuits = [circuits]
        results = [self._execute_tape(tape) for tape in circuits]
        return results[0] if is_single else tuple(results)

    def _execute_tape(self, tape):
        """Run one tape on the native backend and return counts-derived results."""
        from .._qasm import qasm2_to_qasm3

        shots = tape.shots.total_shots or self._default_shots

        # rotations=True appends each observable's diagonalizing gates, so
        # the Z-basis samples below are taken in the correct eigenbasis.
        qasm2 = qml.to_openqasm(tape, rotations=True, measure_all=True)
        result = self._native.run(qasm2_to_qasm3(qasm2), shots)

        # Arvak bitstrings put q[0] rightmost (Qiskit convention);
        # PennyLane counts keys put the first wire leftmost — reverse.
        n = len(tape.wires)
        pl_counts: dict[str, int] = {}
        for bitstring, count in result.counts.items():
            key = bitstring.zfill(n)[::-1][:n]
            pl_counts[key] = pl_counts.get(key, 0) + count

        wire_order = list(tape.wires)
        out = []
        for m in tape.measurements:
            if not isinstance(m, qml.measurements.CountsMP):
                raise NotImplementedError(
                    f"ArvakDevice.execute received measurement {m}; expected "
                    f"counts only. The preprocess transform program must run "
                    f"before execute — use this device through a QNode, or "
                    f"apply `dev.preprocess()[0]` to the tape manually."
                )
            # Restrict the full-register counts to the measurement's wires.
            m_wires = m.wires if len(m.wires) else tape.wires
            idx = [wire_order.index(w) for w in m_wires]
            sub: dict[str, int] = {}
            for key, count in pl_counts.items():
                k = ''.join(key[i] for i in idx)
                sub[k] = sub.get(k, 0) + count
            out.append(sub)

        return out[0] if len(out) == 1 else tuple(out)

    def __repr__(self) -> str:
        return (f"<ArvakDevice(wires={self.wires}, "
                f"backend='{self.backend_name}', "
                f"shots={self._default_shots})>")


def create_device(backend: str = 'sim', **kwargs) -> ArvakDevice:
    """Create an Arvak device for PennyLane.

    Args:
        backend: Arvak backend name (default: 'sim')
        **kwargs: Additional device arguments (wires, shots, etc.)

    Returns:
        ArvakDevice instance

    Example:
        >>> dev = create_device('sim', wires=2, shots=1000)
    """
    return ArvakDevice(backend=backend, **kwargs)
