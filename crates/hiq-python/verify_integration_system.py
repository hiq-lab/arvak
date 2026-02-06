#!/usr/bin/env python3
"""Verification script for HIQ integration system.

This script tests the core integration infrastructure without requiring
any optional framework dependencies.
"""

import sys
import os

# Add python directory to path for testing without installation
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'python'))


def test_imports():
    """Test that all core modules can be imported."""
    print("Testing imports...")

    try:
        import hiq
        print("  ✓ hiq imported")
    except ImportError as e:
        print(f"  ✗ Failed to import hiq: {e}")
        return False

    try:
        from hiq.integrations import IntegrationRegistry, FrameworkIntegration
        print("  ✓ Integration registry imported")
    except ImportError as e:
        print(f"  ✗ Failed to import integration registry: {e}")
        return False

    try:
        from hiq.integrations._base import FrameworkIntegration
        print("  ✓ Base integration class imported")
    except ImportError as e:
        print(f"  ✗ Failed to import base class: {e}")
        return False

    return True


def test_api():
    """Test the public API functions."""
    print("\nTesting public API...")

    import hiq

    # Test list_integrations
    try:
        integrations = hiq.list_integrations()
        print(f"  ✓ list_integrations() returned: {integrations}")
    except Exception as e:
        print(f"  ✗ list_integrations() failed: {e}")
        return False

    # Test integration_status
    try:
        status = hiq.integration_status()
        print(f"  ✓ integration_status() returned: {status}")
    except Exception as e:
        print(f"  ✗ integration_status() failed: {e}")
        return False

    # Test get_integration with unknown framework
    try:
        hiq.get_integration('nonexistent_framework')
        print("  ✗ get_integration() should have raised ValueError")
        return False
    except ValueError as e:
        print(f"  ✓ get_integration() correctly raised ValueError for unknown framework")
    except Exception as e:
        print(f"  ✗ get_integration() raised unexpected exception: {e}")
        return False

    return True


def test_registry():
    """Test the integration registry."""
    print("\nTesting integration registry...")

    from hiq.integrations import IntegrationRegistry, FrameworkIntegration
    from typing import List, Any

    # Create a mock integration
    class TestIntegration(FrameworkIntegration):
        @property
        def framework_name(self) -> str:
            return "test_framework"

        @property
        def required_packages(self) -> List[str]:
            return ["test_package>=1.0.0"]

        def is_available(self) -> bool:
            return True

        def to_hiq(self, circuit: Any):
            return None

        def from_hiq(self, circuit):
            return None

        def get_backend_provider(self):
            return None

    # Clear registry
    original_integrations = IntegrationRegistry._integrations.copy()
    IntegrationRegistry.clear()

    try:
        # Test registration
        test_integration = TestIntegration()
        IntegrationRegistry.register(test_integration)
        print("  ✓ Integration registered")

        # Test retrieval
        retrieved = IntegrationRegistry.get("test_framework")
        if retrieved is test_integration:
            print("  ✓ Integration retrieved correctly")
        else:
            print("  ✗ Retrieved integration doesn't match")
            return False

        # Test list_available
        available = IntegrationRegistry.list_available()
        if "test_framework" in available and available["test_framework"] is True:
            print("  ✓ Integration listed as available")
        else:
            print("  ✗ Integration not listed correctly")
            return False

        # Test status
        status = IntegrationRegistry.status()
        if "test_framework" in status:
            info = status["test_framework"]
            if (info["name"] == "test_framework" and
                info["available"] is True and
                info["packages"] == ["test_package>=1.0.0"]):
                print("  ✓ Integration status correct")
            else:
                print(f"  ✗ Integration status incorrect: {info}")
                return False
        else:
            print("  ✗ Integration not in status")
            return False

        return True

    finally:
        # Restore original registry
        IntegrationRegistry.clear()
        for name, integration in original_integrations.items():
            IntegrationRegistry.register(integration)


