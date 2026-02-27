"""Arvak: Rust-native quantum compilation platform.

This module provides Python bindings for the Arvak quantum circuit
builder and compilation framework.

Example:
    >>> import arvak
    >>> qc = arvak.Circuit("bell", num_qubits=2)
    >>> qc.h(0).cx(0, 1)
    >>> print(arvak.to_qasm(qc))

Framework Integrations:
    >>> # Check available integrations
    >>> status = arvak.integration_status()
    >>> print(status)
    >>>
    >>> # Use Qiskit integration (if installed)
    >>> if arvak.QISKIT_AVAILABLE:
    ...     qiskit_integration = arvak.get_integration('qiskit')
    ...     arvak_circuit = qiskit_integration.to_arvak(qiskit_circuit)
"""

# Re-export everything from the native extension
from arvak._native import (
    # Core types
    Circuit,
    QubitId,
    ClbitId,
    # Compilation types
    Layout,
    CouplingMap,
    BasisGates,
    PropertySet,
    # QASM I/O
    from_qasm,
    to_qasm,
    # Simulation
    run_sim,
    # Compilation
    compile,
)

# Import integration registry
from arvak.integrations import IntegrationRegistry

# Nathan research optimizer (lazy import — only loaded when accessed)
from arvak import nathan

# Variational QUBO solvers and graph decomposition tools
from arvak import optimize

# Demo launcher
from arvak.demo import launch as demo

# Integration API
def list_integrations():
    """List all available framework integrations.

    Returns:
        Dictionary mapping framework names to availability status (True/False).

    Example:
        >>> integrations = arvak.list_integrations()
        >>> print(integrations)
        {'qiskit': True, 'qrisp': False, 'cirq': True}
    """
    return IntegrationRegistry.list_available()


def integration_status():
    """Get detailed status of all integrations.

    Returns:
        Dictionary with metadata for each integration including name,
        availability, and required packages.

    Example:
        >>> status = arvak.integration_status()
        >>> print(status['qiskit'])
        {'name': 'qiskit', 'available': True, 'packages': ['qiskit>=1.0.0']}
    """
    return IntegrationRegistry.status()


def get_integration(framework: str):
    """Get integration by framework name.

    Args:
        framework: Framework name (e.g., 'qiskit', 'qrisp', 'cirq')

    Returns:
        FrameworkIntegration instance

    Raises:
        ValueError: If framework is unknown
        ImportError: If framework is not installed

    Example:
        >>> qiskit = arvak.get_integration('qiskit')
        >>> arvak_circuit = qiskit.to_arvak(qiskit_circuit)
    """
    integration = IntegrationRegistry.get(framework)
    if integration is None:
        available = list(IntegrationRegistry._integrations.keys())
        raise ValueError(
            f"Unknown framework: {framework}. "
            f"Available integrations: {', '.join(available) if available else 'none'}"
        )
    if not integration.is_available():
        packages = ' '.join(integration.required_packages)
        raise ImportError(
            f"{framework} integration not available. "
            f"Install with: pip install {packages}"
        )
    return integration


# Convenience functions for checking integration availability
def QISKIT_AVAILABLE() -> bool:
    """Check if Qiskit integration is available."""
    return IntegrationRegistry.list_available().get('qiskit', False)


def QRISP_AVAILABLE() -> bool:
    """Check if Qrisp integration is available."""
    return IntegrationRegistry.list_available().get('qrisp', False)


def CIRQ_AVAILABLE() -> bool:
    """Check if Cirq integration is available."""
    return IntegrationRegistry.list_available().get('cirq', False)


def PENNYLANE_AVAILABLE() -> bool:
    """Check if PennyLane integration is available."""
    return IntegrationRegistry.list_available().get('pennylane', False)


# Export public API
__all__ = [
    # Core types
    "Circuit",
    "QubitId",
    "ClbitId",
    # Compilation types
    "Layout",
    "CouplingMap",
    "BasisGates",
    "PropertySet",
    # QASM I/O
    "from_qasm",
    "to_qasm",
    # Simulation
    "run_sim",
    # Compilation
    "compile",
    # Integration API
    "list_integrations",
    "integration_status",
    "get_integration",
    # Research optimizer
    "nathan",
    # Variational solvers
    "optimize",
    # Demo
    "demo",
]
