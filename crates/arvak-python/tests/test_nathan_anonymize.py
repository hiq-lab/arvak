"""Tests for Nathan code anonymization."""

import sys
import os

import pytest

# Allow importing anonymize without the full arvak native module
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python", "arvak", "nathan"))
from anonymize import anonymize_code


# ------------------------------------------------------------------ #
# QASM3 tests
# ------------------------------------------------------------------ #


class TestQasm3Anonymization:
    def test_strips_line_comments(self):
        code = """\
OPENQASM 3.0;
// This is Dr. Smith's proprietary VQE circuit for Project Alpha
include "stdgates.inc";
qubit[3] q; // secret qubit register
h q[0];
"""
        result = anonymize_code(code, "qasm3")
        assert "//" not in result
        assert "Dr. Smith" not in result
        assert "Project Alpha" not in result
        assert "secret" not in result

    def test_strips_block_comments(self):
        code = """\
OPENQASM 3.0;
/*
   Confidential: Internal algorithm for client XYZ Corp.
   Author: jane.doe@company.com
*/
qubit[2] q;
h q[0];
"""
        result = anonymize_code(code, "qasm3")
        assert "/*" not in result
        assert "*/" not in result
        assert "Confidential" not in result
        assert "jane.doe" not in result
        assert "XYZ Corp" not in result

    def test_normalizes_qubit_register_names(self):
        code = """\
OPENQASM 3.0;
qubit[3] my_secret_register;
h my_secret_register[0];
cx my_secret_register[0], my_secret_register[1];
"""
        result = anonymize_code(code, "qasm3")
        assert "my_secret_register" not in result
        assert "q0" in result
        assert "h q0[0]" in result

    def test_normalizes_bit_register_names(self):
        code = """\
OPENQASM 3.0;
qubit[2] alice_qubits;
bit[2] measurement_results;
h alice_qubits[0];
measurement_results[0] = measure alice_qubits[0];
"""
        result = anonymize_code(code, "qasm3")
        assert "alice_qubits" not in result
        assert "measurement_results" not in result

    def test_normalizes_float_variables(self):
        code = """\
OPENQASM 3.0;
qubit[2] q;
float secret_angle = 1.5707;
rz(secret_angle) q[0];
"""
        result = anonymize_code(code, "qasm3")
        assert "secret_angle" not in result
        assert "1.5707" in result
        assert "p0" in result

    def test_preserves_gate_operations(self):
        code = """\
OPENQASM 3.0;
include "stdgates.inc";
qubit[3] q;
h q[0];
cx q[0], q[1];
rz(3.14159) q[2];
measure q;
"""
        result = anonymize_code(code, "qasm3")
        assert "h " in result or "h q" in result.replace(" ", "").replace("\n", " ")
        assert "cx " in result
        assert "rz(" in result
        assert "3.14159" in result
        assert "measure" in result

    def test_preserves_openqasm_header(self):
        code = """\
OPENQASM 3.0;
include "stdgates.inc";
qubit[2] q;
"""
        result = anonymize_code(code, "qasm3")
        assert "OPENQASM 3.0" in result
        assert 'include "stdgates.inc"' in result

    def test_preserves_numeric_values(self):
        code = """\
OPENQASM 3.0;
qubit[2] q;
rz(1.5707963) q[0];
ry(0.7853981) q[1];
"""
        result = anonymize_code(code, "qasm3")
        assert "1.5707963" in result
        assert "0.7853981" in result

    def test_multiple_registers(self):
        code = """\
OPENQASM 3.0;
qubit[3] data_qubits;
qubit[1] ancilla;
bit[3] results;
h data_qubits[0];
cx data_qubits[0], ancilla[0];
"""
        result = anonymize_code(code, "qasm3")
        assert "data_qubits" not in result
        assert "ancilla" not in result
        assert "results" not in result
        # Should have q0, q1, c0
        assert "q0" in result
        assert "q1" in result
        assert "c0" in result

    def test_empty_code(self):
        assert anonymize_code("", "qasm3") == ""
        assert anonymize_code("   ", "qasm3") == "   "


# ------------------------------------------------------------------ #
# Python / Qiskit tests
# ------------------------------------------------------------------ #


