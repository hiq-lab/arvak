"""Tests for the Nathan research optimizer integration.

These tests validate the arvak.nathan module: imports, dataclasses,
circuit-to-QASM3 conversion routing, display rendering, and client
parsing. API calls are mocked (no live Nathan service required).
"""

import json
from unittest.mock import MagicMock, patch

import pytest

import arvak
from arvak.nathan import (
    AnalysisReport,
    ChatResponse,
    Paper,
    Suggestion,
    analyze,
    chat,
    configure,
)
from arvak.nathan.report import CircuitStats
from arvak.nathan.display import report_to_html, chat_to_html, _esc, _markdown_to_html
from arvak.nathan.client import NathanClient


# ---------------------------------------------------------------------------
# Module import tests
# ---------------------------------------------------------------------------

class TestNathanImport:
    """Tests that arvak.nathan is importable and exposes the public API."""

    def test_nathan_module_exists(self):
        """arvak.nathan should be importable."""
        assert hasattr(arvak, "nathan")

    def test_analyze_function_exists(self):
        """arvak.nathan.analyze should be callable."""
        assert callable(arvak.nathan.analyze)

    def test_chat_function_exists(self):
        """arvak.nathan.chat should be callable."""
        assert callable(arvak.nathan.chat)

    def test_configure_function_exists(self):
        """arvak.nathan.configure should be callable."""
        assert callable(arvak.nathan.configure)

    def test_public_exports(self):
        """All documented public names should be in __all__."""
        from arvak.nathan import __all__

        expected = {"analyze", "chat", "configure", "AnalysisReport",
                    "ChatResponse", "Paper", "Suggestion"}
        assert expected.issubset(set(__all__))


# ---------------------------------------------------------------------------
# Dataclass tests
# ---------------------------------------------------------------------------

class TestPaper:
    """Tests for the Paper dataclass."""

    def test_paper_creation(self):
        p = Paper(title="QAOA for MaxCut", arxiv_url="https://arxiv.org/abs/2301.00000")
        assert p.title == "QAOA for MaxCut"
        assert p.arxiv_url == "https://arxiv.org/abs/2301.00000"
        assert p.problem_type == ""
        assert p.algorithm == ""

    def test_paper_with_all_fields(self):
        p = Paper(
            title="VQE Improvements",
            arxiv_url="https://arxiv.org/abs/2302.00000",
            problem_type="vqe",
            algorithm="vqe",
            relevance="2x speedup vs classical",
        )
        assert p.problem_type == "vqe"
        assert p.relevance == "2x speedup vs classical"

    def test_paper_repr(self):
        p = Paper(title="Test", arxiv_url="https://example.com")
        r = repr(p)
        assert "Test" in r
        assert "example.com" in r


class TestSuggestion:
    """Tests for the Suggestion dataclass."""

    def test_suggestion_creation(self):
        s = Suggestion(title="Reduce depth", description="Use ZZ decomposition")
        assert s.title == "Reduce depth"
        assert s.qasm3 == ""
        assert s.impact == ""

    def test_suggestion_with_qasm(self):
        s = Suggestion(
            title="Optimized ansatz",
            description="Use hardware-efficient ansatz",
            qasm3='OPENQASM 3.0;\nqubit[2] q;\nh q[0];',
            impact="high",
        )
        assert s.qasm3.startswith("OPENQASM")
        assert s.impact == "high"

    def test_suggestion_circuit_property_without_qasm(self):
        s = Suggestion(title="Info", description="No code")
        assert s.circuit is None

    def test_suggestion_repr(self):
        s = Suggestion(title="Test", description="Desc", impact="medium")
        r = repr(s)
        assert "Test" in r
        assert "medium" in r