def test_qiskit_integration():
    """Test Qiskit integration if available."""
    print("\nTesting Qiskit integration...")

    try:
        import qiskit
        qiskit_available = True
        print("  ℹ Qiskit is installed")
    except ImportError:
        qiskit_available = False
        print("  ℹ Qiskit not installed (skipping integration tests)")
        return True

    if not qiskit_available:
        return True

    import hiq

    # Check if integration is registered
    status = hiq.integration_status()
    if 'qiskit' not in status:
        print("  ✗ Qiskit integration not registered")
        return False

    if not status['qiskit']['available']:
        print("  ✗ Qiskit integration marked as unavailable")
        return False

    print("  ✓ Qiskit integration registered and available")

    # Get integration
    try:
        integration = hiq.get_integration('qiskit')
        print(f"  ✓ Retrieved Qiskit integration: {integration}")
    except Exception as e:
        print(f"  ✗ Failed to get Qiskit integration: {e}")
        return False

    # Test backend provider
    try:
        provider = integration.get_backend_provider()
        print(f"  ✓ Got backend provider: {provider}")
    except Exception as e:
        print(f"  ✗ Failed to get backend provider: {e}")
        return False

    return True


def test_file_structure():
    """Verify that all expected files exist."""
    print("\nVerifying file structure...")

    base_dir = os.path.dirname(__file__)

    expected_files = [
        # Core integration files
        "python/hiq/integrations/__init__.py",
        "python/hiq/integrations/_base.py",

        # Qiskit integration
        "python/hiq/integrations/qiskit/__init__.py",
        "python/hiq/integrations/qiskit/converter.py",
        "python/hiq/integrations/qiskit/backend.py",

        # Qrisp integration
        "python/hiq/integrations/qrisp/__init__.py",
        "python/hiq/integrations/qrisp/converter.py",
        "python/hiq/integrations/qrisp/backend.py",

        # Cirq integration
        "python/hiq/integrations/cirq/__init__.py",
        "python/hiq/integrations/cirq/converter.py",
        "python/hiq/integrations/cirq/backend.py",

        # PennyLane integration
        "python/hiq/integrations/pennylane/__init__.py",
        "python/hiq/integrations/pennylane/converter.py",
        "python/hiq/integrations/pennylane/backend.py",

        # Notebooks
        "notebooks/README.md",
        "notebooks/01_core_hiq.ipynb",
        "notebooks/02_qiskit_integration.ipynb",
        "notebooks/03_qrisp_integration.ipynb",
        "notebooks/04_cirq_integration.ipynb",
        "notebooks/05_pennylane_integration.ipynb",
        "notebooks/generate_notebook.py",
        "notebooks/templates/framework_template.ipynb",

        # Tests
        "tests/integrations/__init__.py",
        "tests/integrations/test_registry.py",
        "tests/integrations/test_qiskit.py",
        "tests/integrations/test_qrisp.py",
        "tests/integrations/test_cirq.py",

        # Documentation
        "docs/INTEGRATION_GUIDE.md",
        "QUICKSTART_INTEGRATIONS.md",
        "notebooks/IMPLEMENTATION_SUMMARY.md",
    ]

    all_exist = True
    for file_path in expected_files:
        full_path = os.path.join(base_dir, file_path)
        if os.path.exists(full_path):
            print(f"  ✓ {file_path}")
        else:
            print(f"  ✗ Missing: {file_path}")
            all_exist = False

    return all_exist


def main():
    """Run all verification tests."""
    print("=" * 70)
    print("HIQ Integration System Verification")
    print("=" * 70)

    tests = [
        ("Imports", test_imports),
        ("Public API", test_api),
        ("Integration Registry", test_registry),
        ("Qiskit Integration", test_qiskit_integration),
        ("File Structure", test_file_structure),
    ]

    results = {}
    for name, test_func in tests:
        try:
            results[name] = test_func()
        except Exception as e:
            print(f"\n✗ {name} test crashed: {e}")
            import traceback
            traceback.print_exc()
            results[name] = False

    # Summary
    print("\n" + "=" * 70)
    print("Summary")
    print("=" * 70)

    for name, passed in results.items():
        status = "✓ PASS" if passed else "✗ FAIL"
        print(f"{status}: {name}")

    all_passed = all(results.values())

    if all_passed:
        print("\n🎉 All tests passed! Integration system is working correctly.")
        return 0
    else:
        print("\n⚠️  Some tests failed. Please review the output above.")
        return 1


if __name__ == "__main__":
    sys.exit(main())
