"""Tests for Nathan's QMAP Clifford synthesis module."""

from __future__ import annotations

import pytest

from arvak.nathan.clifford import (
    CLIFFORD_GATES,
    CliffordOptResult,
    CliffordRegion,
    _extract_qubit_info,
    _parse_qasm3_gates,
    _qmap_available,
    analyze_clifford_content,
    find_clifford_regions,
    generate_clifford_suggestions,
    is_clifford_gate,
    optimize_clifford,
)
from arvak.nathan.report import Suggestion


# ---------------------------------------------------------------------------
# Clifford gate classification
# ---------------------------------------------------------------------------


class TestIsCliffordGate:
    def test_single_qubit_clifford(self):
        for gate in ("h", "s", "sdg", "x", "y", "z", "sx", "sxdg", "id", "i"):
            assert is_clifford_gate(gate), f"{gate} should be Clifford"

    def test_two_qubit_clifford(self):
        for gate in ("cx", "cnot", "cz", "cy", "swap"):
            assert is_clifford_gate(gate), f"{gate} should be Clifford"

    def test_case_insensitive(self):
        assert is_clifford_gate("H")
        assert is_clifford_gate("CX")
        assert is_clifford_gate("SWAP")

    def test_non_clifford_gates(self):
        for gate in ("t", "tdg", "rx", "ry", "rz", "p", "u", "ccx", "cswap"):
            assert not is_clifford_gate(gate), f"{gate} should NOT be Clifford"

    def test_unknown_gate(self):
        assert not is_clifford_gate("foobar")
        assert not is_clifford_gate("")

    def test_whitespace_handling(self):
        assert is_clifford_gate("  h  ")
        assert is_clifford_gate("cx ")


# ---------------------------------------------------------------------------
# QASM3 gate parsing
# ---------------------------------------------------------------------------


SIMPLE_CIRCUIT = """\
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0],q[1];
"""

MIXED_CIRCUIT = """\
OPENQASM 3.0;
qubit[3] q;
h q[0];
cx q[0],q[1];
rz(0.5) q[2];
s q[1];
t q[0];
h q[2];
"""

PARAMETRIC_CIRCUIT = """\
OPENQASM 3.0;
qubit[2] q;
rx(3.14) q[0];
ry(1.57) q[1];
rz(0.785) q[0];
"""


class TestParseQasm3Gates:
    def test_simple_gates(self):
        gates = _parse_qasm3_gates(SIMPLE_CIRCUIT)
        assert len(gates) == 2
        assert gates[0][1] == "h"
        assert gates[1][1] == "cx"

    def test_mixed_gates(self):
        gates = _parse_qasm3_gates(MIXED_CIRCUIT)
        names = [g[1] for g in gates]
        assert "h" in names
        assert "cx" in names
        assert "s" in names
        assert "t" in names

    def test_skips_declarations(self):
        gates = _parse_qasm3_gates(SIMPLE_CIRCUIT)
        names = [g[1] for g in gates]
        assert "openqasm" not in names
        assert "qubit" not in names

    def test_empty_circuit(self):
        gates = _parse_qasm3_gates("")
        assert gates == []

    def test_comments_skipped(self):
        circuit = "OPENQASM 3.0;\nqubit[1] q;\n// this is a comment\nh q[0];\n"
        gates = _parse_qasm3_gates(circuit)
        assert len(gates) == 1
        assert gates[0][1] == "h"

    def test_parametric_gates(self):
        gates = _parse_qasm3_gates(PARAMETRIC_CIRCUIT)
        # The regex should still find the gate names
        assert len(gates) >= 1

    def test_measure_barrier_skipped(self):
        circuit = "OPENQASM 3.0;\nqubit[1] q;\nh q[0];\nbarrier q;\nmeasure q[0];\n"
        gates = _parse_qasm3_gates(circuit)
        assert len(gates) == 1
        assert gates[0][1] == "h"


# ---------------------------------------------------------------------------
# Qubit info extraction
# ---------------------------------------------------------------------------