class TestCircuitStats:
    """Tests for the CircuitStats dataclass."""

    def test_circuit_stats_creation(self):
        cs = CircuitStats(
            num_qubits=4,
            total_gates=12,
            gate_breakdown="h: 4, cx: 4, rz: 4",
            depth=8,
            detected_pattern="qaoa",
        )
        assert cs.num_qubits == 4
        assert cs.total_gates == 12
        assert cs.detected_pattern == "qaoa"

    def test_circuit_stats_defaults(self):
        cs = CircuitStats()
        assert cs.num_qubits == 0
        assert cs.language == "qasm3"
        assert cs.detected_pattern == "unknown"

    def test_circuit_stats_repr(self):
        cs = CircuitStats(num_qubits=2, total_gates=3, depth=2, detected_pattern="vqe")
        r = repr(cs)
        assert "qubits=2" in r
        assert "vqe" in r


class TestAnalysisReport:
    """Tests for the AnalysisReport dataclass."""

    def test_report_creation(self):
        report = AnalysisReport(
            summary="Test summary",
            problem_type="qaoa",
            suitability=0.72,
            recommended_algorithm="qaoa",
            estimated_qubits=4,
        )
        assert report.problem_type == "qaoa"
        assert report.suitability == 0.72
        assert report.papers == []
        assert report.suggestions == []

    def test_report_with_papers_and_suggestions(self):
        report = AnalysisReport(
            summary="Full report",
            problem_type="vqe",
            suitability=0.8,
            papers=[Paper(title="P1", arxiv_url="https://arxiv.org/abs/1")],
            suggestions=[Suggestion(title="S1", description="Do this")],
        )
        assert len(report.papers) == 1
        assert len(report.suggestions) == 1

    def test_report_defaults(self):
        report = AnalysisReport()
        assert report.problem_type == "unknown"
        assert report.suitability == 0.0
        assert report.recommended_shots == 1024

    def test_report_repr(self):
        report = AnalysisReport(
            problem_type="grover",
            suitability=0.5,
            papers=[Paper(title="P1", arxiv_url="u")],
            suggestions=[],
        )
        r = repr(report)
        assert "grover" in r
        assert "50.0%" in r
        assert "papers=1" in r

    def test_report_repr_html(self):
        report = AnalysisReport(
            summary="Test",
            problem_type="qaoa",
            suitability=0.65,
            circuit=CircuitStats(num_qubits=4, total_gates=12, depth=8),
        )
        html = report._repr_html_()
        assert "Nathan Analysis" in html
        assert "qaoa" in html
        assert "65%" in html


class TestChatResponse:
    """Tests for the ChatResponse dataclass."""

    def test_chat_response_creation(self):
        cr = ChatResponse(
            message="QAOA is great for MaxCut.",
            papers=[Paper(title="P1", arxiv_url="https://arxiv.org/abs/1")],
        )
        assert "QAOA" in cr.message
        assert len(cr.papers) == 1

    def test_chat_response_repr_html(self):
        cr = ChatResponse(message="Hello from Nathan")
        html = cr._repr_html_()
        assert "Nathan" in html
        assert "Hello from Nathan" in html


# ---------------------------------------------------------------------------
# Display rendering tests
# ---------------------------------------------------------------------------

