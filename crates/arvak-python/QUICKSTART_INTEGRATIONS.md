# Quick Start: Arvak Framework Integrations

This guide gets you started with Arvak's framework integration system in 5 minutes.

## Installation

### Core Arvak (No Integrations)

```bash
cd crates/arvak-python
maturin develop
pip install jupyter matplotlib
```

### With Qiskit Integration

```bash
pip install qiskit qiskit-aer
maturin develop
```

### With All Integrations (Future)

```bash
pip install qiskit qrisp cirq jupyter matplotlib
maturin develop
```

## Quick Test

### 1. Test Core Integration System

```python
import arvak

# Check available integrations
status = arvak.integration_status()
print("Available integrations:", status)

# List integrations (dict of name: available)
integrations = arvak.list_integrations()
print("Integration list:", integrations)
```

**Expected Output** (no integrations installed):
```python
Available integrations: {}
Integration list: {}
```

**Expected Output** (with Qiskit):
```python
Available integrations: {
    'qiskit': {
        'name': 'qiskit',
        'available': True,
        'packages': ['qiskit>=1.0.0']
    }
}
Integration list: {'qiskit': True}
```

### 2. Test Qiskit Integration (If Installed)

```python
import arvak
from qiskit import QuantumCircuit

# Create Qiskit circuit
qc = QuantumCircuit(2)
qc.h(0)
qc.cx(0, 1)

# Get integration
integration = arvak.get_integration('qiskit')

# Convert to Arvak
arvak_circuit = integration.to_arvak(qc)
print(f"✓ Converted: {arvak_circuit.num_qubits} qubits, depth {arvak_circuit.depth()}")

# Convert back to Qiskit
qc_back = integration.from_arvak(arvak_circuit)
print(f"✓ Round-trip: {qc_back.num_qubits} qubits")

# Use Arvak as Qiskit backend
from arvak.integrations.qiskit import ArvakProvider
provider = ArvakProvider()
backend = provider.get_backend('sim')
print(f"✓ Backend: {backend.name} with {backend.num_qubits} qubits")
```

### 3. Run Notebooks

```bash
# Start Jupyter
jupyter notebook notebooks/

# Open and run:
# - 01_core_arvak.ipynb (no dependencies)
# - 02_qiskit_integration.ipynb (requires Qiskit)
```

### 4. Run Tests

```bash
# Test registry system (no dependencies required)
PYTHONPATH=python python3 -m pytest tests/integrations/test_registry.py -v

# Test Qiskit integration (requires Qiskit)
PYTHONPATH=python python3 -m pytest tests/integrations/test_qiskit.py -v
```

## Adding a New Framework

### Super Quick (< 30 minutes)

```bash
# 1. Generate notebook from template
python notebooks/generate_notebook.py yourframework 03

# 2. Copy Qiskit integration as starting point
cp -r python/arvak/integrations/qiskit python/arvak/integrations/yourframework

# 3. Edit these files:
#    - python/arvak/integrations/yourframework/__init__.py
#      → Change class name, framework_name, required_packages
#    - python/arvak/integrations/yourframework/converter.py
#      → Implement yourframework_to_arvak() and arvak_to_yourframework()
#    - python/arvak/integrations/yourframework/backend.py
#      → Update provider and backend classes

# 4. Update pyproject.toml
#    Add: yourframework = ["yourframework>=X.Y.Z"]

# 5. Test it
python3 -c "
import arvak
print(arvak.integration_status())
integration = arvak.get_integration('yourframework')
print(f'✓ {integration.framework_name} integration works!')
"
```

### Key Points

1. **OpenQASM 3.0**: Use QASM as interchange format for conversion
2. **Auto-Registration**: Integration registers itself on import
3. **Optional Dependencies**: Framework is only imported if available
4. **Template-Driven**: Follow the established pattern

## File Structure

```
crates/arvak-python/
├── python/arvak/
│   ├── __init__.py                    # Integration API
│   └── integrations/
│       ├── _base.py                   # Abstract base class
│       ├── __init__.py                # Registry
│       ├── qiskit/                    # Qiskit integration
│       │   ├── __init__.py
│       │   ├── converter.py
│       │   └── backend.py
│       └── yourframework/             # Your integration
│           ├── __init__.py
│           ├── converter.py
│           └── backend.py
├── notebooks/
│   ├── 01_core_arvak.ipynb             # Core demo
│   ├── 02_qiskit_integration.ipynb   # Qiskit demo
│   ├── templates/                     # Template for new integrations
│   └── generate_notebook.py          # Notebook generator
└── tests/
    └── integrations/
        ├── test_registry.py           # Registry tests
        └── test_yourframework.py      # Your tests
```

## Common Issues

### "Module not found: arvak"

**Solution**: Build the package first:
```bash
maturin develop
```

Or use PYTHONPATH:
```bash
PYTHONPATH=python python3 your_script.py
```

### "Integration not available"

**Solution**: Install the framework:
```bash
pip install qiskit  # or qrisp, cirq, etc.
```

### "Tests are skipped"

This is expected! Tests skip gracefully when dependencies aren't installed:
```python
pytestmark = pytest.mark.skipif(
    not FRAMEWORK_AVAILABLE,
    reason="Framework not installed"
)
```

## Next Steps

1. **Read the Guide**: See `docs/INTEGRATION_GUIDE.md` for detailed instructions
2. **Explore Notebooks**: Check out `notebooks/README.md` for examples
3. **Run Tests**: Verify everything works with `pytest`
4. **Add a Framework**: Follow the guide to add your favorite framework

## Resources

- **Integration Guide**: `docs/INTEGRATION_GUIDE.md`
- **Notebook Guide**: `notebooks/README.md`
- **Implementation Summary**: `notebooks/IMPLEMENTATION_SUMMARY.md`
- **Arvak GitHub**: https://github.com/hiq-lab/arvak
- **Issues**: https://github.com/hiq-lab/arvak/issues

## Getting Help

For questions or issues:
- Check the guides in `docs/` and `notebooks/`
- Search or create an issue on GitHub
- Review the example integrations in `python/arvak/integrations/qiskit/`

## Success!

If you can run this without errors, you're all set:

```python
import arvak
print("Arvak version:", arvak.__version__ if hasattr(arvak, '__version__') else "dev")
print("Available integrations:", list(arvak.list_integrations().keys()))
print("✓ Arvak integration system ready!")
```

Happy quantum computing! 🚀