class TestExtractQubitInfo:
    def test_qubit_declaration(self):
        decl, n = _extract_qubit_info("OPENQASM 3.0;\nqubit[4] q;\nh q[0];")
        assert n == 4
        assert "qubit[4]" in decl

    def test_qreg_declaration(self):
        decl, n = _extract_qubit_info("OPENQASM 2.0;\nqreg q[3];\nh q[0];")
        assert n == 3
        assert "qreg" in decl

    def test_no_declaration_defaults(self):
        decl, n = _extract_qubit_info("h q[0];")
        assert n == 1

    def test_multiple_registers_first_wins(self):
        code = "OPENQASM 3.0;\nqubit[2] q;\nqubit[3] r;\n"
        decl, n = _extract_qubit_info(code)
        assert n == 2


# ---------------------------------------------------------------------------
# Clifford content analysis
# ---------------------------------------------------------------------------


class TestAnalyzeCliffordContent:
    def test_fully_clifford(self):
        result = analyze_clifford_content(SIMPLE_CIRCUIT)
        assert result["total_gates"] == 2
        assert result["clifford_gates"] == 2
        assert result["non_clifford_gates"] == 0
        assert result["is_fully_clifford"] is True
        assert result["clifford_ratio"] == 1.0

    def test_mixed_circuit(self):
        result = analyze_clifford_content(MIXED_CIRCUIT)
        assert result["total_gates"] == 6
        # h, cx, s, h are Clifford; rz(0.5) and t are not
        assert result["clifford_gates"] == 4
        assert result["non_clifford_gates"] == 2
        assert result["is_fully_clifford"] is False
        assert 0.5 < result["clifford_ratio"] < 1.0

    def test_no_clifford(self):
        result = analyze_clifford_content(PARAMETRIC_CIRCUIT)
        assert result["clifford_gates"] == 0
        assert result["is_fully_clifford"] is False
        assert result["clifford_ratio"] == 0.0

    def test_empty_circuit(self):
        result = analyze_clifford_content("")
        assert result["total_gates"] == 0
        assert result["is_fully_clifford"] is False

    def test_gate_breakdown(self):
        result = analyze_clifford_content(SIMPLE_CIRCUIT)
        assert "h" in result["gate_breakdown"]
        assert "cx" in result["gate_breakdown"]
        assert result["gate_breakdown"]["h"] == 1
        assert result["gate_breakdown"]["cx"] == 1

    def test_large_clifford_circuit(self):
        lines = ["OPENQASM 3.0;", "qubit[4] q;"]
        for i in range(20):
            lines.append(f"h q[{i % 4}];")
            lines.append(f"cx q[{i % 4}],q[{(i+1) % 4}];")
        code = "\n".join(lines) + "\n"
        result = analyze_clifford_content(code)
        assert result["total_gates"] == 40
        assert result["is_fully_clifford"] is True


# ---------------------------------------------------------------------------
# Clifford region detection
# ---------------------------------------------------------------------------