class TestDisplay:
    """Tests for rich HTML rendering."""

    def test_escape_html(self):
        assert _esc("<script>") == "&lt;script&gt;"
        assert _esc('a "b" c') == "a &quot;b&quot; c"
        assert _esc("a & b") == "a &amp; b"

    def test_markdown_to_html_bold(self):
        html = _markdown_to_html("This is **bold** text")
        assert "<strong>bold</strong>" in html

    def test_markdown_to_html_code(self):
        html = _markdown_to_html("Use `h q[0]` gate")
        assert "<code" in html
        assert "h q[0]" in html

    def test_markdown_to_html_header(self):
        html = _markdown_to_html("## Section Title")
        assert "Section Title" in html

    def test_report_to_html_with_papers(self):
        report = AnalysisReport(
            problem_type="qaoa",
            suitability=0.7,
            papers=[
                Paper(title="QAOA Paper", arxiv_url="https://arxiv.org/abs/1234"),
            ],
        )
        html = report_to_html(report)
        assert "QAOA Paper" in html
        assert "arxiv.org" in html

    def test_report_to_html_with_suggestions(self):
        report = AnalysisReport(
            problem_type="vqe",
            suitability=0.8,
            suggestions=[
                Suggestion(
                    title="Use UCCSD",
                    description="Better ansatz",
                    qasm3="h q[0];",
                    impact="high",
                ),
            ],
        )
        html = report_to_html(report)
        assert "Use UCCSD" in html
        assert "HIGH" in html
        assert "h q[0];" in html

    def test_report_to_html_suitability_colors(self):
        # High suitability = green
        high = AnalysisReport(suitability=0.8)
        assert "#22c55e" in report_to_html(high)

        # Medium suitability = yellow
        med = AnalysisReport(suitability=0.45)
        assert "#eab308" in report_to_html(med)

        # Low suitability = red
        low = AnalysisReport(suitability=0.2)
        assert "#ef4444" in report_to_html(low)

    def test_chat_to_html(self):
        cr = ChatResponse(
            message="Try QAOA with 3 layers.",
            papers=[Paper(title="P", arxiv_url="https://arxiv.org/abs/1")],
        )
        html = chat_to_html(cr)
        assert "Try QAOA" in html
        assert "Referenced Papers" in html


# ---------------------------------------------------------------------------
# Circuit conversion routing tests
# ---------------------------------------------------------------------------

class TestCircuitConversion:
    """Tests for _to_qasm3 conversion routing."""

    def test_string_passthrough(self):
        """Raw QASM3 strings should pass through unchanged."""
        from arvak.nathan import _to_qasm3

        qasm = "OPENQASM 3.0;\nqubit[2] q;\nh q[0];"
        code, lang = _to_qasm3(qasm)
        assert code == qasm
        assert lang == "qasm3"

    def test_string_with_language_override(self):
        from arvak.nathan import _to_qasm3

        code, lang = _to_qasm3("some code", language="qiskit")
        assert lang == "qiskit"

    def test_arvak_circuit_conversion(self):
        """Arvak Circuit should be converted to QASM3."""
        from arvak.nathan import _to_qasm3

        bell = arvak.Circuit.bell()
        code, lang = _to_qasm3(bell)
        assert "OPENQASM" in code
        assert lang == "qasm3"

    def test_unsupported_type_raises(self):
        """Unsupported circuit types should raise TypeError."""
        from arvak.nathan import _to_qasm3

        with pytest.raises(TypeError, match="Unsupported circuit type"):
            _to_qasm3(42)

    def test_unsupported_object_raises(self):
        from arvak.nathan import _to_qasm3

        with pytest.raises(TypeError, match="Unsupported circuit type"):
            _to_qasm3({"not": "a circuit"})


# ---------------------------------------------------------------------------
# Configure tests
# ---------------------------------------------------------------------------

class TestConfigure:
    """Tests for arvak.nathan.configure()."""

    def test_configure_resets_client(self):
        """Calling configure should reset the cached client."""
        import arvak.nathan as nathan_mod

        # Force a client creation
        nathan_mod._client = MagicMock()
        assert nathan_mod._client is not None

        # Configure should reset it
        configure(api_key="test_key")
        assert nathan_mod._client is None
        assert nathan_mod._api_key == "test_key"

        # Cleanup
        nathan_mod._api_key = None

    def test_configure_api_url(self):
        import arvak.nathan as nathan_mod

        configure(api_url="https://custom.api.com/nathan")
        assert nathan_mod._api_url == "https://custom.api.com/nathan"

        # Cleanup
        nathan_mod._api_url = "https://arvak.io/api/nathan"


# ---------------------------------------------------------------------------
# Client parsing tests (mocked HTTP)
# ---------------------------------------------------------------------------

