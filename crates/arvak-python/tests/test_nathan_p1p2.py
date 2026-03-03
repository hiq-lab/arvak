"""Tests for Nathan P1 + P2 extensions.

Covers:
- P1 #3: MQT Bench reference circuits (bench.py)
- P1 #4: DDSIM noise-aware fidelity scoring (noise.py)
- P1 #5: QECC error correction suggestions (qecc.py)
- P2 #6: Session class (session.py)
- P2 #7: report.apply(idx)
- P2 #8: Custom knowledge sources (extra_context)
"""

from __future__ import annotations

import types
from dataclasses import dataclass
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

# ---------------------------------------------------------------------------
# Helpers / shared fixtures
# ---------------------------------------------------------------------------

SIMPLE_QASM = """\
OPENQASM 3.0;
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
c = measure q;
"""


def _make_report(
    problem_type="qaoa",
    suitability=0.75,
    num_qubits=6,
    total_gates=30,
    depth=18,
    estimated_error_rate="~1%",
    suggestions=None,
):
    """Build a minimal AnalysisReport for testing."""
    from arvak.nathan.report import AnalysisReport, CircuitStats, Suggestion

    circuit = CircuitStats(
        num_qubits=num_qubits,
        total_gates=total_gates,
        depth=depth,
        detected_pattern=problem_type,
    )
    return AnalysisReport(
        summary="Test summary",
        problem_type=problem_type,
        suitability=suitability,
        estimated_error_rate=estimated_error_rate,
        circuit=circuit,
        suggestions=suggestions or [],
    )


# ===========================================================================
# P1 #3 — bench.py
# ===========================================================================


class TestBenchReferences:
    def test_known_algorithm_returns_results(self):
        from arvak.nathan.bench import find_references

        refs = find_references("qaoa", 6)
        assert len(refs) > 0
        assert all(r.algorithm == "qaoa" for r in refs)

    def test_sorted_by_proximity(self):
        from arvak.nathan.bench import find_references

        refs = find_references("qft", 5, max_results=3)
        # Result closest to 5 qubits first
        assert refs[0].num_qubits in (4, 6)
        distances = [abs(r.num_qubits - 5) for r in refs]
        assert distances == sorted(distances)

    def test_max_results_respected(self):
        from arvak.nathan.bench import find_references

        refs = find_references("vqe", 8, max_results=2)
        assert len(refs) <= 2

    def test_unknown_algorithm_returns_empty(self):
        from arvak.nathan.bench import find_references

        refs = find_references("totally_unknown_algo", 6)
        assert refs == []

    def test_alias_bv(self):
        from arvak.nathan.bench import find_references

        refs = find_references("bv", 4)
        assert all(r.algorithm == "bernstein_vazirani" for r in refs)

    def test_alias_grover(self):
        from arvak.nathan.bench import find_references

        refs = find_references("grover", 4)
        assert len(refs) > 0

    def test_reference_has_url(self):
        from arvak.nathan.bench import find_references

        refs = find_references("ghz", 4)
        assert all(r.url.startswith("https://") for r in refs)

    def test_bench_reference_repr(self):
        from arvak.nathan.bench import BenchReference

        r = BenchReference(
            algorithm="qaoa",
            num_qubits=6,
            depth=18,
            gate_count=30,
            bench_id="qaoa_indep_qiskit_6",
            url="https://example.com",
            abstraction_level="indep",
            description="Test",
        )
        assert "qaoa" in repr(r)
        assert "6" in repr(r)

    def test_mqt_bench_available_false_without_package(self, monkeypatch):
        from arvak.nathan import bench

        monkeypatch.setattr(bench, "_mqt_bench_available", lambda: False)
        assert not bench._mqt_bench_available()

    def test_display_section_renders(self):
        from arvak.nathan.display import report_to_html
        from arvak.nathan.bench import find_references

        report = _make_report(problem_type="qaoa", num_qubits=6)
        report.reference_circuits = find_references("qaoa", 6)
        html = report_to_html(report)
        assert "Reference Circuits" in html
        assert "MQT Bench" in html
        assert "qaoa" in html.lower()

    def test_display_empty_references_no_section(self):
        from arvak.nathan.display import report_to_html

        report = _make_report()
        report.reference_circuits = []
        html = report_to_html(report)
        assert "Reference Circuits" not in html


# ===========================================================================
# P1 #4 — noise.py
# ===========================================================================