class TestFindCliffordRegions:
    def test_fully_clifford_single_region(self):
        circuit = "OPENQASM 3.0;\nqubit[2] q;\nh q[0];\ncx q[0],q[1];\ns q[0];\ny q[1];\nz q[0];\n"
        regions = find_clifford_regions(circuit, min_gates=2)
        assert len(regions) == 1
        assert regions[0].gate_count == 5

    def test_split_by_non_clifford(self):
        circuit = (
            "OPENQASM 3.0;\n"
            "qubit[2] q;\n"
            "h q[0];\n"
            "cx q[0],q[1];\n"
            "s q[0];\n"
            "rz(0.5) q[1];\n"  # non-Clifford breaks the run
            "h q[1];\n"
            "cx q[1],q[0];\n"
            "x q[0];\n"
            "y q[1];\n"
        )
        regions = find_clifford_regions(circuit, min_gates=2)
        # Should find two regions: [h, cx, s] and [h, cx, x, y]
        assert len(regions) == 2
        gate_counts = sorted([r.gate_count for r in regions], reverse=True)
        assert gate_counts[0] == 4  # h, cx, x, y
        assert gate_counts[1] == 3  # h, cx, s

    def test_min_gates_filter(self):
        circuit = "OPENQASM 3.0;\nqubit[1] q;\nh q[0];\ns q[0];\n"
        regions = find_clifford_regions(circuit, min_gates=5)
        assert len(regions) == 0

    def test_sorted_by_size(self):
        circuit = (
            "OPENQASM 3.0;\n"
            "qubit[2] q;\n"
            "h q[0];\n"  # region 1: 1 gate (below threshold)
            "t q[0];\n"
            "cx q[0],q[1];\n"  # region 2: 3 gates
            "s q[0];\n"
            "h q[1];\n"
            "t q[1];\n"
            "h q[0];\n"  # region 3: 4 gates
            "cx q[0],q[1];\n"
            "x q[0];\n"
            "z q[1];\n"
        )
        regions = find_clifford_regions(circuit, min_gates=2)
        assert regions[0].gate_count >= regions[-1].gate_count

    def test_region_qasm_is_valid(self):
        circuit = "OPENQASM 3.0;\nqubit[2] q;\nh q[0];\ncx q[0],q[1];\ns q[0];\n"
        regions = find_clifford_regions(circuit, min_gates=2)
        assert len(regions) == 1
        qasm = regions[0].qasm3
        assert "OPENQASM 3.0;" in qasm
        assert "qubit[2] q;" in qasm
        assert "h q[0];" in qasm

    def test_empty_circuit(self):
        regions = find_clifford_regions("", min_gates=1)
        assert regions == []

    def test_no_clifford_gates(self):
        circuit = "OPENQASM 3.0;\nqubit[1] q;\nrx(1.0) q[0];\nry(2.0) q[0];\n"
        regions = find_clifford_regions(circuit, min_gates=1)
        assert regions == []


# ---------------------------------------------------------------------------
# Suggestion model: source field and is_optimal property
# ---------------------------------------------------------------------------


class TestSuggestionSource:
    def test_default_source(self):
        s = Suggestion(title="t", description="d")
        assert s.source == "nathan_llm"
        assert s.is_optimal is False

    def test_qmap_sat_source(self):
        s = Suggestion(title="t", description="d", source="qmap_sat")
        assert s.source == "qmap_sat"
        assert s.is_optimal is True

    def test_qmap_heuristic_source(self):
        s = Suggestion(title="t", description="d", source="qmap_heuristic")
        assert s.source == "qmap_heuristic"
        assert s.is_optimal is False

    def test_repr_includes_source(self):
        s = Suggestion(title="t", description="d", source="qmap_sat")
        assert "qmap_sat" in repr(s)

    def test_repr_excludes_default_source(self):
        s = Suggestion(title="t", description="d")
        assert "source" not in repr(s)


# ---------------------------------------------------------------------------
# optimize_clifford (with mocked QMAP)
# ---------------------------------------------------------------------------


class TestOptimizeClifford:
    def test_returns_none_when_qmap_unavailable(self, monkeypatch):
        """Should return None when mqt.qmap is not installed."""
        import arvak.nathan.clifford as mod
        monkeypatch.setattr(mod, "_qmap_available", lambda: False)

        result = optimize_clifford(SIMPLE_CIRCUIT)
        assert result is None

    def test_returns_none_on_exception(self, monkeypatch):
        """Should return None and log warning on QMAP error."""
        import arvak.nathan.clifford as mod

        def mock_run(*args, **kwargs):
            raise RuntimeError("SAT solver exploded")

        monkeypatch.setattr(mod, "_run_qmap_optimization", mock_run)

        result = optimize_clifford(SIMPLE_CIRCUIT)
        assert result is None

    def test_returns_result_with_mock(self, monkeypatch):
        """Mock _run_qmap_optimization to return a valid result."""
        import arvak.nathan.clifford as mod

        mock_result = CliffordOptResult(
            original_qasm=SIMPLE_CIRCUIT,
            optimized_qasm="OPENQASM 3.0;\nqubit[2] q;\ncx q[0],q[1];\n",
            original_gates=2,
            optimized_gates=1,
            original_depth=2,
            optimized_depth=1,
            improvement_pct=50.0,
            method="sat_optimal",
        )
        monkeypatch.setattr(mod, "_run_qmap_optimization", lambda *a, **k: mock_result)

        result = optimize_clifford(SIMPLE_CIRCUIT)
        assert result is not None
        assert result.improvement_pct == 50.0
        assert result.method == "sat_optimal"