class TestClientParsing:
    """Tests for NathanClient response parsing (no live API needed)."""

    def _make_client(self):
        return NathanClient(api_url="https://arvak.io/api/nathan", api_key="test")

    def test_parse_report_minimal(self):
        client = self._make_client()
        data = {
            "summary": "Basic analysis",
            "problem_type": "qaoa",
            "suitability": 0.65,
        }
        report = client._parse_report(data)
        assert isinstance(report, AnalysisReport)
        assert report.problem_type == "qaoa"
        assert report.suitability == 0.65
        assert report.circuit is None
        assert report.papers == []

    def test_parse_report_with_circuit(self):
        client = self._make_client()
        data = {
            "summary": "Full",
            "problem_type": "vqe",
            "suitability": 0.8,
            "circuit": {
                "num_qubits": 4,
                "total_gates": 12,
                "gate_breakdown": "h: 4, cx: 8",
                "depth": 6,
                "detected_pattern": "vqe",
                "language": "qasm3",
            },
        }
        report = client._parse_report(data)
        assert report.circuit is not None
        assert report.circuit.num_qubits == 4
        assert report.circuit.detected_pattern == "vqe"

    def test_parse_report_with_papers(self):
        client = self._make_client()
        data = {
            "problem_type": "grover",
            "suitability": 0.6,
            "papers": [
                {
                    "title": "Grover 2.0",
                    "arxiv_url": "https://arxiv.org/abs/2301.12345",
                    "problem_type": "grover",
                    "algorithm": "grover",
                    "relevance": "quadratic speedup",
                },
            ],
        }
        report = client._parse_report(data)
        assert len(report.papers) == 1
        assert report.papers[0].title == "Grover 2.0"
        assert report.papers[0].relevance == "quadratic speedup"

    def test_parse_report_with_suggestions(self):
        client = self._make_client()
        data = {
            "problem_type": "qaoa",
            "suitability": 0.7,
            "suggestions": [
                {
                    "title": "Increase layers",
                    "description": "Use p=3 for better approximation ratio",
                    "qasm3": "OPENQASM 3.0;\nqubit[4] q;",
                    "impact": "high",
                },
                {
                    "title": "Use warm-start",
                    "description": "Initialize with classical solution",
                    "impact": "medium",
                },
            ],
        }
        report = client._parse_report(data)
        assert len(report.suggestions) == 2
        assert report.suggestions[0].impact == "high"
        assert report.suggestions[0].qasm3.startswith("OPENQASM")
        assert report.suggestions[1].qasm3 == ""

    def test_parse_report_all_fields(self):
        client = self._make_client()
        data = {
            "summary": "Comprehensive analysis",
            "problem_type": "portfolio_opt",
            "suitability": 0.55,
            "recommended_algorithm": "qaoa",
            "estimated_qubits": 8,
            "hardware_fit": "Good fit for IQM Garnet",
            "estimated_error_rate": "~2%",
            "recommended_shots": 4096,
            "circuit": {
                "num_qubits": 8,
                "total_gates": 32,
                "gate_breakdown": "h: 8, cx: 16, rz: 8",
                "depth": 12,
                "detected_pattern": "qaoa",
                "language": "qasm3",
            },
            "papers": [],
            "suggestions": [],
        }
        report = client._parse_report(data)
        assert report.recommended_algorithm == "qaoa"
        assert report.estimated_qubits == 8
        assert report.hardware_fit == "Good fit for IQM Garnet"
        assert report.estimated_error_rate == "~2%"
        assert report.recommended_shots == 4096

    def test_parse_report_missing_fields_use_defaults(self):
        """Missing fields should get sensible defaults."""
        client = self._make_client()
        data = {}
        report = client._parse_report(data)
        assert report.problem_type == "unknown"
        assert report.suitability == 0.0
        assert report.recommended_algorithm == ""
        assert report.recommended_shots == 1024

    def test_headers_include_auth(self):
        client = NathanClient(api_url="https://arvak.io/api/nathan", api_key="nthn_test123")
        headers = client._headers()
        assert headers["Authorization"] == "Bearer nthn_test123"
        assert headers["Content-Type"] == "application/json"

    def test_headers_no_auth_when_empty(self):
        client = NathanClient(api_url="https://arvak.io/api/nathan", api_key="")
        headers = client._headers()
        assert "Authorization" not in headers


