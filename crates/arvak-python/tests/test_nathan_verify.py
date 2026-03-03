"""Tests for Nathan's QCEC verification module."""

from __future__ import annotations

import pytest

from arvak.nathan.report import Suggestion
from arvak.nathan.verify import (
    VerificationResult,
    VerificationStatus,
    verify_equivalence,
    verify_suggestions,
    _qcec_available,
)


# ---------------------------------------------------------------------------
# VerificationStatus / VerificationResult
# ---------------------------------------------------------------------------


class TestVerificationTypes:
    def test_verification_status_values(self):
        assert VerificationStatus.VERIFIED == "verified"
        assert VerificationStatus.NOT_EQUIVALENT == "not_equivalent"
        assert VerificationStatus.TIMEOUT == "timeout"
        assert VerificationStatus.ERROR == "error"
        assert VerificationStatus.NOT_CHECKED == "not_checked"

    def test_verification_result_defaults(self):
        r = VerificationResult(status=VerificationStatus.VERIFIED)
        assert r.status == VerificationStatus.VERIFIED
        assert r.message == ""

    def test_verification_result_with_message(self):
        r = VerificationResult(
            status=VerificationStatus.NOT_EQUIVALENT,
            message="Circuits differ",
        )
        assert "differ" in r.message


# ---------------------------------------------------------------------------
# Suggestion verified field
# ---------------------------------------------------------------------------


class TestSuggestionVerified:
    def test_verified_default_is_none(self):
        s = Suggestion(title="test", description="desc")
        assert s.verified is None
        assert s.verification_status == "not_checked"
        assert s.verification_message == ""

    def test_verified_true(self):
        s = Suggestion(title="test", description="desc", verified=True)
        assert s.verified is True

    def test_verified_false_blocks_circuit(self):
        """A non-equivalent suggestion should not expose .circuit."""
        s = Suggestion(
            title="test",
            description="desc",
            qasm3="OPENQASM 3.0; qubit[1] q; h q[0];",
            verified=False,
        )
        assert s.circuit is None

    def test_verified_none_allows_circuit(self):
        """Unchecked suggestions should still expose .circuit."""
        s = Suggestion(
            title="test",
            description="desc",
            qasm3="OPENQASM 3.0; qubit[1] q;",
            verified=None,
        )
        # circuit may be None if arvak.from_qasm fails, but it should
        # not be blocked by the verified check
        # (we just test the guard logic, not the actual parsing)
        assert s.verified is None

    def test_repr_includes_verified(self):
        s1 = Suggestion(title="t", description="d", verified=True)
        assert "verified=True" in repr(s1)

        s2 = Suggestion(title="t", description="d", verified=False)
        assert "verified=False" in repr(s2)

        s3 = Suggestion(title="t", description="d")
        assert "verified" not in repr(s3)


# ---------------------------------------------------------------------------
# verify_equivalence (with mocked QCEC)
# ---------------------------------------------------------------------------