# ---------------------------------------------------------------------------
# generate_clifford_suggestions (with mocked QMAP)
# ---------------------------------------------------------------------------


class TestGenerateCliffordSuggestions:
    def test_skips_when_qmap_unavailable(self, monkeypatch):
        import arvak.nathan.clifford as mod
        monkeypatch.setattr(mod, "_qmap_available", lambda: False)

        suggestions = generate_clifford_suggestions(SIMPLE_CIRCUIT)
        assert suggestions == []

    def test_skips_small_circuits(self, monkeypatch):
        import arvak.nathan.clifford as mod
        monkeypatch.setattr(mod, "_qmap_available", lambda: True)

        tiny = "OPENQASM 3.0;\nqubit[1] q;\nh q[0];\n"
        suggestions = generate_clifford_suggestions(tiny, min_region_gates=5)
        assert suggestions == []

    def test_generates_suggestion_for_clifford_circuit(self, monkeypatch):
        import arvak.nathan.clifford as mod

        monkeypatch.setattr(mod, "_qmap_available", lambda: True)

        mock_result = CliffordOptResult(
            original_qasm=SIMPLE_CIRCUIT,
            optimized_qasm="OPENQASM 3.0;\nqubit[2] q;\ncx q[0],q[1];\n",
            original_gates=5,
            optimized_gates=3,
            original_depth=5,
            optimized_depth=2,
            improvement_pct=40.0,
            method="sat_optimal",
        )
        monkeypatch.setattr(mod, "optimize_clifford", lambda *a, **k: mock_result)

        # Build a circuit with enough Clifford gates
        circuit = (
            "OPENQASM 3.0;\n"
            "qubit[2] q;\n"
            "h q[0];\n"
            "cx q[0],q[1];\n"
            "s q[0];\n"
            "h q[1];\n"
            "x q[0];\n"
        )
        suggestions = generate_clifford_suggestions(circuit, min_region_gates=4)
        assert len(suggestions) >= 1

        s = suggestions[0]
        assert s.impact == "high"
        assert s.verified is True
        assert s.source in ("qmap_sat", "qmap_heuristic")
        assert "QMAP" in s.description
        assert s.verification_status == "verified"

    def test_generates_region_suggestions_for_mixed(self, monkeypatch):
        import arvak.nathan.clifford as mod

        monkeypatch.setattr(mod, "_qmap_available", lambda: True)

        mock_result = CliffordOptResult(
            original_qasm="...",
            optimized_qasm="OPENQASM 3.0;\nqubit[3] q;\ncx q[0],q[1];\n",
            original_gates=4,
            optimized_gates=2,
            original_depth=4,
            optimized_depth=2,
            improvement_pct=50.0,
            method="sat_optimal",
        )
        monkeypatch.setattr(mod, "optimize_clifford", lambda *a, **k: mock_result)

        # Mixed circuit: Clifford region then non-Clifford then Clifford
        circuit = (
            "OPENQASM 3.0;\n"
            "qubit[3] q;\n"
            "h q[0];\n"
            "cx q[0],q[1];\n"
            "s q[0];\n"
            "z q[2];\n"
            "rz(0.5) q[1];\n"  # breaks Clifford run
            "h q[1];\n"
            "cx q[1],q[2];\n"
        )
        suggestions = generate_clifford_suggestions(circuit, min_region_gates=3)
        assert len(suggestions) >= 1

        for s in suggestions:
            assert "Clifford" in s.title or "Optimal" in s.title
            assert s.verified is True

    def test_no_suggestions_when_no_improvement(self, monkeypatch):
        import arvak.nathan.clifford as mod

        monkeypatch.setattr(mod, "_qmap_available", lambda: True)

        # Return result with 0% improvement
        mock_result = CliffordOptResult(
            original_qasm=SIMPLE_CIRCUIT,
            optimized_qasm=SIMPLE_CIRCUIT,
            original_gates=5,
            optimized_gates=5,
            original_depth=5,
            optimized_depth=5,
            improvement_pct=0.0,
            method="sat_optimal",
        )
        monkeypatch.setattr(mod, "optimize_clifford", lambda *a, **k: mock_result)

        circuit = (
            "OPENQASM 3.0;\n"
            "qubit[2] q;\n"
            "h q[0];\n"
            "cx q[0],q[1];\n"
            "s q[0];\n"
            "h q[1];\n"
            "x q[0];\n"
        )
        suggestions = generate_clifford_suggestions(circuit, min_region_gates=4)
        assert suggestions == []

    def test_no_suggestions_for_non_clifford(self, monkeypatch):
        import arvak.nathan.clifford as mod

        monkeypatch.setattr(mod, "_qmap_available", lambda: True)

        suggestions = generate_clifford_suggestions(PARAMETRIC_CIRCUIT, min_region_gates=2)
        assert suggestions == []


