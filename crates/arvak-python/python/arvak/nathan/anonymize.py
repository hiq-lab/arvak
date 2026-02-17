"""Code anonymization for Nathan — strips PII and proprietary details before sending to LLM.

Removes comments, normalizes variable names, strips string literals, while preserving
gate operations, circuit structure, and numeric values needed for analysis.
"""

from __future__ import annotations

import re

# Standard library imports that should be preserved (not stripped)
_STANDARD_IMPORTS = frozenset({
    "stdgates.inc", "qelib1.inc",
    "qiskit", "cirq", "pennylane", "numpy", "np", "math", "cmath",
    "qiskit.circuit", "qiskit.quantum_info", "qiskit.primitives",
    "qiskit.transpiler", "qiskit.providers",
    "pennylane.numpy", "pennylane.templates",
    "cirq.ops", "cirq.circuits",
})

# QASM3 keywords and built-in gate names that should never be renamed
_QASM3_KEYWORDS = frozenset({
    "OPENQASM", "include", "qubit", "bit", "int", "uint", "float", "bool",
    "angle", "duration", "stretch", "complex", "const", "mutable",
    "gate", "def", "defcal", "cal", "extern", "if", "else", "for", "while",
    "in", "return", "break", "continue", "end", "switch", "case", "default",
    "measure", "reset", "barrier", "delay", "box", "let", "input", "output",
    "creg", "qreg", "true", "false", "pi", "euler", "tau",
})

_QASM3_GATES = frozenset({
    "h", "x", "y", "z", "s", "t", "sdg", "tdg", "sx", "sxdg",
    "cx", "cy", "cz", "ch", "cs", "ct", "swap", "iswap", "cswap",
    "ccx", "ccz", "rx", "ry", "rz", "rxx", "ryy", "rzz", "rzx",
    "p", "cp", "crx", "cry", "crz", "u", "u1", "u2", "u3", "cu", "cu1", "cu3",
    "id", "ecr", "dcx", "xx_plus_yy", "xx_minus_yy",
    "r", "gpi", "gpi2", "ms", "prx", "cz",
})

# Python keywords and builtins to preserve
_PYTHON_KEYWORDS = frozenset({
    "False", "None", "True", "and", "as", "assert", "async", "await",
    "break", "class", "continue", "def", "del", "elif", "else", "except",
    "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
    "try", "while", "with", "yield",
    "print", "range", "len", "list", "dict", "set", "tuple", "int", "float",
    "str", "bool", "type", "super", "self", "cls", "enumerate", "zip", "map",
    "filter", "sorted", "reversed", "abs", "min", "max", "sum", "any", "all",
    "isinstance", "issubclass", "hasattr", "getattr", "setattr",
})

# Framework API names to preserve in Python quantum code
_FRAMEWORK_NAMES = frozenset({
    # Qiskit
    "QuantumCircuit", "QuantumRegister", "ClassicalRegister", "AncillaRegister",
    "Aer", "execute", "transpile", "assemble",
    "Sampler", "Estimator", "StatevectorSimulator",
    "qiskit", "qc",
    # PennyLane
    "qml", "qnode", "device", "QNode",
    "AngleEmbedding", "StronglyEntanglingLayers", "BasicEntanglerLayers",
    # Cirq
    "cirq", "Circuit", "LineQubit", "GridQubit", "NamedQubit",
    "Simulator", "DensityMatrixSimulator",
    "moment", "Moment",
    # Common gate method names
    "h", "x", "y", "z", "s", "t", "cx", "cz", "swap", "measure",
    "rx", "ry", "rz", "rxx", "ryy", "rzz", "p", "cp", "ccx", "u",
    "append", "add", "compose",
    # NumPy
    "np", "numpy", "pi", "array", "zeros", "ones", "eye", "linspace",
    "arange", "sqrt", "exp", "sin", "cos", "tan",
})


def anonymize_code(code: str, language: str) -> str:
    """Anonymize quantum code by stripping PII and normalizing names.

    Args:
        code: Source code string.
        language: One of "qasm3", "qiskit", "pennylane", "cirq", "python".

    Returns:
        Anonymized code string safe to send to LLM.
    """
    if not code or not code.strip():
        return code

    if language == "qasm3":
        return _anonymize_qasm3(code)
    else:
        return _anonymize_python(code)