class TestPythonAnonymization:
    def test_strips_comments(self):
        code = """\
# Dr. Smith's proprietary QAOA for Project Alpha
from qiskit import QuantumCircuit
qc = QuantumCircuit(3)  # secret circuit
qc.h(0)
"""
        result = anonymize_code(code, "qiskit")
        assert "#" not in result
        assert "Dr. Smith" not in result
        assert "Project Alpha" not in result
        assert "secret" not in result

    def test_strips_docstrings(self):
        code = '''\
"""
Confidential algorithm for client XYZ Corp.
Author: jane.doe@company.com
"""
from qiskit import QuantumCircuit
qc = QuantumCircuit(2)
qc.h(0)
'''
        result = anonymize_code(code, "qiskit")
        assert "Confidential" not in result
        assert "jane.doe" not in result
        assert "XYZ Corp" not in result

    def test_strips_string_literals(self):
        code = """\
from qiskit import QuantumCircuit
qc = QuantumCircuit(2, name="secret_project_alpha")
qc.h(0)
"""
        result = anonymize_code(code, "qiskit")
        assert "secret_project_alpha" not in result

    def test_normalizes_circuit_variable(self):
        code = """\
from qiskit import QuantumCircuit
my_proprietary_circuit = QuantumCircuit(3)
my_proprietary_circuit.h(0)
my_proprietary_circuit.cx(0, 1)
"""
        result = anonymize_code(code, "qiskit")
        assert "my_proprietary_circuit" not in result
        assert "qc" in result
        assert ".h(0)" in result
        assert ".cx(0, 1)" in result

    def test_normalizes_function_names(self):
        code = """\
from qiskit import QuantumCircuit

def build_proprietary_ansatz(n_qubits):
    qc = QuantumCircuit(n_qubits)
    qc.h(0)
    return qc
"""
        result = anonymize_code(code, "qiskit")
        assert "build_proprietary_ansatz" not in result
        assert "fn0" in result

    def test_normalizes_class_names(self):
        code = """\
from qiskit import QuantumCircuit

class SecretAlgorithm:
    def run(self):
        qc = QuantumCircuit(2)
        return qc
"""
        result = anonymize_code(code, "qiskit")
        assert "SecretAlgorithm" not in result
        assert "Cls0" in result

    def test_preserves_framework_imports(self):
        code = """\
from qiskit import QuantumCircuit
from qiskit.circuit import Parameter
import numpy as np
"""
        result = anonymize_code(code, "qiskit")
        assert "from qiskit import QuantumCircuit" in result
        assert "numpy" in result

    def test_strips_custom_imports(self):
        code = """\
from qiskit import QuantumCircuit
from my_secret_lib import proprietary_function
import internal_tools
qc = QuantumCircuit(2)
"""
        result = anonymize_code(code, "qiskit")
        assert "my_secret_lib" not in result
        assert "internal_tools" not in result
        assert "from qiskit import QuantumCircuit" in result

    def test_preserves_gate_calls(self):
        code = """\
from qiskit import QuantumCircuit
qc = QuantumCircuit(3)
qc.h(0)
qc.cx(0, 1)
qc.rz(3.14, 2)
qc.measure_all()
"""
        result = anonymize_code(code, "qiskit")
        assert ".h(0)" in result
        assert ".cx(0, 1)" in result
        assert ".rz(3.14, 2)" in result
        assert ".measure_all()" in result

    def test_preserves_numeric_values(self):
        code = """\
from qiskit import QuantumCircuit
import numpy as np
qc = QuantumCircuit(2)
qc.rz(np.pi / 4, 0)
qc.ry(1.5707, 1)
"""
        result = anonymize_code(code, "qiskit")
        assert "np.pi" in result
        assert "1.5707" in result


# ------------------------------------------------------------------ #
# PennyLane tests
# ------------------------------------------------------------------ #


class TestPennyLaneAnonymization:
    def test_basic_pennylane(self):
        code = """\
# My secret quantum ML model
import pennylane as qml
import numpy as np

dev = qml.device("default.qubit", wires=3)

def my_secret_classifier(weights):
    qml.AngleEmbedding(weights, wires=range(3))
    qml.StronglyEntanglingLayers(weights, wires=range(3))
    return qml.expval(qml.PauliZ(0))
"""
        result = anonymize_code(code, "pennylane")
        assert "# My secret" not in result
        assert "my_secret_classifier" not in result
        assert "qml.AngleEmbedding" in result
        assert "qml.StronglyEntanglingLayers" in result


# ------------------------------------------------------------------ #
# Cirq tests
# ------------------------------------------------------------------ #


class TestCirqAnonymization:
    def test_basic_cirq(self):
        code = """\
# Proprietary Grover's implementation for client ABC
import cirq

my_circuit = cirq.Circuit()
qubits = cirq.LineQubit.range(3)
my_circuit.append(cirq.H(qubits[0]))
my_circuit.append(cirq.CNOT(qubits[0], qubits[1]))
"""
        result = anonymize_code(code, "cirq")
        assert "Proprietary" not in result
        assert "client ABC" not in result
        assert "cirq.H" in result
        assert "cirq.CNOT" in result


# ------------------------------------------------------------------ #
# Edge cases
# ------------------------------------------------------------------ #


class TestEdgeCases:
    def test_empty_string(self):
        assert anonymize_code("", "qasm3") == ""
        assert anonymize_code("", "qiskit") == ""

    def test_whitespace_only(self):
        assert anonymize_code("   \n  ", "qasm3") == "   \n  "

    def test_unknown_language_uses_python(self):
        code = """\
# comment
x = 1
"""
        result = anonymize_code(code, "unknown")
        assert "#" not in result

    def test_preserves_circuit_structure(self):
        """Ensure the overall structure (indentation, line ordering) is maintained."""
        code = """\
OPENQASM 3.0;
include "stdgates.inc";
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
c = measure q;
"""
        result = anonymize_code(code, "qasm3")
        lines = [l for l in result.strip().split('\n') if l.strip()]
        assert lines[0].startswith("OPENQASM 3.0")
        assert "measure" in result
