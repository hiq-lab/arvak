# HIQ Framework Integration Guide

This guide explains how to add new quantum framework integrations to HIQ, making it easy to support additional frameworks like Qrisp, Cirq, PennyLane, and more.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Quick Start](#quick-start)
- [Detailed Steps](#detailed-steps)
- [Testing](#testing)
- [Examples](#examples)
- [Best Practices](#best-practices)

## Overview

HIQ's integration system is designed to be:

- **Extensible**: Add new frameworks by implementing a simple interface
- **Modular**: Each integration is self-contained
- **Auto-discovered**: Frameworks are automatically registered when dependencies are available
- **Zero-dependency**: Core HIQ works without any framework integrations installed

### What You Get

When you add a framework integration:

1. **Bi-directional conversion**: Framework ↔ HIQ circuit conversion
2. **Backend provider**: Execute HIQ circuits through framework's API
3. **Auto-discovery**: Integration is automatically available when dependencies are installed
4. **Consistent API**: Users interact with all integrations the same way
5. **Template notebook**: Generate example notebooks automatically

## Architecture

### Directory Structure

```
crates/hiq-python/
├── python/hiq/
│   ├── __init__.py                    # Main API + integration exports
│   ├── integrations/
│   │   ├── __init__.py                # Registry + auto-discovery
│   │   ├── _base.py                   # Abstract base class
│   │   ├── qiskit/                    # Example integration
│   │   │   ├── __init__.py            # Integration class
│   │   │   ├── converter.py          # Circuit conversion
│   │   │   └── backend.py             # Backend provider
│   │   └── yourframework/             # Your new integration
│   │       ├── __init__.py
│   │       ├── converter.py
│   │       └── backend.py
├── notebooks/
│   ├── 01_core_hiq.ipynb
│   ├── 02_qiskit_integration.ipynb
│   ├── 0X_yourframework_integration.ipynb
│   └── templates/
│       └── framework_template.ipynb
└── tests/
    └── integrations/
        └── test_yourframework.py
```

### Key Components

1. **FrameworkIntegration** (`_base.py`): Abstract base class defining the integration interface
2. **IntegrationRegistry** (`integrations/__init__.py`): Auto-discovery and registration system
3. **Converter** (`converter.py`): Circuit conversion using OpenQASM 3.0
4. **Backend** (`backend.py`): Framework-specific backend provider

## Quick Start

Adding a new framework takes about 30 minutes:

```bash
# 1. Create directory structure
mkdir -p python/hiq/integrations/yourframework

# 2. Generate notebook from template
python notebooks/generate_notebook.py yourframework 03

# 3. Copy and adapt Qiskit integration
cp python/hiq/integrations/qiskit/__init__.py python/hiq/integrations/yourframework/
cp python/hiq/integrations/qiskit/converter.py python/hiq/integrations/yourframework/
cp python/hiq/integrations/qiskit/backend.py python/hiq/integrations/yourframework/

# 4. Update pyproject.toml
# Add: yourframework = ["yourframework>=X.Y.Z"]

# 5. Test it
pytest tests/integrations/test_yourframework.py
```

## Detailed Steps

### Step 1: Create Integration Module

Create the directory structure:

```bash
mkdir -p python/hiq/integrations/yourframework
touch python/hiq/integrations/yourframework/__init__.py
touch python/hiq/integrations/yourframework/converter.py
touch python/hiq/integrations/yourframework/backend.py
```

### Step 2: Implement FrameworkIntegration Class

Edit `yourframework/__init__.py`:

```python
"""YourFramework integration for HIQ."""

from typing import List
from .._base import FrameworkIntegration


class YourFrameworkIntegration(FrameworkIntegration):
    """YourFramework framework integration for HIQ."""

    @property
    def framework_name(self) -> str:
        """Name of the framework."""
        return "yourframework"

    @property
    def required_packages(self) -> List[str]:
        """Required packages for this integration."""
        return ["yourframework>=X.Y.Z"]

    def is_available(self) -> bool:
        """Check if YourFramework is installed."""
        try:
            import yourframework
            return True
        except ImportError:
            return False

    def to_hiq(self, circuit):
        """Convert YourFramework circuit to HIQ.

        Args:
            circuit: YourFramework Circuit

        Returns:
            HIQ Circuit
        """
        from .converter import yourframework_to_hiq
        return yourframework_to_hiq(circuit)

    def from_hiq(self, circuit):
        """Convert HIQ circuit to YourFramework.

        Args:
            circuit: HIQ Circuit

        Returns:
            YourFramework Circuit
        """
        from .converter import hiq_to_yourframework
        return hiq_to_yourframework(circuit)

    def get_backend_provider(self):
        """Get HIQ backend provider for YourFramework.

        Returns:
            YourFrameworkProvider instance
        """
        from .backend import YourFrameworkProvider
        return YourFrameworkProvider()


# Auto-register if available
_integration = YourFrameworkIntegration()
if _integration.is_available():
    from .. import IntegrationRegistry
    IntegrationRegistry.register(_integration)

    # Expose public API
    from .backend import YourFrameworkProvider
    from .converter import yourframework_to_hiq, hiq_to_yourframework

    __all__ = [
        'YourFrameworkProvider',
        'yourframework_to_hiq',
        'hiq_to_yourframework',
        'YourFrameworkIntegration'
    ]
else:
    __all__ = ['YourFrameworkIntegration']
```

### Step 3: Implement Converter

Edit `yourframework/converter.py`:

```python
"""YourFramework circuit conversion utilities."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from yourframework import Circuit
    import hiq


def yourframework_to_hiq(circuit: 'Circuit') -> 'hiq.Circuit':
    """Convert YourFramework circuit to HIQ via OpenQASM 3.0.

    Args:
        circuit: YourFramework Circuit instance

    Returns:
        HIQ Circuit instance

    Raises:
        ImportError: If yourframework is not installed
        ValueError: If circuit cannot be converted
    """
    try:
        import yourframework
    except ImportError:
        raise ImportError(
            "YourFramework is required for this operation. "
            "Install with: pip install yourframework>=X.Y.Z"
        )

    import hiq

    # Convert to QASM3
    # Adapt this to your framework's QASM export method
    qasm_str = circuit.to_qasm3()  # or circuit.qasm() or dumps(circuit)

    # Import into HIQ
    hiq_circuit = hiq.from_qasm(qasm_str)

    return hiq_circuit


def hiq_to_yourframework(circuit: 'hiq.Circuit') -> 'Circuit':
    """Convert HIQ circuit to YourFramework via OpenQASM 3.0.

    Args:
        circuit: HIQ Circuit instance

    Returns:
        YourFramework Circuit instance

    Raises:
        ImportError: If yourframework is not installed
        ValueError: If circuit cannot be converted
    """
    try:
        import yourframework
    except ImportError:
        raise ImportError(
            "YourFramework is required for this operation. "
            "Install with: pip install yourframework>=X.Y.Z"
        )

    import hiq

    # Export HIQ to QASM3
    qasm_str = hiq.to_qasm(circuit)

    # Import into YourFramework
    # Adapt this to your framework's QASM import method
    yourframework_circuit = yourframework.from_qasm3(qasm_str)

    return yourframework_circuit
```

### Step 4: Implement Backend Provider

Edit `yourframework/backend.py`:

```python
"""YourFramework backend provider for HIQ."""

from typing import List, Optional, Union, TYPE_CHECKING
import warnings

if TYPE_CHECKING:
    from yourframework import Circuit


class YourFrameworkProvider:
    """YourFramework provider for HIQ backends.

    This provider allows users to access HIQ execution capabilities through
    YourFramework's standard provider interface.
    """

    def __init__(self):
        """Initialize the HIQ provider."""
        self._backends = {}

    def backends(self, name: Optional[str] = None, **filters) -> List:
        """Get list of available backends.

        Args:
            name: Optional backend name filter
            **filters: Additional filters

        Returns:
            List of HIQBackend instances
        """
        if not self._backends:
            self._backends = {
                'sim': HIQSimulatorBackend(provider=self),
            }

        if name:
            backend = self._backends.get(name)
            return [backend] if backend else []

        return list(self._backends.values())

    def get_backend(self, name: str = 'sim'):
        """Get a specific backend by name.

        Args:
            name: Backend name (default: 'sim')

        Returns:
            HIQBackend instance

        Raises:
            ValueError: If backend name is unknown
        """
        backends = self.backends(name=name)
        if not backends:
            available = list(self._backends.keys())
            raise ValueError(
                f"Unknown backend: {name}. "
                f"Available backends: {', '.join(available)}"
            )
        return backends[0]


class HIQSimulatorBackend:
    """HIQ simulator backend with YourFramework-compatible interface."""

    def __init__(self, provider: YourFrameworkProvider):
        """Initialize the simulator backend.

        Args:
            provider: Parent YourFrameworkProvider instance
        """
        self._provider = provider
        self.name = 'hiq_simulator'
        self.description = 'HIQ quantum circuit simulator'

    @property
    def num_qubits(self) -> int:
        """Number of qubits supported."""
        return 32

    def run(self, circuits: Union['Circuit', List['Circuit']],
            shots: int = 1024, **options):
        """Run circuits on the simulator.

        Args:
            circuits: Single circuit or list of circuits
            shots: Number of shots
            **options: Additional options

        Returns:
            Job instance
        """
        warnings.warn(
            "HIQ backend execution through YourFramework is not yet fully implemented. "
            "Use HIQ CLI for execution: 'hiq run circuit.qasm --backend sim --shots 1000'",
            RuntimeWarning
        )

        # Convert circuits to HIQ format
        from .converter import yourframework_to_hiq

        if not isinstance(circuits, list):
            circuits = [circuits]

        hiq_circuits = [yourframework_to_hiq(qc) for qc in circuits]

        # Create mock job (replace with actual execution)
        job = HIQJob(
            backend=self,
            circuits=hiq_circuits,
            shots=shots,
            options=options
        )

        return job


class HIQJob:
    """Mock job for HIQ backend execution."""

    def __init__(self, backend, circuits, shots, options):
        self._backend = backend
        self._circuits = circuits
        self._shots = shots
        self._options = options
        self._result = None

    def result(self):
        """Get job result."""
        if self._result is None:
            self._result = HIQResult(
                backend_name=self._backend.name,
                circuits=self._circuits,
                shots=self._shots
            )
        return self._result

    def status(self) -> str:
        """Get job status."""
        return "DONE"


class HIQResult:
    """Mock result for HIQ backend execution."""

    def __init__(self, backend_name, circuits, shots):
        self.backend_name = backend_name
        self._circuits = circuits
        self._shots = shots

    def get_counts(self, circuit=None):
        """Get measurement counts."""
        warnings.warn(
            "Returning mock results. Use HIQ CLI for actual execution.",
            RuntimeWarning
        )

        # Return mock Bell state results
        return {
            '00': self._shots // 2,
            '11': self._shots // 2,
        }
```

### Step 5: Update pyproject.toml

Add your framework to the optional dependencies:

```toml
[project.optional-dependencies]
yourframework = ["yourframework>=X.Y.Z", "additional-dependency>=A.B.C"]
all = ["hiq-quantum[qiskit,qrisp,cirq,yourframework,notebook]"]
```

### Step 6: Generate Notebook

```bash
python notebooks/generate_notebook.py yourframework 03
```

Then edit `notebooks/03_yourframework_integration.ipynb` to fill in framework-specific examples.

### Step 7: Add Tests

Create `tests/integrations/test_yourframework.py`:

```python
"""Tests for YourFramework integration."""

import pytest

try:
    import hiq
    import yourframework
    YOURFRAMEWORK_AVAILABLE = True
except ImportError:
    YOURFRAMEWORK_AVAILABLE = False

pytestmark = pytest.mark.skipif(
    not YOURFRAMEWORK_AVAILABLE,
    reason="YourFramework not installed"
)


def test_integration_registered():
    """Test that YourFramework integration is registered."""
    status = hiq.integration_status()
    assert 'yourframework' in status
    assert status['yourframework']['available'] is True


def test_yourframework_to_hiq():
    """Test converting YourFramework circuit to HIQ."""
    # Create YourFramework circuit
    circuit = yourframework.Circuit(2)
    circuit.h(0)
    circuit.cx(0, 1)

    # Convert to HIQ
    integration = hiq.get_integration('yourframework')
    hiq_circuit = integration.to_hiq(circuit)

    assert hiq_circuit.num_qubits == 2


def test_hiq_to_yourframework():
    """Test converting HIQ circuit to YourFramework."""
    hiq_circuit = hiq.Circuit.bell()

    integration = hiq.get_integration('yourframework')
    framework_circuit = integration.from_hiq(hiq_circuit)

    assert framework_circuit is not None


def test_backend_provider():
    """Test getting backend provider."""
    integration = hiq.get_integration('yourframework')
    provider = integration.get_backend_provider()

    assert provider is not None
    backends = provider.backends()
    assert len(backends) > 0
```

## Testing

### Run Tests

```bash
# Test registry system
pytest tests/integrations/test_registry.py -v

# Test your integration (requires framework installed)
pytest tests/integrations/test_yourframework.py -v

# Test without framework (should skip gracefully)
pip uninstall yourframework -y
pytest tests/integrations/test_yourframework.py -v
```

### Manual Testing

```python
import hiq

# Check integration status
status = hiq.integration_status()
print(status)

# Get integration
integration = hiq.get_integration('yourframework')
print(integration)

# Test conversion
import yourframework
circuit = yourframework.Circuit(2)
circuit.h(0)
circuit.cx(0, 1)

hiq_circuit = integration.to_hiq(circuit)
print(f"Converted: {hiq_circuit.num_qubits} qubits")

# Test backend
provider = integration.get_backend_provider()
backend = provider.get_backend('sim')
print(f"Backend: {backend.name}")
```

## Examples

### Minimal Integration (Qrisp)

See `python/hiq/integrations/qiskit/` for a complete working example.

Key points:
- OpenQASM 3.0 as interchange format
- Auto-registration when dependencies available
- Mock backend until HIQ execution is exposed to Python

## Best Practices

### 1. Use OpenQASM 3.0 for Conversion

This ensures compatibility and reduces framework-specific code:

```python
# Export to QASM
qasm_str = framework_circuit.to_qasm3()

# Import from QASM
framework_circuit = Framework.from_qasm3(qasm_str)
```

### 2. Handle Missing Dependencies Gracefully

```python
def is_available(self) -> bool:
    try:
        import yourframework
        return True
    except ImportError:
        return False
```

### 3. Provide Clear Error Messages

```python
if not integration.is_available():
    raise ImportError(
        "YourFramework integration not available. "
        "Install with: pip install yourframework>=X.Y.Z"
    )
```

### 4. Add Type Hints

```python
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from yourframework import Circuit
    import hiq

def to_hiq(circuit: 'Circuit') -> 'hiq.Circuit':
    ...
```

### 5. Test Optional Dependencies

Your tests should pass whether or not the framework is installed:

```python
pytestmark = pytest.mark.skipif(
    not FRAMEWORK_AVAILABLE,
    reason="Framework not installed"
)
```

## FAQ

### Q: What if my framework doesn't support OpenQASM 3.0?

A: You can implement custom gate-by-gate conversion, but QASM is recommended for maintainability. If the framework supports QASM 2.0, you can convert to QASM 3.0.

### Q: How do I handle framework-specific gates?

A: Either decompose them into standard gates before conversion, or extend HIQ's gate set to include them.

### Q: Can I add hardware backends?

A: Yes! Just extend the backend provider to support additional backend names. The actual execution will happen through HIQ CLI or when HIQ execution is exposed to Python.

### Q: Do I need to modify core HIQ code?

A: No! Integrations are completely self-contained and auto-discovered.

### Q: How do I test my integration?

A: Create tests in `tests/integrations/test_yourframework.py` and use `pytest.mark.skipif` to skip when dependencies aren't available.

## Contributing

To contribute a new integration:

1. Follow this guide to implement the integration
2. Add comprehensive tests
3. Generate and fill in the notebook
4. Submit a PR with:
   - Integration code
   - Tests
   - Notebook
   - Updated pyproject.toml

## Resources

- HIQ GitHub: https://github.com/hiq-lab/HIQ
- OpenQASM 3.0 Spec: https://openqasm.com/
- Example integrations: `python/hiq/integrations/qiskit/`

## Support

For questions or issues:
- GitHub Issues: https://github.com/hiq-lab/HIQ/issues
- Discussions: https://github.com/hiq-lab/HIQ/discussions