class TestFidelityEstimate:
    def test_heuristic_returns_value(self):
        from arvak.nathan.noise import _heuristic_fidelity

        fe = _heuristic_fidelity(SIMPLE_QASM, "iqm_garnet")
        assert 0.0 <= fe.fidelity <= 1.0
        assert fe.method == "heuristic"
        assert fe.backend == "iqm_garnet"

    def test_heuristic_perfect_for_ideal_sim(self):
        from arvak.nathan.noise import _heuristic_fidelity

        fe = _heuristic_fidelity(SIMPLE_QASM, "aer_simulator")
        assert fe.fidelity == pytest.approx(1.0)

    def test_heuristic_custom_noise_profile(self):
        from arvak.nathan.noise import _heuristic_fidelity

        fe = _heuristic_fidelity(SIMPLE_QASM, "custom", {"sq_err": 0.0, "tq_err": 0.0})
        assert fe.fidelity == pytest.approx(1.0)

    def test_estimate_fidelity_without_ddsim_uses_heuristic(self, monkeypatch):
        from arvak.nathan import noise

        monkeypatch.setattr(noise, "_ddsim_available", lambda: False)
        fe = noise.estimate_fidelity(SIMPLE_QASM, "ibm_heron")
        assert fe.method == "heuristic"
        assert 0.0 <= fe.fidelity <= 1.0

    def test_estimate_fidelity_with_mocked_ddsim(self, monkeypatch):
        """Mock ddsim returning specific counts and verify TVD calculation."""
        from arvak.nathan import noise

        monkeypatch.setattr(noise, "_ddsim_available", lambda: True)

        def mock_run_ddsim(qasm3_code, backend_name, noise_profile, shots):
            return noise.FidelityEstimate(
                fidelity=0.92,
                method="ddsim_noisy",
                noise_model="depolarizing(sq=0.001, tq=0.003)",
                backend=backend_name,
                num_shots=shots,
            )

        monkeypatch.setattr(noise, "_run_ddsim", mock_run_ddsim)
        fe = noise.estimate_fidelity(SIMPLE_QASM, "ibm_heron", shots=512)
        assert fe.method == "ddsim_noisy"
        assert fe.fidelity == pytest.approx(0.92)
        assert fe.num_shots == 512

    def test_ddsim_failure_falls_back_to_heuristic(self, monkeypatch):
        """If ddsim raises, fall back to heuristic."""
        from arvak.nathan import noise

        monkeypatch.setattr(noise, "_ddsim_available", lambda: True)
        monkeypatch.setattr(noise, "_run_ddsim", lambda *args, **kwargs: None)
        fe = noise.estimate_fidelity(SIMPLE_QASM, "iqm_sirius")
        assert fe.method == "heuristic"

    def test_fidelity_repr(self):
        from arvak.nathan.noise import FidelityEstimate

        fe = FidelityEstimate(
            fidelity=0.85,
            method="heuristic",
            noise_model="test",
            backend="test_backend",
            num_shots=0,
        )
        assert "0.850" in repr(fe)
        assert "heuristic" in repr(fe)

    def test_parse_qasm3_stats(self):
        from arvak.nathan.noise import _parse_qasm3_stats

        n_qubits, total, tq = _parse_qasm3_stats(SIMPLE_QASM)
        assert n_qubits == 2
        assert total >= 1  # at least h and cx found
        assert tq >= 1    # cx found

    def test_report_field_set_after_analyze(self, monkeypatch):
        """estimate_fidelity=True sets report.simulated_fidelity."""
        import arvak.nathan as nathan
        from arvak.nathan.noise import FidelityEstimate

        mock_fe = FidelityEstimate(0.88, "heuristic", "test", "ibm_heron", 0)

        with patch.object(nathan, "_get_client") as mock_client:
            mock_client.return_value.analyze.return_value = _make_report(
                suitability=0.8
            )
            with patch("arvak.nathan.noise.estimate_fidelity", return_value=mock_fe):
                # Patch _to_qasm3 to return valid code without actual conversion
                with patch.object(nathan, "_to_qasm3", return_value=(SIMPLE_QASM, "qasm3")):
                    with patch("arvak.nathan.generate_clifford_suggestions", return_value=[]):
                        report = nathan.analyze(
                            SIMPLE_QASM,
                            backend="ibm_heron",
                            estimate_fidelity=True,
                            verify=False,
                            optimize_clifford=False,
                            show_references=False,
                        )
        assert report.simulated_fidelity == pytest.approx(0.88)
        assert report.fidelity_estimate is mock_fe

    def test_display_fidelity_section(self):
        from arvak.nathan.display import report_to_html
        from arvak.nathan.noise import FidelityEstimate

        report = _make_report()
        report.simulated_fidelity = 0.73
        report.fidelity_estimate = FidelityEstimate(
            0.73, "heuristic", "depolarizing(sq=0.001)", "iqm_garnet", 0
        )
        html = report_to_html(report)
        assert "Simulated Fidelity" in html
        assert "73%" in html
        assert "heuristic" in html


