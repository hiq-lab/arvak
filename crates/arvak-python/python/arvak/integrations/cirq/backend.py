"""Cirq sampler for Arvak.

This module implements Cirq's ``cirq.Sampler`` interface on top of Arvak's
native HAL backends. ``ArvakSampler`` is a real Sampler subclass, so it
works anywhere Cirq expects one (``sampler.sample``, ``run_batch``, pandas
result data, plotting helpers, …) and returns standard ``cirq.ResultDict``
objects.

Circuits are serialized to OpenQASM with ``cirq.qasm`` and executed via
``arvak.backend_for(name)`` — any backend from ``arvak.list_backends()``
works: ``sim`` (default, local Rust statevector simulator), IQM, Scaleway,
IBM, Quantinuum, AQT, IonQ, … Hardware backends read credentials from the
environment exactly like the Qiskit / Qrisp / PennyLane integrations.
"""

from __future__ import annotations

import re
from typing import Optional, Sequence, TYPE_CHECKING

import cirq

if TYPE_CHECKING:
    import numpy as np


class ArvakSampler(cirq.Sampler):
    """Arvak sampler implementing Cirq's ``cirq.Sampler`` interface.

    Args:
        backend_name: Arvak backend name from ``arvak.list_backends()``
            (default: ``'sim'``). Construction is cheap and credential-free;
            hardware backends resolve lazily on first run.

    Example:
        >>> from arvak.integrations.cirq import ArvakSampler
        >>> import cirq
        >>>
        >>> qubits = cirq.LineQubit.range(2)
        >>> circuit = cirq.Circuit(
        ...     cirq.H(qubits[0]),
        ...     cirq.CNOT(qubits[0], qubits[1]),
        ...     cirq.measure(*qubits, key='result')
        ... )
        >>>
        >>> sampler = ArvakSampler('sim')
        >>> result = sampler.run(circuit, repetitions=1000)
        >>> print(result.histogram(key='result'))
    """

    def __init__(self, backend_name: str = 'sim'):
        self.backend_name = backend_name
        self.name = f'arvak_{backend_name}'
        self._native_backend = None

    @property
    def _native(self):
        """The underlying ``arvak.Backend``, created on first access."""
        if self._native_backend is None:
            import arvak
            self._native_backend = arvak.backend_for(self.backend_name)
        return self._native_backend

    def run_sweep(self, program: 'cirq.AbstractCircuit',
                  params: 'cirq.Sweepable',
                  repetitions: int = 1) -> Sequence['cirq.Result']:
        """Run the circuit for every parameter resolver in the sweep.

        Args:
            program: Cirq circuit to execute. All measurements must be
                terminal.
            params: Parameters to sweep over (``None`` for none).
            repetitions: Number of shots per resolver.

        Returns:
            List of ``cirq.ResultDict``, one per resolver.
        """
        results = []
        for resolver in cirq.to_resolvers(params):
            resolved = cirq.resolve_parameters(program, resolver)
            measurements = self._sample(resolved, repetitions)
            results.append(
                cirq.ResultDict(params=resolver, measurements=measurements)
            )
        return results

    def _sample(self, program: 'cirq.AbstractCircuit',
                repetitions: int) -> dict[str, 'np.ndarray']:
        """Execute one resolved circuit; return per-key measurement arrays.

        The circuit is exported with ``cirq.qasm`` (one classical register
        per measurement key) and run on the native backend. Arvak returns
        counts keyed by the concatenated classical bits — declaration
        order, bit 0 rightmost — which are mapped back to each key's
        qubits by parsing the emitted ``measure`` statements.
        """
        import numpy as np
        from .._qasm import qasm2_to_qasm3

        if not program.are_all_measurements_terminal():
            raise ValueError(
                "ArvakSampler only supports terminal measurements; "
                "mid-circuit measurement is not supported."
            )

        # key → measured qubits, in the measurement gate's qubit order
        key_qubits: dict[str, tuple] = {}
        for op in program.all_operations():
            if cirq.is_measurement(op):
                key = cirq.measurement_key_name(op)
                if key in key_qubits:
                    raise ValueError(
                        f"Duplicate measurement key {key!r} is not supported."
                    )
                key_qubits[key] = tuple(op.qubits)
        if not key_qubits:
            raise ValueError(
                "Circuit has no measurements — add cirq.measure(...) "
                "before sampling."
            )

        qasm2 = cirq.qasm(program)

        # Global classical-bit index for each measured QASM qubit index.
        # cirq.qasm declares one creg per measurement key; Arvak's counts
        # bitstring concatenates all classical bits in declaration order
        # with bit 0 rightmost.
        creg_offset: dict[str, int] = {}
        total_clbits = 0
        qubit_to_clbit: dict[int, int] = {}
        for line in qasm2.splitlines():
            line = line.strip()
            m = re.match(r'creg\s+(\w+)\[(\d+)\];', line)
            if m:
                creg_offset[m.group(1)] = total_clbits
                total_clbits += int(m.group(2))
                continue
            m = re.match(r'measure\s+\w+\[(\d+)\]\s*->\s*(\w+)\[(\d+)\];', line)
            if m:
                qubit_to_clbit[int(m.group(1))] = \
                    creg_offset[m.group(2)] + int(m.group(3))

        result = self._native.run(qasm2_to_qasm3(qasm2), repetitions)

        # cirq.qasm indexes qubits in sorted order
        qubit_index = {q: i for i, q in enumerate(sorted(program.all_qubits()))}

        rows: dict[str, list] = {key: [] for key in key_qubits}
        for bitstring, count in result.counts.items():
            bs = bitstring.zfill(total_clbits)
            for key, qubits in key_qubits.items():
                bits = [
                    int(bs[-1 - qubit_to_clbit[qubit_index[q]]])
                    for q in qubits
                ]
                rows[key].extend([bits] * count)

        return {
            key: np.array(r, dtype=np.uint8) for key, r in rows.items()
        }

    def __repr__(self) -> str:
        return f"<ArvakSampler('{self.name}')>"


class ArvakEngine:
    """Arvak engine for Cirq.

    Thin convenience wrapper that hands out :class:`ArvakSampler`
    instances via an Engine-shaped ``get_sampler`` call.
    """

    def __init__(self, backend_name: str = 'sim'):
        self.backend_name = backend_name
        self._sampler = ArvakSampler(backend_name)

    def get_sampler(self, processor_id: Optional[str] = None) -> ArvakSampler:
        """Get a sampler for this engine."""
        return self._sampler

    def __repr__(self) -> str:
        return f"<ArvakEngine(backend='{self.backend_name}')>"