# ---------------------------------------------------------------------------
# Display rendering with optimal badges
# ---------------------------------------------------------------------------


class TestDisplayOptimalBadges:
    def test_sat_optimal_badge_in_html(self):
        from arvak.nathan.report import AnalysisReport
        from arvak.nathan.display import report_to_html

        report = AnalysisReport(
            suggestions=[
                Suggestion(
                    title="Optimal Clifford",
                    description="SAT-proven",
                    qasm3="h q[0];",
                    impact="high",
                    verified=True,
                    source="qmap_sat",
                ),
            ]
        )
        html = report_to_html(report)
        assert "OPTIMAL (SAT)" in html
        assert "#818cf8" in html  # indigo color for optimal badge
        assert "VERIFIED" in html  # also shows verified badge

    def test_heuristic_badge_in_html(self):
        from arvak.nathan.report import AnalysisReport
        from arvak.nathan.display import report_to_html

        report = AnalysisReport(
            suggestions=[
                Suggestion(
                    title="Heuristic Clifford",
                    description="heuristic",
                    qasm3="h q[0];",
                    impact="high",
                    verified=True,
                    source="qmap_heuristic",
                ),
            ]
        )
        html = report_to_html(report)
        assert "OPTIMIZED" in html
        assert "#818cf8" in html

    def test_llm_suggestion_no_optimal_badge(self):
        from arvak.nathan.report import AnalysisReport
        from arvak.nathan.display import report_to_html

        report = AnalysisReport(
            suggestions=[
                Suggestion(
                    title="LLM suggestion",
                    description="desc",
                    qasm3="h q[0];",
                    impact="medium",
                    verified=True,
                ),
            ]
        )
        html = report_to_html(report)
        assert "OPTIMAL" not in html
        assert "OPTIMIZED" not in html
        assert "VERIFIED" in html  # only shows verified badge


# ---------------------------------------------------------------------------
# Integration: analyze() with optimize_clifford parameter
# ---------------------------------------------------------------------------