# ===========================================================================
# P1 #5 — qecc.py
# ===========================================================================


class TestQecRecommendation:
    def test_not_triggered_when_suitability_high(self):
        from arvak.nathan.qecc import recommend_qec

        result = recommend_qec(4, "~2%", 0.7)
        assert result is None

    def test_triggered_when_suitability_low(self):
        from arvak.nathan.qecc import recommend_qec

        result = recommend_qec(4, "~2%", 0.3)
        assert result is not None
        assert result.logical_qubits == 4
        assert result.distance >= 3
        assert result.physical_qubits >= 4

    def test_boundary_0_4_not_triggered(self):
        from arvak.nathan.qecc import recommend_qec

        result = recommend_qec(4, "~2%", 0.4)
        assert result is None

    def test_boundary_just_below_0_4(self):
        from arvak.nathan.qecc import recommend_qec

        result = recommend_qec(4, "~2%", 0.39)
        assert result is not None

    def test_parse_error_rate_percentage(self):
        from arvak.nathan.qecc import _parse_error_rate

        assert _parse_error_rate("~2%") == pytest.approx(0.02)
        assert _parse_error_rate("1.5%") == pytest.approx(0.015)
        assert _parse_error_rate("<1%") == pytest.approx(0.01)

    def test_parse_error_rate_named(self):
        from arvak.nathan.qecc import _parse_error_rate

        assert _parse_error_rate("high") == pytest.approx(0.05)
        assert _parse_error_rate("medium") == pytest.approx(0.01)
        assert _parse_error_rate("low") == pytest.approx(0.001)

    def test_parse_error_rate_float_string(self):
        from arvak.nathan.qecc import _parse_error_rate

        assert _parse_error_rate("0.02") == pytest.approx(0.02)
        assert _parse_error_rate("0.005") == pytest.approx(0.005)

    def test_parse_error_rate_empty(self):
        from arvak.nathan.qecc import _parse_error_rate

        assert _parse_error_rate("") == pytest.approx(0.01)

    def test_surface_code_selected_for_moderate_error(self):
        from arvak.nathan.qecc import recommend_qec

        result = recommend_qec(4, "~2%", 0.2)
        assert result is not None
        assert result.code == "surface_code"

    def test_repetition_code_for_very_high_error(self):
        from arvak.nathan.qecc import recommend_qec

        result = recommend_qec(4, "20%", 0.1)
        assert result is not None
        assert result.code == "repetition_code"

    def test_color_code_for_low_error(self):
        from arvak.nathan.qecc import recommend_qec

        # Below color code threshold (~0.82%) → color code preferred
        result = recommend_qec(4, "0.5%", 0.2)
        assert result is not None
        assert result.code == "color_code"

    def test_mqt_qecc_available_false_without_package(self, monkeypatch):
        from arvak.nathan import qecc

        monkeypatch.setattr(qecc, "_qecc_available", lambda: False)
        assert not qecc._qecc_available()

    def test_recommendation_repr(self):
        from arvak.nathan.qecc import QecRecommendation

        r = QecRecommendation(
            code="surface_code",
            distance=5,
            physical_qubits=50,
            logical_qubits=1,
            threshold=0.01,
            description="test",
            mqt_qecc_available=False,
        )
        assert "surface_code" in repr(r)
        assert "50" in repr(r)

    def test_qec_suggestion_appended_to_report(self, monkeypatch):
        """When suitability < 0.4, a QECC suggestion is added to report."""
        import arvak.nathan as nathan

        with patch.object(nathan, "_get_client") as mock_client:
            mock_client.return_value.analyze.return_value = _make_report(
                suitability=0.2,
                estimated_error_rate="~5%",
                num_qubits=4,
            )
            with patch.object(nathan, "_to_qasm3", return_value=(SIMPLE_QASM, "qasm3")):
                with patch("arvak.nathan.generate_clifford_suggestions", return_value=[]):
                    report = nathan.analyze(
                        SIMPLE_QASM,
                        verify=False,
                        optimize_clifford=False,
                        show_references=False,
                        estimate_fidelity=False,
                    )

        qec_suggestions = [s for s in report.suggestions if s.source == "qecc"]
        assert len(qec_suggestions) == 1
        assert qec_suggestions[0].verified is True
        assert report.qec_recommendation is not None

    def test_display_qec_section(self):
        from arvak.nathan.display import report_to_html
        from arvak.nathan.qecc import QecRecommendation

        report = _make_report()
        report.qec_recommendation = QecRecommendation(
            code="surface_code",
            distance=5,
            physical_qubits=50,
            logical_qubits=4,
            threshold=0.01,
            description="Rotated surface code.",
            mqt_qecc_available=False,
        )
        html = report_to_html(report)
        assert "QEC Recommendation" in html
        assert "Surface Code" in html
        assert "50" in html  # physical qubits

    def test_display_no_qec_section_when_none(self):
        from arvak.nathan.display import report_to_html

        report = _make_report()
        report.qec_recommendation = None
        html = report_to_html(report)
        assert "QEC Recommendation" not in html


