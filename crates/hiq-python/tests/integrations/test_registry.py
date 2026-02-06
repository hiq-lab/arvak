"""Tests for the integration registry system."""

import pytest
import hiq
from hiq.integrations import IntegrationRegistry, FrameworkIntegration
from typing import List, Any


class MockIntegration(FrameworkIntegration):
    """Mock integration for testing."""

    def __init__(self, name: str, available: bool = True):
        self._name = name
        self._available = available

    @property
    def framework_name(self) -> str:
        return self._name

    @property
    def required_packages(self) -> List[str]:
        return [f"{self._name}>=1.0.0"]

    def is_available(self) -> bool:
        return self._available

    def to_hiq(self, circuit: Any) -> 'hiq.Circuit':
        """Mock conversion."""
        return hiq.Circuit.bell()

    def from_hiq(self, circuit: 'hiq.Circuit') -> Any:
        """Mock conversion."""
        return "mock_circuit"

    def get_backend_provider(self) -> Any:
        """Mock provider."""
        return "mock_provider"


class TestIntegrationRegistry:
    """Tests for IntegrationRegistry."""

    def setup_method(self):
        """Clear registry before each test."""
        IntegrationRegistry.clear()

    def teardown_method(self):
        """Clear registry after each test."""
        IntegrationRegistry.clear()

    def test_register_integration(self):
        """Test registering an integration."""
        mock = MockIntegration("test_framework")
        IntegrationRegistry.register(mock)

        assert "test_framework" in IntegrationRegistry._integrations
        assert IntegrationRegistry.get("test_framework") is mock

    def test_get_integration(self):
        """Test retrieving an integration."""
        mock = MockIntegration("test_framework")
        IntegrationRegistry.register(mock)

        integration = IntegrationRegistry.get("test_framework")
        assert integration is not None
        assert integration.framework_name == "test_framework"

    def test_get_nonexistent_integration(self):
        """Test retrieving a non-existent integration."""
        integration = IntegrationRegistry.get("nonexistent")
        assert integration is None

    def test_list_available(self):
        """Test listing available integrations."""
        mock1 = MockIntegration("available", available=True)
        mock2 = MockIntegration("unavailable", available=False)

        IntegrationRegistry.register(mock1)
        IntegrationRegistry.register(mock2)

        available = IntegrationRegistry.list_available()
        assert available["available"] is True
        assert available["unavailable"] is False

    def test_status(self):
        """Test getting detailed status."""
        mock = MockIntegration("test_framework")
        IntegrationRegistry.register(mock)

        status = IntegrationRegistry.status()
        assert "test_framework" in status
        assert status["test_framework"]["name"] == "test_framework"
        assert status["test_framework"]["available"] is True
        assert status["test_framework"]["packages"] == ["test_framework>=1.0.0"]

    def test_clear(self):
        """Test clearing the registry."""
        mock = MockIntegration("test_framework")
        IntegrationRegistry.register(mock)
        assert len(IntegrationRegistry._integrations) > 0

        IntegrationRegistry.clear()
        assert len(IntegrationRegistry._integrations) == 0


class TestHIQIntegrationAPI:
    """Tests for HIQ's public integration API."""

    def setup_method(self):
        """Setup test fixtures."""
        IntegrationRegistry.clear()
        self.mock = MockIntegration("test_framework", available=True)
        IntegrationRegistry.register(self.mock)

    def teardown_method(self):
        """Cleanup."""
        IntegrationRegistry.clear()

    def test_list_integrations(self):
        """Test hiq.list_integrations()."""
        integrations = hiq.list_integrations()
        assert isinstance(integrations, dict)
        assert "test_framework" in integrations
        assert integrations["test_framework"] is True

    def test_integration_status(self):
        """Test hiq.integration_status()."""
        status = hiq.integration_status()
        assert isinstance(status, dict)
        assert "test_framework" in status
        assert status["test_framework"]["available"] is True

    def test_get_integration_success(self):
        """Test hiq.get_integration() with available framework."""
        integration = hiq.get_integration("test_framework")
        assert integration is self.mock

    def test_get_integration_unknown(self):
        """Test hiq.get_integration() with unknown framework."""
        with pytest.raises(ValueError, match="Unknown framework"):
            hiq.get_integration("nonexistent")

    def test_get_integration_unavailable(self):
        """Test hiq.get_integration() with unavailable framework."""
        unavailable = MockIntegration("unavailable", available=False)
        IntegrationRegistry.register(unavailable)

        with pytest.raises(ImportError, match="not available"):
            hiq.get_integration("unavailable")


class TestFrameworkIntegration:
    """Tests for FrameworkIntegration base class."""

    def test_metadata(self):
        """Test integration metadata."""
        mock = MockIntegration("test_framework")
        metadata = mock.metadata()

        assert metadata["name"] == "test_framework"
        assert metadata["available"] is True
        assert metadata["packages"] == ["test_framework>=1.0.0"]

    def test_repr(self):
        """Test string representation."""
        mock = MockIntegration("test_framework", available=True)
        repr_str = repr(mock)

        assert "MockIntegration" in repr_str
        assert "test_framework" in repr_str
        assert "available" in repr_str

    def test_repr_unavailable(self):
        """Test string representation for unavailable integration."""
        mock = MockIntegration("test_framework", available=False)
        repr_str = repr(mock)

        assert "not installed" in repr_str


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