# --------------------------------------------------------------------------- #
# QASM3 anonymization
# --------------------------------------------------------------------------- #

def _anonymize_qasm3(code: str) -> str:
    """Anonymize QASM3 code."""
    # Step 1: Strip block comments /* ... */
    code = re.sub(r'/\*.*?\*/', '', code, flags=re.DOTALL)

    # Step 2: Strip line comments // ...
    code = re.sub(r'//[^\n]*', '', code)

    # Step 3: Normalize register and variable names
    code = _normalize_qasm3_names(code)

    # Step 4: Clean up blank lines
    lines = [line for line in code.split('\n') if line.strip()]
    return '\n'.join(lines) + '\n'


def _normalize_qasm3_names(code: str) -> str:
    """Normalize user-defined names in QASM3 to generic identifiers."""
    name_map: dict[str, str] = {}
    counters = {"q": 0, "c": 0, "p": 0, "g": 0, "v": 0}

    def get_replacement(name: str, category: str) -> str:
        if name in name_map:
            return name_map[name]
        prefix = category
        idx = counters[category]
        counters[category] += 1
        replacement = f"{prefix}{idx}"
        name_map[name] = replacement
        return replacement

    # Normalize qubit registers: qubit[N] name or qubit name
    def replace_qubit_reg(m: re.Match) -> str:
        decl = m.group(1)  # "qubit[N]" or "qubit"
        name = m.group(2)
        return f"{decl} {get_replacement(name, 'q')}"

    code = re.sub(
        r'(qubit(?:\[\d+\])?)\s+([a-zA-Z_]\w*)',
        replace_qubit_reg,
        code,
    )

    # Normalize bit/creg registers: bit[N] name, creg name
    # Negative lookbehind prevents matching "bit" inside "qubit"
    def replace_bit_reg(m: re.Match) -> str:
        decl = m.group(1)
        name = m.group(2)
        return f"{decl} {get_replacement(name, 'c')}"

    code = re.sub(
        r'(?<![a-zA-Z_])((?:bit|creg)(?:\[\d+\])?)\s+([a-zA-Z_]\w*)',
        replace_bit_reg,
        code,
    )

    # Normalize qreg: qreg name[N]
    def replace_qreg(m: re.Match) -> str:
        name = m.group(1)
        size = m.group(2)
        return f"qreg {get_replacement(name, 'q')}[{size}]"

    code = re.sub(
        r'qreg\s+([a-zA-Z_]\w*)\[(\d+)\]',
        replace_qreg,
        code,
    )

    # Normalize float/angle/int variables: float[N] name = ... or float name = ...
    def replace_var(m: re.Match) -> str:
        decl = m.group(1)
        name = m.group(2)
        rest = m.group(3)
        return f"{decl} {get_replacement(name, 'p')}{rest}"

    code = re.sub(
        r'((?:float|angle|int|uint)(?:\[\d+\])?)\s+([a-zA-Z_]\w*)((?:\s*=)?)',
        replace_var,
        code,
    )

    # Now replace all occurrences of mapped names in the rest of the code
    # Sort by length (longest first) to avoid partial replacements
    if name_map:
        pattern = '|'.join(
            re.escape(name) for name in sorted(name_map, key=len, reverse=True)
        )
        # Only replace whole words
        code = re.sub(
            rf'\b({pattern})\b',
            lambda m: name_map.get(m.group(1), m.group(1)),
            code,
        )

    return code


# --------------------------------------------------------------------------- #
# Python (Qiskit / PennyLane / Cirq) anonymization
# --------------------------------------------------------------------------- #