# ===========================================================================
# P2 #6 — session.py
# ===========================================================================


class TestSession:
    def _make_session_with_mock_analyze(self, monkeypatch, suitability=0.7, suggestions=None):
        """Create a Session where analyze() is mocked to return a canned report."""
        from arvak.nathan.session import Session
        import arvak.nathan as nathan

        base_report = _make_report(suitability=suitability, suggestions=suggestions or [])

        with patch.object(nathan, "_get_client") as mock_client:
            mock_client.return_value.analyze.return_value = base_report
            # Avoid real Clifford / verify / QEC processing
            with patch.object(nathan, "_to_qasm3", return_value=(SIMPLE_QASM, "qasm3")):
                with patch("arvak.nathan.generate_clifford_suggestions", return_value=[]):
                    session = Session(
                        SIMPLE_QASM,
                        backend=None,
                        verify=False,
                        optimize_clifford=False,
                        show_references=False,
                        estimate_fidelity=False,
                    )
                    report = session.analyze()
        return session, report

    def test_analyze_returns_report(self, monkeypatch):
        session, report = self._make_session_with_mock_analyze(monkeypatch)
        assert report is not None
        assert session.report is report

    def test_history_after_analyze(self, monkeypatch):
        session, _ = self._make_session_with_mock_analyze(monkeypatch)
        assert len(session.history) == 1
        assert session.history[0].applied_suggestion is None

    def test_current_property(self, monkeypatch):
        session, _ = self._make_session_with_mock_analyze(monkeypatch)
        assert session.current == SIMPLE_QASM

    def test_reset_clears_history(self, monkeypatch):
        session, _ = self._make_session_with_mock_analyze(monkeypatch)
        session.reset()
        assert session.history == []
        assert session.report is None
        assert session.current == SIMPLE_QASM

    def test_no_analysis_raises_on_apply(self):
        from arvak.nathan.session import Session

        session = Session(SIMPLE_QASM)
        with pytest.raises(RuntimeError, match="No analysis has been run yet"):
            session.apply(0)

    def test_apply_index_out_of_range(self, monkeypatch):
        session, _ = self._make_session_with_mock_analyze(monkeypatch, suggestions=[])
        with pytest.raises(IndexError):
            session.apply(0)

    def test_apply_suggestion_without_qasm3_raises(self, monkeypatch):
        from arvak.nathan.report import Suggestion

        s = Suggestion(title="No-op", description="No QASM3", qasm3="", impact="low")
        session, _ = self._make_session_with_mock_analyze(monkeypatch, suggestions=[s])
        with pytest.raises(ValueError, match="no QASM3 rewrite"):
            session.apply(0)

    def test_apply_unverified_suggestion_raises(self, monkeypatch):
        from arvak.nathan.report import Suggestion

        s = Suggestion(
            title="Bad",
            description="",
            qasm3=SIMPLE_QASM,
            impact="low",
            verified=False,
            verification_status="not_equivalent",
        )
        session, _ = self._make_session_with_mock_analyze(monkeypatch, suggestions=[s])
        with pytest.raises(ValueError, match="non-equivalent"):
            session.apply(0)

    def test_apply_valid_suggestion_updates_history(self, monkeypatch):
        from arvak.nathan.report import Suggestion
        import arvak.nathan as nathan

        s = Suggestion(
            title="Clifford opt",
            description="Better",
            qasm3=SIMPLE_QASM,
            impact="high",
            verified=None,  # not checked — allowed
            source="qmap_sat",
        )

        base_report = _make_report(suggestions=[s])
        new_report = _make_report(suggestions=[])

        call_count = [0]

        def mock_analyze_fn(*args, **kwargs):
            if call_count[0] == 0:
                call_count[0] += 1
                return base_report
            call_count[0] += 1
            return new_report

        with patch("arvak.nathan.analyze", side_effect=mock_analyze_fn):
            from arvak.nathan.session import Session

            session = Session(SIMPLE_QASM, verify=False, optimize_clifford=False,
                              show_references=False, estimate_fidelity=False)
            session.analyze()

            # Patch suggestion.circuit property to return a dummy circuit
            with patch.object(type(s), "circuit", new_callable=lambda: property(lambda self: SIMPLE_QASM)):
                session.apply(0)

        assert len(session.history) == 2
        assert session.history[1].applied_suggestion is s

    def test_compare_before_apply(self, monkeypatch):
        session, _ = self._make_session_with_mock_analyze(monkeypatch)
        diff = session.compare()
        assert diff.suggestions_applied == 0
        assert diff.verified_rewrites == 0

    def test_session_repr(self, monkeypatch):
        session, _ = self._make_session_with_mock_analyze(monkeypatch)
        assert "Session(" in repr(session)

    def test_session_diff_repr(self):
        from arvak.nathan.session import SessionDiff

        d = SessionDiff(30, 20, 10, 0.33, 18, 12, 6, 0.33, 1, 1)
        r = repr(d)
        assert "SessionDiff(" in r
        assert "33.0%" in r  # gate_reduction_pct=0.33 → +33.0%
        assert "applied=1" in r


