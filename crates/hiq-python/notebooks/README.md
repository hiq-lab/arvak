# HIQ Jupyter Notebooks

This directory contains example notebooks demonstrating HIQ's capabilities and framework integrations.

## Available Notebooks

### Core HIQ
- **01_core_hiq.ipynb** - Introduction to HIQ's Python API without external dependencies
  - Circuit construction with fluent API
  - OpenQASM 3.0 import/export
  - Compilation configuration (coupling maps, basis gates)
  - Pre-built circuits (Bell, GHZ, QFT)

### Framework Integrations
- **02_qiskit_integration.ipynb** - Qiskit ↔ HIQ integration (requires: `pip install hiq-quantum[qiskit]`)
  - Circuit conversion between Qiskit and HIQ
  - Using HIQ as a Qiskit backend
  - Comparing compilation strategies

- **03_qrisp_integration.ipynb** - Qrisp ↔ HIQ integration (requires: `pip install hiq-quantum[qrisp]`)
  - High-level quantum programming with Qrisp
  - Converting Qrisp circuits to HIQ
  - Leveraging HIQ's compilation for Qrisp programs

- **04_cirq_integration.ipynb** - Cirq ↔ HIQ integration (requires: `pip install hiq-quantum[cirq]`)
  - Google Cirq circuit conversion
  - Using HIQ compilation with Cirq
  - Grid qubit and line qubit support

## Installation

### Core HIQ (no integrations)
```bash
cd crates/hiq-python
maturin develop
pip install jupyter matplotlib
```

### With Qiskit Integration
```bash
pip install 'hiq-quantum[qiskit,notebook]'
```

### With All Integrations
```bash
pip install 'hiq-quantum[all]'
```

## Running Notebooks

```bash
jupyter notebook notebooks/
```

Or with JupyterLab:
```bash
jupyter lab notebooks/
```

## Adding a New Framework Integration

Want to add support for a new quantum framework? Follow these steps:

### 1. Create Integration Module

```bash
mkdir -p python/hiq/integrations/yourframework
touch python/hiq/integrations/yourframework/__init__.py
touch python/hiq/integrations/yourframework/converter.py
touch python/hiq/integrations/yourframework/backend.py
```

### 2. Implement FrameworkIntegration

Copy the structure from `qiskit/__init__.py`:

```python
from .._base import FrameworkIntegration

class YourFrameworkIntegration(FrameworkIntegration):
    @property
    def framework_name(self) -> str:
        return "yourframework"

    @property
    def required_packages(self) -> List[str]:
        return ["yourframework>=1.0.0"]

    def is_available(self) -> bool:
        try:
            import yourframework
            return True
        except ImportError:
            return False

    def to_hiq(self, circuit):
        from .converter import yourframework_to_hiq
        return yourframework_to_hiq(circuit)

    def from_hiq(self, circuit):
        from .converter import hiq_to_yourframework
        return hiq_to_yourframework(circuit)

    def get_backend_provider(self):
        from .backend import YourFrameworkProvider
        return YourFrameworkProvider()

# Auto-register
_integration = YourFrameworkIntegration()
if _integration.is_available():
    from .. import IntegrationRegistry
    IntegrationRegistry.register(_integration)
```

### 3. Implement Converter

In `converter.py`, implement conversion using OpenQASM 3.0:

```python
def yourframework_to_hiq(circuit):
    """Convert framework circuit to HIQ via QASM3."""
    import hiq
    # Export to QASM3
    qasm_str = circuit.to_qasm3()  # Framework-specific method
    # Import to HIQ
    return hiq.from_qasm(qasm_str)

def hiq_to_yourframework(circuit):
    """Convert HIQ circuit to framework via QASM3."""
    import hiq
    import yourframework
    # Export from HIQ
    qasm_str = hiq.to_qasm(circuit)
    # Import to framework
    return yourframework.from_qasm3(qasm_str)  # Framework-specific method
```

### 4. Update pyproject.toml

```toml
[project.optional-dependencies]
yourframework = ["yourframework>=1.0.0"]
all = ["hiq-quantum[qiskit,qrisp,cirq,yourframework,notebook]"]
```

### 5. Generate Notebook

```bash
python notebooks/generate_notebook.py yourframework
```

Then fill in the generated notebook with framework-specific examples.

### 6. Test the Integration

```python
import hiq

# Check it's registered
status = hiq.integration_status()
assert 'yourframework' in status
assert status['yourframework']['available'] == True

# Test conversion
integration = hiq.get_integration('yourframework')
hiq_circuit = integration.to_hiq(your_circuit)
```

That's it! Your integration will be automatically discovered and available when users install `hiq-quantum[yourframework]`.

## Template Files

- **templates/framework_template.ipynb** - Template structure for new integration notebooks
- **generate_notebook.py** - Script to generate notebooks from template

## Support

For questions or issues with notebooks:
- GitHub Issues: https://github.com/hiq-lab/HIQ/issues
- Documentation: https://github.com/hiq-lab/HIQ

## Contributing

Contributions of new integration notebooks are welcome! Please follow the integration guide above and submit a PR.