def _anonymize_python(code: str) -> str:
    """Anonymize Python quantum code (Qiskit, PennyLane, Cirq)."""
    # Step 1: Strip docstrings (triple-quoted)
    code = re.sub(r'""".*?"""', '""', code, flags=re.DOTALL)
    code = re.sub(r"'''.*?'''", '""', code, flags=re.DOTALL)

    # Step 2: Strip comments
    code = re.sub(r'#[^\n]*', '', code)

    # Step 3: Replace non-empty string literals (preserve empty strings)
    # Handle single and double quoted strings, but not triple-quoted (already handled)
    code = _replace_string_literals(code)

    # Step 4: Strip custom imports (keep standard ones)
    code = _filter_imports(code)

    # Step 5: Normalize user-defined names
    code = _normalize_python_names(code)

    # Step 6: Clean up blank lines
    lines = [line for line in code.split('\n') if line.strip()]
    return '\n'.join(lines) + '\n'


def _replace_string_literals(code: str) -> str:
    """Replace string literal contents with empty strings."""
    result = []
    i = 0
    while i < len(code):
        # Check for string start
        if code[i] in ('"', "'"):
            quote = code[i]
            # Not triple-quote (already stripped)
            if code[i:i+3] in ('"""', "'''"):
                result.append('""')
                # Skip to end of triple-quote
                end = code.find(code[i:i+3], i + 3)
                i = end + 3 if end != -1 else len(code)
            else:
                result.append('""')
                # Skip to end of single-line string
                i += 1
                while i < len(code) and code[i] != quote:
                    if code[i] == '\\' and i + 1 < len(code):
                        i += 2  # Skip escaped character
                    else:
                        i += 1
                if i < len(code):
                    i += 1  # Skip closing quote
        else:
            result.append(code[i])
            i += 1
    return ''.join(result)


def _filter_imports(code: str) -> str:
    """Keep standard library imports, strip custom ones."""
    lines = code.split('\n')
    result = []
    for line in lines:
        stripped = line.strip()
        if stripped.startswith(('import ', 'from ')):
            # Extract the module name
            if stripped.startswith('from '):
                module = stripped.split()[1] if len(stripped.split()) > 1 else ""
            else:
                module = stripped.split()[1] if len(stripped.split()) > 1 else ""
            # Remove trailing comma/as clauses for checking
            module = module.split('.')[0]
            if module in _STANDARD_IMPORTS or module in (
                "qiskit", "cirq", "pennylane", "numpy", "np", "math", "cmath",
                "matplotlib", "scipy",
            ):
                result.append(line)
            # else: strip the import
        else:
            result.append(line)
    return '\n'.join(result)


def _normalize_python_names(code: str) -> str:
    """Normalize user-defined variable and function names in Python quantum code."""
    name_map: dict[str, str] = {}
    counters = {"qc": 0, "fn": 0, "var": 0, "cls": 0}

    # Find circuit variable assignments: name = QuantumCircuit(...), cirq.Circuit(...), etc.
    circuit_constructors = re.findall(
        r'([a-zA-Z_]\w*)\s*=\s*(?:QuantumCircuit|cirq\.Circuit|qml\.device|'
        r'QuantumRegister|ClassicalRegister)',
        code,
    )
    for name in circuit_constructors:
        if name not in _FRAMEWORK_NAMES and name not in _PYTHON_KEYWORDS:
            if name not in name_map:
                idx = counters["qc"]
                counters["qc"] += 1
                name_map[name] = f"qc{idx}" if idx > 0 else "qc"

    # Find function definitions: def name(...)
    func_defs = re.findall(r'def\s+([a-zA-Z_]\w*)\s*\(', code)
    for name in func_defs:
        if name not in _FRAMEWORK_NAMES and name not in _PYTHON_KEYWORDS:
            if name not in name_map:
                idx = counters["fn"]
                counters["fn"] += 1
                name_map[name] = f"fn{idx}"

    # Find class definitions: class Name(...)
    class_defs = re.findall(r'class\s+([a-zA-Z_]\w*)', code)
    for name in class_defs:
        if name not in _FRAMEWORK_NAMES and name not in _PYTHON_KEYWORDS:
            if name not in name_map:
                idx = counters["cls"]
                counters["cls"] += 1
                name_map[name] = f"Cls{idx}"

    # Replace all mapped names
    if name_map:
        pattern = '|'.join(
            re.escape(name) for name in sorted(name_map, key=len, reverse=True)
        )
        code = re.sub(
            rf'\b({pattern})\b',
            lambda m: name_map.get(m.group(1), m.group(1)),
            code,
        )

    return code