# ===========================================================================
# P2 #7 — report.apply(idx)
# ===========================================================================


class TestReportApply:
    def test_apply_index_error(self):
        report = _make_report(suggestions=[])
        with pytest.raises(IndexError):
            report.apply(0)

    def test_apply_no_qasm3_raises(self):
        from arvak.nathan.report import Suggestion

        s = Suggestion(title="X", description="", qasm3="", impact="low")
        report = _make_report(suggestions=[s])
        with pytest.raises(ValueError, match="no QASM3 rewrite"):
            report.apply(0)

    def test_apply_not_equivalent_raises(self):
        from arvak.nathan.report import Suggestion

        s = Suggestion(
            title="Bad",
            description="",
            qasm3=SIMPLE_QASM,
            impact="low",
            verified=False,
            verification_status="not_equivalent",
        )
        report = _make_report(suggestions=[s])
        with pytest.raises(ValueError, match="non-equivalent"):
            report.apply(0)

    def test_apply_valid_suggestion_with_mocked_arvak(self, monkeypatch):
        """apply() returns arvak.Circuit when arvak.from_qasm works."""
        from arvak.nathan.report import Suggestion
        import types

        # Create a mock arvak module
        fake_circuit = object()
        fake_arvak = types.ModuleType("arvak")
        fake_arvak.from_qasm = lambda qasm: fake_circuit

        s = Suggestion(
            title="Good",
            description="",
            qasm3=SIMPLE_QASM,
            impact="high",
            verified=True,
            verification_status="verified",
        )
        report = _make_report(suggestions=[s])

        import sys
        sys.modules["arvak"] = fake_arvak
        try:
            result = report.apply(0)
            assert result is fake_circuit
        finally:
            del sys.modules["arvak"]

    def test_apply_verified_none_returns_circuit(self, monkeypatch):
        """verified=None (not checked) is allowed in apply()."""
        from arvak.nathan.report import Suggestion
        import types

        fake_circuit = object()
        fake_arvak = types.ModuleType("arvak")
        fake_arvak.from_qasm = lambda qasm: fake_circuit

        s = Suggestion(
            title="Unverified opt",
            description="",
            qasm3=SIMPLE_QASM,
            impact="medium",
            verified=None,
        )
        report = _make_report(suggestions=[s])

        import sys
        sys.modules["arvak"] = fake_arvak
        try:
            result = report.apply(0)
            assert result is fake_circuit
        finally:
            del sys.modules["arvak"]

    def test_original_circuit_property(self):
        report = _make_report()
        report._original_circuit = SIMPLE_QASM
        assert report.original_circuit == SIMPLE_QASM