# ---------------------------------------------------------------------------
# End-to-end with mocked HTTP
# ---------------------------------------------------------------------------

class TestAnalyzeWithMockedAPI:
    """Tests for arvak.nathan.analyze() with mocked HTTP responses."""

    @patch("arvak.nathan.client.httpx.Client")
    def test_analyze_arvak_circuit(self, mock_client_cls):
        """analyze() should convert an Arvak circuit and call the API."""
        mock_resp = MagicMock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {
            "summary": "Bell state detected",
            "problem_type": "unknown",
            "suitability": 0.3,
            "circuit": {
                "num_qubits": 2,
                "total_gates": 2,
                "gate_breakdown": "h: 1, cx: 1",
                "depth": 2,
                "detected_pattern": "unknown",
                "language": "qasm3",
            },
            "papers": [],
            "suggestions": [],
        }
        mock_resp.raise_for_status = MagicMock()

        mock_instance = MagicMock()
        mock_instance.post.return_value = mock_resp
        mock_client_cls.return_value = mock_instance

        # Reset cached client
        import arvak.nathan as nathan_mod
        nathan_mod._client = None

        bell = arvak.Circuit.bell()
        report = arvak.nathan.analyze(bell)

        assert isinstance(report, AnalysisReport)
        assert report.circuit.num_qubits == 2
        assert mock_instance.post.called

        # Verify QASM3 was sent in the request body
        call_args = mock_instance.post.call_args
        body = call_args[1]["json"] if "json" in call_args[1] else call_args[0][1]
        assert "OPENQASM" in body.get("code", "")

        # Cleanup
        nathan_mod._client = None

    @patch("arvak.nathan.client.httpx.Client")
    def test_analyze_raw_qasm(self, mock_client_cls):
        """analyze() should pass raw QASM3 strings directly."""
        mock_resp = MagicMock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {
            "problem_type": "qaoa",
            "suitability": 0.7,
        }
        mock_resp.raise_for_status = MagicMock()

        mock_instance = MagicMock()
        mock_instance.post.return_value = mock_resp
        mock_client_cls.return_value = mock_instance

        import arvak.nathan as nathan_mod
        nathan_mod._client = None

        qasm = "OPENQASM 3.0;\nqubit[4] q;\nh q[0];"
        report = arvak.nathan.analyze(qasm, anonymize=False)

        assert report.problem_type == "qaoa"
        call_body = mock_instance.post.call_args[1]["json"]
        assert call_body["code"] == qasm
        assert call_body["language"] == "qasm3"

        nathan_mod._client = None


class TestChatWithMockedAPI:
    """Tests for arvak.nathan.chat() with mocked HTTP responses."""

    @patch("arvak.nathan.client.httpx.Client")
    def test_chat_returns_response(self, mock_client_cls):
        mock_resp = MagicMock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {
            "message": "QAOA is well-suited for MaxCut problems.",
            "papers": [
                {"title": "QAOA Original", "arxiv_url": "https://arxiv.org/abs/1411.4028"},
            ],
        }
        mock_resp.raise_for_status = MagicMock()

        mock_instance = MagicMock()
        mock_instance.post.return_value = mock_resp
        mock_client_cls.return_value = mock_instance

        import arvak.nathan as nathan_mod
        nathan_mod._client = None

        resp = arvak.nathan.chat("What algorithm for MaxCut?")

        assert isinstance(resp, ChatResponse)
        assert "QAOA" in resp.message
        assert len(resp.papers) == 1
        assert resp.papers[0].title == "QAOA Original"

        nathan_mod._client = None


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
