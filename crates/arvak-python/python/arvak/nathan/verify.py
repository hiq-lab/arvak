"""Circuit equivalence verification via MQT QCEC.

Verifies that Nathan's suggested QASM3 rewrites preserve circuit
semantics using the MQT Quantum Circuit Equivalence Checker.

Requires: ``pip install mqt.qcec``
"""

from __future__ import annotations

import logging
import tempfile
import os
from dataclasses import dataclass
from enum import Enum

logger = logging.getLogger(__name__)


class VerificationStatus(str, Enum):
    """Result of a circuit equivalence check."""

    VERIFIED = "verified"
    NOT_EQUIVALENT = "not_equivalent"
    TIMEOUT = "timeout"
    ERROR = "error"
    NOT_CHECKED = "not_checked"


@dataclass
class VerificationResult:
    """Detailed result of a QCEC equivalence check."""

    status: VerificationStatus
    message: str = ""


def _qcec_available() -> bool:
    """Check if mqt.qcec is installed."""
    try:
        import mqt.qcec  # noqa: F401
        return True
    except ImportError:
        return False


def verify_equivalence(
    original_qasm: str,
    rewrite_qasm: str,
    timeout: float = 30.0,
) -> VerificationResult:
    """Verify that two QASM circuits are functionally equivalent.

    Uses MQT QCEC's decision-diagram-based equivalence checker to prove
    (or disprove) that ``original_qasm`` and ``rewrite_qasm`` compute
    the same unitary.

    Args:
        original_qasm: The original circuit as a QASM string.
        rewrite_qasm: The suggested rewrite as a QASM string.
        timeout: Maximum seconds for the verification (default: 30).

    Returns:
        VerificationResult with status and optional message.
    """
    try:
        from mqt import qcec
    except ImportError:
        return VerificationResult(
            status=VerificationStatus.NOT_CHECKED,
            message="mqt.qcec not installed (pip install mqt.qcec)",
        )

    # QCEC needs file paths — write both circuits to temp files
    orig_fd, orig_path = tempfile.mkstemp(suffix=".qasm")
    rewrite_fd, rewrite_path = tempfile.mkstemp(suffix=".qasm")
    try:
        with os.fdopen(orig_fd, "w") as f:
            f.write(original_qasm)
        with os.fdopen(rewrite_fd, "w") as f:
            f.write(rewrite_qasm)

        result = qcec.verify(
            orig_path,
            rewrite_path,
            timeout=timeout,
        )

        equivalence = str(result.equivalence)

        if "equivalent" in equivalence.lower() and "not" not in equivalence.lower():
            return VerificationResult(
                status=VerificationStatus.VERIFIED,
                message="Circuits are equivalent (QCEC verified)",
            )
        elif "not_equivalent" in equivalence.lower() or "not equivalent" in equivalence.lower():
            return VerificationResult(
                status=VerificationStatus.NOT_EQUIVALENT,
                message=f"Circuits are NOT equivalent: {equivalence}",
            )
        elif "timeout" in equivalence.lower() or "no_information" in equivalence.lower():
            return VerificationResult(
                status=VerificationStatus.TIMEOUT,
                message=f"Verification inconclusive (timeout or too complex): {equivalence}",
            )
        else:
            return VerificationResult(
                status=VerificationStatus.NOT_CHECKED,
                message=f"Unknown QCEC result: {equivalence}",
            )

    except Exception as e:
        logger.warning("QCEC verification failed: %s", e)
        return VerificationResult(
            status=VerificationStatus.ERROR,
            message=f"Verification error: {e}",
        )
    finally:
        # Clean up temp files
        for path in (orig_path, rewrite_path):
            try:
                os.unlink(path)
            except OSError:
                pass


def verify_suggestions(
    original_qasm: str,
    suggestions: list,
    timeout: float = 30.0,
) -> list:
    """Verify all suggestions that contain QASM3 rewrites.

    For each suggestion with a non-empty ``qasm3`` field, runs QCEC
    verification against the original circuit.  Sets the ``verified``
    and ``verification_message`` fields on each suggestion in-place.

    Args:
        original_qasm: The original circuit QASM string.
        suggestions: List of Suggestion objects to verify.
        timeout: Timeout per verification (default: 30s).

    Returns:
        The same list of suggestions (modified in-place).
    """
    if not _qcec_available():
        logger.debug("mqt.qcec not available — skipping suggestion verification")
        return suggestions

    for s in suggestions:
        if not s.qasm3:
            # No rewrite to verify
            continue

        result = verify_equivalence(original_qasm, s.qasm3, timeout=timeout)
        s.verified = result.status == VerificationStatus.VERIFIED
        s.verification_status = result.status.value
        s.verification_message = result.message

    return suggestions