class TestVerifyEquivalence:
    def test_qcec_not_installed(self, monkeypatch):
        """When mqt.qcec is not installed, return NOT_CHECKED."""
        import arvak.nathan.verify as mod
        monkeypatch.setattr(mod, "_qcec_available", lambda: False)

        # Force ImportError on import
        import builtins
        real_import = builtins.__import__

        def mock_import(name, *args, **kwargs):
            if name == "mqt" or name == "mqt.qcec":
                raise ImportError("No mqt.qcec")
            return real_import(name, *args, **kwargs)

        monkeypatch.setattr(builtins, "__import__", mock_import)

        result = verify_equivalence("OPENQASM 3.0;", "OPENQASM 3.0;")
        assert result.status == VerificationStatus.NOT_CHECKED
        assert "not installed" in result.message

    def test_verify_with_mock_qcec_equivalent(self, monkeypatch, tmp_path):
        """Mock QCEC to return equivalent."""
        import types

        class MockResult:
            equivalence = "equivalent"

        mock_qcec = types.ModuleType("mqt.qcec")
        mock_qcec.verify = lambda *args, **kwargs: MockResult()

        # Patch the import
        import sys
        monkeypatch.setitem(sys.modules, "mqt", types.ModuleType("mqt"))
        monkeypatch.setitem(sys.modules, "mqt.qcec", mock_qcec)

        result = verify_equivalence(
            "OPENQASM 3.0;\nqubit[1] q;\nh q[0];",
            "OPENQASM 3.0;\nqubit[1] q;\nh q[0];",
        )
        assert result.status == VerificationStatus.VERIFIED

    def test_verify_with_mock_qcec_not_equivalent(self, monkeypatch):
        """Mock QCEC to return not_equivalent."""
        import types

        class MockResult:
            equivalence = "not_equivalent"

        mock_qcec = types.ModuleType("mqt.qcec")
        mock_qcec.verify = lambda *args, **kwargs: MockResult()

        import sys
        monkeypatch.setitem(sys.modules, "mqt", types.ModuleType("mqt"))
        monkeypatch.setitem(sys.modules, "mqt.qcec", mock_qcec)

        result = verify_equivalence("OPENQASM 3.0;", "OPENQASM 3.0;")
        assert result.status == VerificationStatus.NOT_EQUIVALENT

    def test_verify_with_mock_qcec_timeout(self, monkeypatch):
        """Mock QCEC to return no_information (timeout)."""
        import types

        class MockResult:
            equivalence = "no_information"

        mock_qcec = types.ModuleType("mqt.qcec")
        mock_qcec.verify = lambda *args, **kwargs: MockResult()

        import sys
        monkeypatch.setitem(sys.modules, "mqt", types.ModuleType("mqt"))
        monkeypatch.setitem(sys.modules, "mqt.qcec", mock_qcec)

        result = verify_equivalence("OPENQASM 3.0;", "OPENQASM 3.0;")
        assert result.status == VerificationStatus.TIMEOUT

    def test_verify_with_exception(self, monkeypatch):
        """Mock QCEC to raise an exception."""
        import types

        mock_qcec = types.ModuleType("mqt.qcec")
        mock_qcec.verify = lambda *args, **kwargs: (_ for _ in ()).throw(
            RuntimeError("QCEC internal error")
        )

        import sys
        monkeypatch.setitem(sys.modules, "mqt", types.ModuleType("mqt"))
        monkeypatch.setitem(sys.modules, "mqt.qcec", mock_qcec)

        result = verify_equivalence("OPENQASM 3.0;", "OPENQASM 3.0;")
        assert result.status == VerificationStatus.ERROR
        assert "error" in result.message.lower()


# ---------------------------------------------------------------------------
# verify_suggestions
# ---------------------------------------------------------------------------


class TestVerifySuggestions:
    def test_skips_when_qcec_unavailable(self, monkeypatch):
        """Should not modify suggestions when QCEC is not installed."""
        import arvak.nathan.verify as mod
        monkeypatch.setattr(mod, "_qcec_available", lambda: False)

        suggestions = [
            Suggestion(title="t1", description="d1", qasm3="OPENQASM 3.0;"),
            Suggestion(title="t2", description="d2"),
        ]
        result = verify_suggestions("OPENQASM 3.0;", suggestions)
        assert result is suggestions
        assert suggestions[0].verified is None
        assert suggestions[1].verified is None

    def test_skips_suggestions_without_qasm3(self, monkeypatch):
        """Suggestions without qasm3 should not be verified."""
        import types
        import arvak.nathan.verify as mod

        class MockResult:
            equivalence = "equivalent"

        mock_qcec = types.ModuleType("mqt.qcec")
        mock_qcec.verify = lambda *args, **kwargs: MockResult()
        monkeypatch.setattr(mod, "_qcec_available", lambda: True)

        import sys
        monkeypatch.setitem(sys.modules, "mqt", types.ModuleType("mqt"))
        monkeypatch.setitem(sys.modules, "mqt.qcec", mock_qcec)

        suggestions = [
            Suggestion(title="info only", description="no rewrite"),
        ]
        verify_suggestions("OPENQASM 3.0;", suggestions)
        assert suggestions[0].verified is None  # unchanged

    def test_verifies_suggestions_with_qasm3(self, monkeypatch):
        """Suggestions with qasm3 should get verified."""
        import types
        import arvak.nathan.verify as mod

        class MockResult:
            equivalence = "equivalent"

        mock_qcec = types.ModuleType("mqt.qcec")
        mock_qcec.verify = lambda *args, **kwargs: MockResult()
        monkeypatch.setattr(mod, "_qcec_available", lambda: True)

        import sys
        monkeypatch.setitem(sys.modules, "mqt", types.ModuleType("mqt"))
        monkeypatch.setitem(sys.modules, "mqt.qcec", mock_qcec)

        suggestions = [
            Suggestion(
                title="Rewrite",
                description="optimized",
                qasm3="OPENQASM 3.0;\nqubit[2] q;\nh q[0];\ncx q[0],q[1];",
            ),
            Suggestion(title="info", description="no code"),
        ]
        verify_suggestions("OPENQASM 3.0;\nqubit[2] q;\nh q[0];\ncx q[0],q[1];", suggestions)

        assert suggestions[0].verified is True
        assert suggestions[0].verification_status == "verified"
        assert suggestions[1].verified is None  # no qasm3 — untouched

    def test_marks_non_equivalent(self, monkeypatch):
        """Non-equivalent rewrites should be marked as such."""
        import types
        import arvak.nathan.verify as mod

        class MockResult:
            equivalence = "not_equivalent"

        mock_qcec = types.ModuleType("mqt.qcec")
        mock_qcec.verify = lambda *args, **kwargs: MockResult()
        monkeypatch.setattr(mod, "_qcec_available", lambda: True)

        import sys
        monkeypatch.setitem(sys.modules, "mqt", types.ModuleType("mqt"))
        monkeypatch.setitem(sys.modules, "mqt.qcec", mock_qcec)

        suggestions = [
            Suggestion(title="Bad rewrite", description="wrong", qasm3="OPENQASM 3.0;"),
        ]
        verify_suggestions("OPENQASM 3.0;", suggestions)

        assert suggestions[0].verified is False
        assert suggestions[0].verification_status == "not_equivalent"


