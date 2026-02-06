# Quick Start: HIQ Framework Integrations

This guide gets you started with HIQ's framework integration system in 5 minutes.

## Installation

### Core HIQ (No Integrations)

```bash
cd crates/hiq-python
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
import hiq

# Check available integrations
status = hiq.integration_status()
print("Available integrations:", status)

# List integrations (dict of name: available)
integrations = hiq.list_integrations()
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
import hiq
from qiskit import QuantumCircuit

# Create Qiskit circuit
qc = QuantumCircuit(2)
qc.h(0)
qc.cx(0, 1)

# Get integration
integration = hiq.get_integration('qiskit')

# Convert to HIQ
hiq_circuit = integration.to_hiq(qc)
print(f"✓ Converted: {hiq_circuit.num_qubits} qubits, depth {hiq_circuit.depth()}")

# Convert back to Qiskit
qc_back = integration.from_hiq(hiq_circuit)
print(f"✓ Round-trip: {qc_back.num_qubits} qubits")

# Use HIQ as Qiskit backend
from hiq.integrations.qiskit import HIQProvider
provider = HIQProvider()
backend = provider.get_backend('sim')
print(f"✓ Backend: {backend.name} with {backend.num_qubits} qubits")
```

### 3. Run Notebooks

```bash
# Start Jupyter
jupyter notebook notebooks/

# Open and run:
# - 01_core_hiq.ipynb (no dependencies)
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
cp -r python/hiq/integrations/qiskit python/hiq/integrations/yourframework

# 3. Edit these files:
#    - python/hiq/integrations/yourframework/__init__.py
#      → Change class name, framework_name, required_packages
#    - python/hiq/integrations/yourframework/converter.py
#      → Implement yourframework_to_hiq() and hiq_to_yourframework()
#    - python/hiq/integrations/yourframework/backend.py
#      → Update provider and backend classes

# 4. Update pyproject.toml
#    Add: yourframework = ["yourframework>=X.Y.Z"]

# 5. Test it
python3 -c "
import hiq
print(hiq.integration_status())
integration = hiq.get_integration('yourframework')
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
crates/hiq-python/
├── python/hiq/
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
│   ├── 01_core_hiq.ipynb             # Core demo
│   ├── 02_qiskit_integration.ipynb   # Qiskit demo
│   ├── templates/                     # Template for new integrations
│   └── generate_notebook.py          # Notebook generator
└── tests/
    └── integrations/
        ├── test_registry.py           # Registry tests
        └── test_yourframework.py      # Your tests
```

## Common Issues

### "Module not found: hiq"

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
- **HIQ GitHub**: https://github.com/hiq-lab/HIQ
- **Issues**: https://github.com/hiq-lab/HIQ/issues

## Getting Help

For questions or issues:
- Check the guides in `docs/` and `notebooks/`
- Search or create an issue on GitHub
- Review the example integrations in `python/hiq/integrations/qiskit/`

## Success!

If you can run this without errors, you're all set:

```python
import hiq
print("HIQ version:", hiq.__version__ if hasattr(hiq, '__version__') else "dev")
print("Available integrations:", list(hiq.list_integrations().keys()))
print("✓ HIQ integration system ready!")
```

Happy quantum computing! 🚀
