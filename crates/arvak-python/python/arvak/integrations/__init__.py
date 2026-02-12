"""Framework integrations for Arvak.

This package provides optional integrations with popular quantum computing
frameworks like Qiskit, Qrisp, and Cirq. Integrations are automatically
discovered and registered when their dependencies are available.

Usage:
    >>> import arvak
    >>> # Check available integrations
    >>> print(arvak.integration_status())
    >>>
    >>> # Get specific integration
    >>> qiskit = arvak.get_integration('qiskit')
    >>> arvak_circuit = qiskit.to_arvak(qiskit_circuit)

Adding new integrations:
    1. Create a new directory: integrations/yourframework/
    2. Implement FrameworkIntegration in yourframework/__init__.py
    3. Add to pyproject.toml optional-dependencies
    4. The integration will be auto-discovered on import
"""

from typing import Any, Optional
from ._base import FrameworkIntegration

__all__ = ['FrameworkIntegration', 'IntegrationRegistry']


class IntegrationRegistry:
    """Global registry for framework integrations.

    This registry maintains all available framework integrations and provides
    methods to query their status and retrieve them by name.
    """

    _integrations: dict[str, FrameworkIntegration] = {}

    @classmethod
    def register(cls, integration: FrameworkIntegration) -> None:
        """Register a framework integration.

        Args:
            integration: FrameworkIntegration instance to register
        """
        cls._integrations[integration.framework_name] = integration

    @classmethod
    def get(cls, name: str) -> Optional[FrameworkIntegration]:
        """Get integration by framework name.

        Args:
            name: Framework name (e.g., 'qiskit', 'qrisp')

        Returns:
            FrameworkIntegration instance or None if not found
        """
        return cls._integrations.get(name)

    @classmethod
    def list_available(cls) -> dict[str, bool]:
        """List all integrations and their availability status.

        Returns:
            Dictionary mapping framework names to availability (True/False)
        """
        return {
            name: integration.is_available()
            for name, integration in cls._integrations.items()
        }

    @classmethod
    def status(cls) -> dict[str, Any]:
        """Get detailed status of all integrations.

        Returns:
            Dictionary with metadata for each integration including:
            - name: framework name
            - available: installation status
            - packages: required packages
        """
        return {
            name: integration.metadata()
            for name, integration in cls._integrations.items()
        }

    @classmethod
    def clear(cls) -> None:
        """Clear all registered integrations (mainly for testing)."""
        cls._integrations.clear()


def _discover_integrations() -> None:
    """Automatically discover and register available integrations.

    This function is called on module import to find all integration modules
    in the integrations/ directory and attempt to import them. If an integration's
    dependencies are not available, the import will fail silently.
    """
    import importlib
    import pkgutil

    # Iterate through all modules in this package
    for finder, name, ispkg in pkgutil.iter_modules(__path__):
        # Skip private modules (like _base)
        if name.startswith('_'):
            continue

        try:
            # Try to import the integration module
            # If successful, it should auto-register itself
            importlib.import_module(f'.{name}', __package__)
        except ImportError:
            # Integration not available (missing dependencies)
            # This is expected and not an error
            pass
        except (AttributeError, TypeError, ValueError, RuntimeError) as e:
            # Non-import errors indicate a problem with the integration module
            import warnings
            warnings.warn(
                f"Failed to load integration '{name}': {e}",
                RuntimeWarning
            )


# Auto-discover integrations on module import
_discover_integrations()
