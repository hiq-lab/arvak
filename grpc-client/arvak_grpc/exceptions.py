"""Exceptions for the Arvak gRPC client."""


class ArvakError(Exception):
    """Base exception for Arvak errors."""
    pass


class ArvakJobNotFoundError(ArvakError):
    """Raised when a job is not found."""
    pass


class ArvakBackendNotFoundError(ArvakError):
    """Raised when a backend is not found."""
    pass


class ArvakInvalidCircuitError(ArvakError):
    """Raised when a circuit is invalid."""
    pass


class ArvakJobNotCompletedError(ArvakError):
    """Raised when attempting to get results for a non-completed job."""
    pass