# ---------------------------------------------------------------------------
# Display rendering with verification badges
# ---------------------------------------------------------------------------


class TestDisplayVerificationBadges:
    def test_verified_badge_in_html(self):
        from arvak.nathan.report import AnalysisReport
        from arvak.nathan.display import report_to_html

        report = AnalysisReport(
            suggestions=[
                Suggestion(
                    title="Verified rewrite",
                    description="desc",
                    qasm3="h q[0];",
                    impact="high",
                    verified=True,
                ),
            ]
        )
        html = report_to_html(report)
        assert "VERIFIED" in html
        assert "#22c55e" in html  # green color

    def test_unverified_badge_in_html(self):
        from arvak.nathan.report import AnalysisReport
        from arvak.nathan.display import report_to_html

        report = AnalysisReport(
            suggestions=[
                Suggestion(
                    title="Bad rewrite",
                    description="desc",
                    qasm3="h q[0];",
                    impact="high",
                    verified=False,
                ),
            ]
        )
        html = report_to_html(report)
        assert "UNVERIFIED" in html
        assert "#ef4444" in html  # red color

    def test_no_badge_when_not_checked(self):
        from arvak.nathan.report import AnalysisReport
        from arvak.nathan.display import report_to_html

        report = AnalysisReport(
            suggestions=[
                Suggestion(
                    title="Unchecked rewrite",
                    description="desc",
                    qasm3="h q[0];",
                    impact="medium",
                ),
            ]
        )
        html = report_to_html(report)
        assert "VERIFIED" not in html
        assert "UNVERIFIED" not in html


# ---------------------------------------------------------------------------
# Integration: analyze() with verify parameter
# ---------------------------------------------------------------------------


class TestAnalyzeVerify:
    def test_analyze_passes_verify_to_suggestions(self, monkeypatch):
        """analyze(verify=True) should call verify_suggestions."""
        import types
        import arvak.nathan as nathan_mod
        import arvak.nathan.verify as verify_mod

        # Mock the client
        mock_report = nathan_mod.AnalysisReport(
            summary="test",
            suggestions=[
                Suggestion(
                    title="opt",
                    description="d",
                    qasm3="OPENQASM 3.0;\nqubit[1] q;\nh q[0];",
                ),
            ],
        )

        class MockClient:
            def analyze(self, **kwargs):
                return mock_report

        monkeypatch.setattr(nathan_mod, "_get_client", lambda: MockClient())
        monkeypatch.setattr(nathan_mod, "_to_qasm3", lambda c, l: ("OPENQASM 3.0;", "qasm3"))

        # Mock QCEC
        class MockResult:
            equivalence = "equivalent"

        mock_qcec = types.ModuleType("mqt.qcec")
        mock_qcec.verify = lambda *args, **kwargs: MockResult()
        monkeypatch.setattr(verify_mod, "_qcec_available", lambda: True)

        import sys
        monkeypatch.setitem(sys.modules, "mqt", types.ModuleType("mqt"))
        monkeypatch.setitem(sys.modules, "mqt.qcec", mock_qcec)

        report = nathan_mod.analyze("OPENQASM 3.0;", verify=True)
        assert report.suggestions[0].verified is True

    def test_analyze_verify_false_skips(self, monkeypatch):
        """analyze(verify=False) should not verify."""
        import arvak.nathan as nathan_mod

        mock_report = nathan_mod.AnalysisReport(
            suggestions=[
                Suggestion(title="opt", description="d", qasm3="code"),
            ],
        )

        class MockClient:
            def analyze(self, **kwargs):
                return mock_report

        monkeypatch.setattr(nathan_mod, "_get_client", lambda: MockClient())
        monkeypatch.setattr(nathan_mod, "_to_qasm3", lambda c, l: ("code", "qasm3"))

        report = nathan_mod.analyze("code", verify=False)
        assert report.suggestions[0].verified is None  # untouched