class TestAnalyzeOptimizeClifford:
    def test_analyze_with_clifford_optimization(self, monkeypatch):
        import arvak.nathan as nathan_mod
        import arvak.nathan.clifford as cliff_mod

        mock_report = nathan_mod.AnalysisReport(
            summary="test",
            suitability=1.0,  # above QEC threshold — only test Clifford behavior
            suggestions=[
                Suggestion(title="LLM opt", description="d"),
            ],
        )

        class MockClient:
            def analyze(self, **kwargs):
                return mock_report

        monkeypatch.setattr(nathan_mod, "_get_client", lambda: MockClient())
        monkeypatch.setattr(nathan_mod, "_to_qasm3", lambda c, l: ("OPENQASM 3.0;", "qasm3"))

        # Mock Clifford suggestions
        cliff_suggestion = Suggestion(
            title="Optimal Clifford",
            description="SAT",
            qasm3="optimized",
            impact="high",
            verified=True,
            source="qmap_sat",
        )
        monkeypatch.setattr(
            cliff_mod, "generate_clifford_suggestions",
            lambda *a, **k: [cliff_suggestion],
        )

        report = nathan_mod.analyze("OPENQASM 3.0;", verify=False, optimize_clifford=True)
        # Clifford suggestions should be prepended
        assert len(report.suggestions) == 2
        assert report.suggestions[0].source == "qmap_sat"
        assert report.suggestions[0].title == "Optimal Clifford"
        assert report.suggestions[1].title == "LLM opt"

    def test_analyze_clifford_disabled(self, monkeypatch):
        import arvak.nathan as nathan_mod

        mock_report = nathan_mod.AnalysisReport(
            suitability=1.0,  # above QEC threshold — only test Clifford behavior
            suggestions=[
                Suggestion(title="LLM opt", description="d"),
            ],
        )

        class MockClient:
            def analyze(self, **kwargs):
                return mock_report

        monkeypatch.setattr(nathan_mod, "_get_client", lambda: MockClient())
        monkeypatch.setattr(nathan_mod, "_to_qasm3", lambda c, l: ("OPENQASM 3.0;", "qasm3"))

        report = nathan_mod.analyze("code", verify=False, optimize_clifford=False)
        assert len(report.suggestions) == 1
        assert report.suggestions[0].source == "nathan_llm"

    def test_analyze_clifford_no_qmap_graceful(self, monkeypatch):
        import arvak.nathan as nathan_mod
        import arvak.nathan.clifford as cliff_mod

        mock_report = nathan_mod.AnalysisReport(
            suitability=1.0,  # above QEC threshold — only test Clifford behavior
            suggestions=[Suggestion(title="LLM", description="d")],
        )

        class MockClient:
            def analyze(self, **kwargs):
                return mock_report

        monkeypatch.setattr(nathan_mod, "_get_client", lambda: MockClient())
        monkeypatch.setattr(nathan_mod, "_to_qasm3", lambda c, l: ("OPENQASM 3.0;", "qasm3"))
        monkeypatch.setattr(cliff_mod, "_qmap_available", lambda: False)

        report = nathan_mod.analyze("code", verify=False, optimize_clifford=True)
        # Should still work, just no Clifford suggestions
        assert len(report.suggestions) == 1
        assert report.suggestions[0].title == "LLM"


# ---------------------------------------------------------------------------
# CliffordOptResult dataclass
# ---------------------------------------------------------------------------


class TestCliffordOptResult:
    def test_fields(self):
        r = CliffordOptResult(
            original_qasm="orig",
            optimized_qasm="opt",
            original_gates=10,
            optimized_gates=5,
            original_depth=8,
            optimized_depth=3,
            improvement_pct=50.0,
            method="sat_optimal",
        )
        assert r.original_gates == 10
        assert r.optimized_gates == 5
        assert r.improvement_pct == 50.0
        assert r.method == "sat_optimal"


# ---------------------------------------------------------------------------
# CliffordRegion dataclass
# ---------------------------------------------------------------------------


class TestCliffordRegion:
    def test_fields(self):
        r = CliffordRegion(
            qasm3="OPENQASM 3.0;\nqubit[2] q;\nh q[0];\n",
            qubit_decl="qubit[2] q;",
            num_qubits=2,
            gate_count=1,
            start_line=2,
            end_line=2,
        )
        assert r.num_qubits == 2
        assert r.gate_count == 1
        assert "h q[0]" in r.qasm3


# ---------------------------------------------------------------------------
# CLIFFORD_GATES constant
# ---------------------------------------------------------------------------


class TestCliffordGatesConstant:
    def test_is_frozenset(self):
        assert isinstance(CLIFFORD_GATES, frozenset)

    def test_contains_expected(self):
        expected = {"h", "s", "sdg", "x", "y", "z", "cx", "cz", "swap", "id"}
        assert expected.issubset(CLIFFORD_GATES)

    def test_no_t_gate(self):
        assert "t" not in CLIFFORD_GATES
        assert "tdg" not in CLIFFORD_GATES

    def test_no_rotation_gates(self):
        assert "rx" not in CLIFFORD_GATES
        assert "ry" not in CLIFFORD_GATES
        assert "rz" not in CLIFFORD_GATES
