"""Base classes for framework integrations.

This module defines the abstract interfaces that all framework integrations
must implement to provide consistent conversion and execution capabilities.
"""

from abc import ABC, abstractmethod
from typing import Any, Dict, List, TYPE_CHECKING

if TYPE_CHECKING:
    import arvak


class FrameworkIntegration(ABC):
    """Base class for all framework integrations.

    Each framework integration (Qiskit, Qrisp, Cirq, etc.) must inherit from
    this class and implement all abstract methods to enable:
    - Circuit conversion to/from Arvak
    - Backend execution through the framework's native API
    - Automatic discovery and registration

    Example:
        >>> class QiskitIntegration(FrameworkIntegration):
        ...     @property
        ...     def framework_name(self) -> str:
        ...         return "qiskit"
        ...
        ...     def is_available(self) -> bool:
        ...         try:
        ...             import qiskit
        ...             return True
        ...         except ImportError:
        ...             return False
    """

    @property
    @abstractmethod
    def framework_name(self) -> str:
        """Name of the framework (e.g., 'qiskit', 'qrisp', 'cirq')."""
        pass

    @property
    @abstractmethod
    def required_packages(self) -> List[str]:
        """List of required package names for this integration.

        Returns:
            List of pip package specifiers (e.g., ["qiskit>=1.0.0"])
        """
        pass

    @abstractmethod
    def is_available(self) -> bool:
        """Check if the framework is installed and importable.

        Returns:
            True if the framework can be imported, False otherwise.
        """
        pass

    @abstractmethod
    def to_arvak(self, circuit: Any) -> 'arvak.Circuit':
        """Convert a framework circuit to Arvak.

        Args:
            circuit: Circuit object from the framework

        Returns:
            Arvak Circuit object

        Raises:
            ImportError: If framework is not available
            ValueError: If circuit cannot be converted
        """
        pass

    @abstractmethod
    def from_arvak(self, circuit: 'arvak.Circuit') -> Any:
        """Convert Arvak circuit to framework format.

        Args:
            circuit: Arvak Circuit object

        Returns:
            Circuit object in the framework's native format

        Raises:
            ImportError: If framework is not available
            ValueError: If circuit cannot be converted
        """
        pass

    @abstractmethod
    def get_backend_provider(self) -> Any:
        """Return framework-specific backend provider.

        This allows users to execute Arvak circuits through the framework's
        native backend API (e.g., Qiskit's backend.run()).

        Returns:
            Backend provider object for the framework

        Raises:
            ImportError: If framework is not available
        """
        pass

    def metadata(self) -> Dict[str, Any]:
        """Return integration metadata.

        Returns:
            Dictionary with integration information:
            - name: framework name
            - available: whether framework is installed
            - packages: required package list
        """
        return {
            "name": self.framework_name,
            "available": self.is_available(),
            "packages": self.required_packages,
        }

    def __repr__(self) -> str:
        """String representation of the integration."""
        status = "available" if self.is_available() else "not installed"
        return f"<{self.__class__.__name__} ({self.framework_name}): {status}>"