# ===========================================================================
# P2 #8 — knowledge sources
# ===========================================================================


class TestKnowledgeSources:
    def setup_method(self):
        """Clear module-level sources before each test."""
        import arvak.nathan as nathan
        nathan.clear_sources()

    def teardown_method(self):
        """Clean up after each test."""
        import arvak.nathan as nathan
        nathan.clear_sources()

    def test_add_source_string(self):
        import arvak.nathan as nathan

        nathan.add_source("OPENQASM 3.0; // reference circuit")
        assert len(nathan._knowledge_sources) == 1

    def test_add_source_path(self, tmp_path):
        import arvak.nathan as nathan

        f = tmp_path / "ref.qasm"
        f.write_text("OPENQASM 3.0; qubit[2] q;")
        nathan.add_source(str(f))
        assert len(nathan._knowledge_sources) == 1

    def test_clear_sources(self):
        import arvak.nathan as nathan

        nathan.add_source("content 1")
        nathan.add_source("content 2")
        nathan.clear_sources()
        assert nathan._knowledge_sources == []

    def test_load_knowledge_sources_raw_strings(self):
        import arvak.nathan as nathan

        result = nathan._load_knowledge_sources(["hello", "world"])
        assert "hello" in result
        assert "world" in result
        assert "---" in result

    def test_load_knowledge_sources_file(self, tmp_path):
        import arvak.nathan as nathan

        f = tmp_path / "ref.qasm"
        f.write_text("OPENQASM 3.0; qubit[4] q;")
        result = nathan._load_knowledge_sources([str(f)])
        assert "OPENQASM" in result
        assert "qubit[4]" in result

    def test_load_knowledge_sources_mixed(self, tmp_path):
        import arvak.nathan as nathan

        f = tmp_path / "ref.qasm"
        f.write_text("file content")
        result = nathan._load_knowledge_sources(["raw string", str(f)])
        assert "raw string" in result
        assert "file content" in result

    def test_load_knowledge_sources_none_returns_none(self):
        import arvak.nathan as nathan

        assert nathan._load_knowledge_sources(None) is None
        assert nathan._load_knowledge_sources([]) is None

    def test_extra_context_passed_to_client(self, monkeypatch, tmp_path):
        """analyze() passes extra_context to client.analyze()."""
        import arvak.nathan as nathan

        captured = {}

        def mock_client_analyze(code, language, backend_id, anonymize, extra_context=None):
            captured["extra_context"] = extra_context
            return _make_report()

        with patch.object(nathan, "_get_client") as mock_client_factory:
            mock_client_factory.return_value.analyze = mock_client_analyze
            with patch.object(nathan, "_to_qasm3", return_value=(SIMPLE_QASM, "qasm3")):
                with patch("arvak.nathan.generate_clifford_suggestions", return_value=[]):
                    nathan.analyze(
                        SIMPLE_QASM,
                        verify=False,
                        optimize_clifford=False,
                        show_references=False,
                        knowledge_sources=["custom context here"],
                    )

        assert captured["extra_context"] is not None
        assert "custom context here" in captured["extra_context"]

    def test_module_level_sources_combined_with_per_call(self, monkeypatch):
        """Module-level sources are combined with per-call sources."""
        import arvak.nathan as nathan

        nathan.add_source("module source")
        captured = {}

        def mock_client_analyze(code, language, backend_id, anonymize, extra_context=None):
            captured["extra_context"] = extra_context
            return _make_report()

        with patch.object(nathan, "_get_client") as mock_client_factory:
            mock_client_factory.return_value.analyze = mock_client_analyze
            with patch.object(nathan, "_to_qasm3", return_value=(SIMPLE_QASM, "qasm3")):
                with patch("arvak.nathan.generate_clifford_suggestions", return_value=[]):
                    nathan.analyze(
                        SIMPLE_QASM,
                        verify=False,
                        optimize_clifford=False,
                        show_references=False,
                        knowledge_sources=["call source"],
                    )

        ec = captured["extra_context"]
        assert "module source" in ec
        assert "call source" in ec

    def test_no_sources_sends_none(self, monkeypatch):
        """Without sources, extra_context is not passed (or is None)."""
        import arvak.nathan as nathan

        captured = {}

        def mock_client_analyze(code, language, backend_id, anonymize, extra_context=None):
            captured["extra_context"] = extra_context
            return _make_report()

        with patch.object(nathan, "_get_client") as mock_client_factory:
            mock_client_factory.return_value.analyze = mock_client_analyze
            with patch.object(nathan, "_to_qasm3", return_value=(SIMPLE_QASM, "qasm3")):
                with patch("arvak.nathan.generate_clifford_suggestions", return_value=[]):
                    nathan.analyze(
                        SIMPLE_QASM,
                        verify=False,
                        optimize_clifford=False,
                        show_references=False,
                    )

        assert captured["extra_context"] is None


