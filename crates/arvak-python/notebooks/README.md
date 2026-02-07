# Arvak Jupyter Notebooks

This directory contains example notebooks demonstrating Arvak's capabilities and framework integrations.

## Available Notebooks

### Core Arvak
- **01_core_arvak.ipynb** - Introduction to Arvak's Python API without external dependencies
  - Circuit construction with fluent API
  - OpenQASM 3.0 import/export
  - Compilation configuration (coupling maps, basis gates)
  - Pre-built circuits (Bell, GHZ, QFT)

### Framework Integrations
- **02_qiskit_integration.ipynb** - Qiskit ↔ Arvak integration (requires: `pip install arvak[qiskit]`)
  - Circuit conversion between Qiskit and Arvak
  - Using Arvak as a Qiskit backend
  - Comparing compilation strategies

- **03_qrisp_integration.ipynb** - Qrisp ↔ Arvak integration (requires: `pip install arvak[qrisp]`)
  - High-level quantum programming with Qrisp
  - Converting Qrisp circuits to Arvak
  - Leveraging Arvak's compilation for Qrisp programs

- **04_cirq_integration.ipynb** - Cirq ↔ Arvak integration (requires: `pip install arvak[cirq]`)
  - Google Cirq circuit conversion
  - Using Arvak compilation with Cirq
  - Grid qubit and line qubit support

## Installation

### Core Arvak (no integrations)
```bash
cd crates/arvak-python
maturin develop
pip install jupyter matplotlib
```

### With Qiskit Integration
```bash
pip install 'arvak[qiskit,notebook]'
```

### With All Integrations
```bash
pip install 'arvak[all]'
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
mkdir -p python/arvak/integrations/yourframework
touch python/arvak/integrations/yourframework/__init__.py
touch python/arvak/integrations/yourframework/converter.py
touch python/arvak/integrations/yourframework/backend.py
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

    def to_arvak(self, circuit):
        from .converter import yourframework_to_arvak
        return yourframework_to_arvak(circuit)

    def from_arvak(self, circuit):
        from .converter import arvak_to_yourframework
        return arvak_to_yourframework(circuit)

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
def yourframework_to_arvak(circuit):
    """Convert framework circuit to Arvak via QASM3."""
    import arvak
    # Export to QASM3
    qasm_str = circuit.to_qasm3()  # Framework-specific method
    # Import to Arvak
    return arvak.from_qasm(qasm_str)

def arvak_to_yourframework(circuit):
    """Convert Arvak circuit to framework via QASM3."""
    import arvak
    import yourframework
    # Export from Arvak
    qasm_str = arvak.to_qasm(circuit)
    # Import to framework
    return yourframework.from_qasm3(qasm_str)  # Framework-specific method
```

### 4. Update pyproject.toml

```toml
[project.optional-dependencies]
yourframework = ["yourframework>=1.0.0"]
all = ["arvak[qiskit,qrisp,cirq,yourframework,notebook]"]
```

### 5. Generate Notebook

```bash
python notebooks/generate_notebook.py yourframework
```

Then fill in the generated notebook with framework-specific examples.

### 6. Test the Integration

```python
import arvak

# Check it's registered
status = arvak.integration_status()
assert 'yourframework' in status
assert status['yourframework']['available'] == True

# Test conversion
integration = arvak.get_integration('yourframework')
arvak_circuit = integration.to_arvak(your_circuit)
```

That's it! Your integration will be automatically discovered and available when users install `arvak[yourframework]`.

## Template Files

- **templates/framework_template.ipynb** - Template structure for new integration notebooks
- **generate_notebook.py** - Script to generate notebooks from template

## Support

For questions or issues with notebooks:
- GitHub Issues: https://github.com/hiq-lab/arvak/issues
- Documentation: https://github.com/hiq-lab/arvak

## Contributing

Contributions of new integration notebooks are welcome! Please follow the integration guide above and submit a PR.
