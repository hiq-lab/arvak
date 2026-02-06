"""HIQ: Rust-native quantum compilation platform.

This module provides Python bindings for the HIQ quantum circuit
builder and compilation framework.

Example:
    >>> import hiq
    >>> qc = hiq.Circuit("bell", num_qubits=2)
    >>> qc.h(0).cx(0, 1)
    >>> print(hiq.to_qasm(qc))

Framework Integrations:
    >>> # Check available integrations
    >>> status = hiq.integration_status()
    >>> print(status)
    >>>
    >>> # Use Qiskit integration (if installed)
    >>> if hiq.QISKIT_AVAILABLE:
    ...     qiskit_integration = hiq.get_integration('qiskit')
    ...     hiq_circuit = qiskit_integration.to_hiq(qiskit_circuit)
"""

# Re-export everything from the native extension
from hiq.hiq import (
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
)

# Import integration registry
from hiq.integrations import IntegrationRegistry

# Integration API
def list_integrations():
    """List all available framework integrations.

    Returns:
        Dictionary mapping framework names to availability status (True/False).

    Example:
        >>> integrations = hiq.list_integrations()
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
        >>> status = hiq.integration_status()
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
        >>> qiskit = hiq.get_integration('qiskit')
        >>> hiq_circuit = qiskit.to_hiq(qiskit_circuit)
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


# Convenience properties for checking integration availability
@property
def QISKIT_AVAILABLE():
    """Check if Qiskit integration is available."""
    return IntegrationRegistry.list_available().get('qiskit', False)


@property
def QRISP_AVAILABLE():
    """Check if Qrisp integration is available."""
    return IntegrationRegistry.list_available().get('qrisp', False)


@property
def CIRQ_AVAILABLE():
    """Check if Cirq integration is available."""
    return IntegrationRegistry.list_available().get('cirq', False)


# Note: Properties need to be accessed as functions in Python
# e.g., hiq.QISKIT_AVAILABLE() instead of hiq.QISKIT_AVAILABLE
# So we provide both property and direct boolean access
def _check_availability(framework: str) -> bool:
    """Internal helper to check framework availability."""
    return IntegrationRegistry.list_available().get(framework, False)


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
    # Integration API
    "list_integrations",
    "integration_status",
    "get_integration",
]