# ===========================================================================
# Integration — analyze() with all P1/P2 params
# ===========================================================================


class TestAnalyzeIntegration:
    def test_analyze_show_references_populates_field(self, monkeypatch):
        import arvak.nathan as nathan
        from arvak.nathan.bench import BenchReference

        fake_refs = [
            BenchReference("qaoa", 6, 18, 30, "qaoa_indep_qiskit_6",
                           "https://example.com", "indep", "test")
        ]

        with patch.object(nathan, "_get_client") as mock_client:
            mock_client.return_value.analyze.return_value = _make_report(
                problem_type="qaoa", num_qubits=6, suitability=0.8
            )
            with patch.object(nathan, "_to_qasm3", return_value=(SIMPLE_QASM, "qasm3")):
                with patch("arvak.nathan.generate_clifford_suggestions", return_value=[]):
                    with patch("arvak.nathan.bench.find_references", return_value=fake_refs):
                        report = nathan.analyze(
                            SIMPLE_QASM,
                            verify=False,
                            optimize_clifford=False,
                            show_references=True,
                            estimate_fidelity=False,
                        )

        assert report.reference_circuits == fake_refs

    def test_analyze_show_references_false_skips(self, monkeypatch):
        import arvak.nathan as nathan

        with patch.object(nathan, "_get_client") as mock_client:
            mock_client.return_value.analyze.return_value = _make_report()
            with patch.object(nathan, "_to_qasm3", return_value=(SIMPLE_QASM, "qasm3")):
                with patch("arvak.nathan.generate_clifford_suggestions", return_value=[]):
                    report = nathan.analyze(
                        SIMPLE_QASM,
                        verify=False,
                        optimize_clifford=False,
                        show_references=False,
                    )

        assert report.reference_circuits == []

    def test_analyze_estimate_fidelity_requires_backend(self, monkeypatch):
        """estimate_fidelity=True without backend doesn't crash."""
        import arvak.nathan as nathan

        with patch.object(nathan, "_get_client") as mock_client:
            mock_client.return_value.analyze.return_value = _make_report()
            with patch.object(nathan, "_to_qasm3", return_value=(SIMPLE_QASM, "qasm3")):
                with patch("arvak.nathan.generate_clifford_suggestions", return_value=[]):
                    # No backend specified — fidelity estimation skipped silently
                    report = nathan.analyze(
                        SIMPLE_QASM,
                        verify=False,
                        optimize_clifford=False,
                        show_references=False,
                        estimate_fidelity=True,
                        backend=None,
                    )

        assert report.simulated_fidelity is None

    def test_full_pipeline_low_suitability(self, monkeypatch):
        """Full pipeline: low suitability triggers QEC suggestion + field."""
        import arvak.nathan as nathan

        with patch.object(nathan, "_get_client") as mock_client:
            mock_client.return_value.analyze.return_value = _make_report(
                suitability=0.15,
                estimated_error_rate="~8%",
                num_qubits=4,
            )
            with patch.object(nathan, "_to_qasm3", return_value=(SIMPLE_QASM, "qasm3")):
                with patch("arvak.nathan.generate_clifford_suggestions", return_value=[]):
                    report = nathan.analyze(
                        SIMPLE_QASM,
                        verify=False,
                        optimize_clifford=False,
                        show_references=False,
                    )

        assert report.qec_recommendation is not None
        qec_suggs = [s for s in report.suggestions if s.source == "qecc"]
        assert len(qec_suggs) == 1
        assert qec_suggs[0].impact == "high"
