"""Tests for arvak.predictor — circuit feature extraction and device prediction."""

from __future__ import annotations

import pytest

from arvak.predictor.features import (
    CircuitFeatures,
    extract_features,
    _compute_depth,
    _compute_program_communication,
    _compute_critical_depth,
    _compute_parallelism,
    _parse_gates,
    _parse_num_qubits,
)
from arvak.predictor.device import (
    DevicePrediction,
    DeviceScore,
    KNOWN_DEVICES,
    _predict_heuristic,
    _predictor_available,
    _score_device,
    predict_device,
    rank_devices,
)


# ---------------------------------------------------------------------------
# Test circuits
# ---------------------------------------------------------------------------

BELL_CIRCUIT = """\
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0],q[1];
"""

GHZ_5 = """\
OPENQASM 3.0;
qubit[5] q;
h q[0];
cx q[0],q[1];
cx q[1],q[2];
cx q[2],q[3];
cx q[3],q[4];
"""

PARALLEL_CIRCUIT = """\
OPENQASM 3.0;
qubit[4] q;
h q[0];
h q[1];
h q[2];
h q[3];
"""

MIXED_CIRCUIT = """\
OPENQASM 3.0;
qubit[3] q;
h q[0];
cx q[0],q[1];
rz(0.5) q[2];
cx q[1],q[2];
h q[0];
s q[1];
"""

DEEP_SERIAL = """\
OPENQASM 3.0;
qubit[1] q;
h q[0];
s q[0];
h q[0];
s q[0];
h q[0];
s q[0];
h q[0];
s q[0];
"""

EMPTY_CIRCUIT = "OPENQASM 3.0;\nqubit[2] q;\n"


# ---------------------------------------------------------------------------
# Feature extraction: qubit parsing
# ---------------------------------------------------------------------------


class TestParseNumQubits:
    def test_single_register(self):
        assert _parse_num_qubits(BELL_CIRCUIT) == 2

    def test_five_qubits(self):
        assert _parse_num_qubits(GHZ_5) == 5

    def test_multiple_registers(self):
        code = "OPENQASM 3.0;\nqubit[3] q;\nqubit[2] r;\n"
        assert _parse_num_qubits(code) == 5

    def test_qreg_syntax(self):
        code = "OPENQASM 2.0;\nqreg q[4];\n"
        assert _parse_num_qubits(code) == 4

    def test_no_register_defaults_to_1(self):
        assert _parse_num_qubits("h q[0];") == 1


# ---------------------------------------------------------------------------
# Feature extraction: gate parsing
# ---------------------------------------------------------------------------


class TestParseGates:
    def test_bell_gates(self):
        gates = _parse_gates(BELL_CIRCUIT)
        assert len(gates) == 2
        assert gates[0][0] == "h"
        assert gates[0][1] == [0]
        assert gates[1][0] == "cx"
        assert sorted(gates[1][1]) == [0, 1]

    def test_parametric_gate(self):
        gates = _parse_gates(MIXED_CIRCUIT)
        names = [g[0] for g in gates]
        assert "rz" in names

    def test_empty_circuit(self):
        gates = _parse_gates(EMPTY_CIRCUIT)
        assert gates == []

    def test_multi_register(self):
        code = "OPENQASM 3.0;\nqubit[2] q;\nqubit[2] r;\ncx q[0],r[1];\n"
        gates = _parse_gates(code)
        assert len(gates) == 1
        # q[0] = index 0, r[1] = index 2+1 = 3
        assert sorted(gates[0][1]) == [0, 3]


# ---------------------------------------------------------------------------
# Feature extraction: depth computation
# ---------------------------------------------------------------------------


class TestComputeDepth:
    def test_bell_depth(self):
        gates = [("h", [0]), ("cx", [0, 1])]
        assert _compute_depth(gates, 2) == 2

    def test_parallel_depth(self):
        gates = [("h", [0]), ("h", [1]), ("h", [2]), ("h", [3])]
        assert _compute_depth(gates, 4) == 1

    def test_serial_depth(self):
        gates = [("h", [0]), ("s", [0]), ("h", [0]), ("s", [0])]
        assert _compute_depth(gates, 1) == 4

    def test_empty(self):
        assert _compute_depth([], 2) == 0

    def test_ghz_depth(self):
        gates = [
            ("h", [0]),
            ("cx", [0, 1]),
            ("cx", [1, 2]),
            ("cx", [2, 3]),
            ("cx", [3, 4]),
        ]
        assert _compute_depth(gates, 5) == 5


# ---------------------------------------------------------------------------
# Feature extraction: program communication
# ---------------------------------------------------------------------------


class TestProgramCommunication:
    def test_bell_communication(self):
        # Bell: q0-q1 interact. 1 pair out of C(2,2) = 1 pair → 1.0
        multi_gates = [("cx", [0, 1])]
        edges, comm = _compute_program_communication(multi_gates, 2)
        assert comm == 1.0
        assert (0, 1) in edges

    def test_ghz_communication(self):
        # GHZ-5: pairs (0,1), (1,2), (2,3), (3,4) → 4 out of C(5,2)=10 → 0.4
        multi_gates = [
            ("cx", [0, 1]), ("cx", [1, 2]),
            ("cx", [2, 3]), ("cx", [3, 4]),
        ]
        edges, comm = _compute_program_communication(multi_gates, 5)
        assert len(edges) == 4
        assert abs(comm - 0.4) < 0.01

    def test_no_multi_qubit(self):
        edges, comm = _compute_program_communication([], 3)
        assert comm == 0.0
        assert edges == []

    def test_full_communication(self):
        # All pairs interact
        multi_gates = [
            ("cx", [0, 1]), ("cx", [0, 2]),
            ("cx", [1, 2]),
        ]
        edges, comm = _compute_program_communication(multi_gates, 3)
        assert comm == 1.0


# ---------------------------------------------------------------------------
# Feature extraction: critical depth
# ---------------------------------------------------------------------------


class TestCriticalDepth:
    def test_bell_critical_depth(self):
        # Both gates on critical path, 1 of 2 is multi-qubit
        gates = [("h", [0]), ("cx", [0, 1])]
        cd = _compute_critical_depth(gates, 2)
        assert cd == 0.5

    def test_all_single_qubit(self):
        gates = [("h", [0]), ("s", [0]), ("h", [0])]
        cd = _compute_critical_depth(gates, 1)
        assert cd == 0.0

    def test_all_multi_qubit(self):
        gates = [("cx", [0, 1]), ("cx", [1, 2]), ("cx", [2, 3])]
        cd = _compute_critical_depth(gates, 4)
        assert cd > 0.0

    def test_empty(self):
        cd = _compute_critical_depth([], 2)
        assert cd == 0.0


# ---------------------------------------------------------------------------
# Feature extraction: parallelism
# ---------------------------------------------------------------------------


class TestParallelism:
    def test_fully_parallel(self):
        gates = [("h", [0]), ("h", [1]), ("h", [2]), ("h", [3])]
        p = _compute_parallelism(gates, 4, 1)
        assert p == 0.75  # 1 - 1/4

    def test_fully_serial(self):
        gates = [("h", [0]), ("s", [0]), ("h", [0]), ("s", [0])]
        p = _compute_parallelism(gates, 1, 4)
        assert p == 0.0

    def test_empty(self):
        p = _compute_parallelism([], 2, 0)
        assert p == 0.0

    def test_single_gate(self):
        p = _compute_parallelism([("h", [0])], 1, 1)
        assert p == 0.0


# ---------------------------------------------------------------------------
# Full feature extraction
# ---------------------------------------------------------------------------


class TestExtractFeatures:
    def test_bell_features(self):
        f = extract_features(BELL_CIRCUIT)
        assert f.num_qubits == 2
        assert f.depth == 2
        assert f.num_gates == 2
        assert f.num_single_qubit_gates == 1
        assert f.num_multi_qubit_gates == 1
        assert f.entanglement_ratio == 0.5
        assert f.program_communication == 1.0

    def test_ghz5_features(self):
        f = extract_features(GHZ_5)
        assert f.num_qubits == 5
        assert f.num_gates == 5
        assert f.num_multi_qubit_gates == 4
        assert f.entanglement_ratio == 0.8
        assert 0.3 <= f.program_communication <= 0.5

    def test_parallel_features(self):
        f = extract_features(PARALLEL_CIRCUIT)
        assert f.num_qubits == 4
        assert f.depth == 1
        assert f.num_gates == 4
        assert f.num_multi_qubit_gates == 0
        assert f.entanglement_ratio == 0.0
        assert f.parallelism == 0.75

    def test_empty_circuit(self):
        f = extract_features(EMPTY_CIRCUIT)
        assert f.num_qubits == 2
        assert f.num_gates == 0
        assert f.depth == 0

    def test_serial_circuit(self):
        f = extract_features(DEEP_SERIAL)
        assert f.num_qubits == 1
        assert f.depth == 8
        assert f.parallelism == 0.0

    def test_gate_counts(self):
        f = extract_features(BELL_CIRCUIT)
        assert f.gate_counts["h"] == 1
        assert f.gate_counts["cx"] == 1

    def test_mixed_circuit(self):
        f = extract_features(MIXED_CIRCUIT)
        assert f.num_qubits == 3
        assert f.num_gates == 6
        assert f.num_multi_qubit_gates == 2
        assert "rz" in f.gate_counts

    def test_to_dict(self):
        f = extract_features(BELL_CIRCUIT)
        d = f.to_dict()
        assert "num_qubits" in d
        assert "depth" in d
        assert "program_communication" in d
        assert "entanglement_ratio" in d
        assert d["num_qubits"] == 2

    def test_to_predictor_features(self):
        f = extract_features(BELL_CIRCUIT)
        vec = f.to_predictor_features()
        assert len(vec) == 7
        assert vec[0] == 2.0  # num_qubits
        assert vec[1] == 2.0  # depth

    def test_repr(self):
        f = extract_features(BELL_CIRCUIT)
        r = repr(f)
        assert "qubits=2" in r
        assert "depth=2" in r

    def test_interaction_graph(self):
        f = extract_features(GHZ_5)
        assert len(f.qubit_interaction_graph) == 4
        assert (0, 1) in f.qubit_interaction_graph


# ---------------------------------------------------------------------------
# CircuitFeatures dataclass
# ---------------------------------------------------------------------------


class TestCircuitFeatures:
    def test_defaults(self):
        f = CircuitFeatures()
        assert f.num_qubits == 0
        assert f.gate_counts == {}

    def test_to_dict_keys(self):
        f = CircuitFeatures(num_qubits=5, depth=10)
        d = f.to_dict()
        expected_keys = {
            "num_qubits", "depth", "num_gates",
            "num_single_qubit_gates", "num_multi_qubit_gates",
            "program_communication", "critical_depth",
            "entanglement_ratio", "parallelism",
        }
        assert set(d.keys()) == expected_keys


# ---------------------------------------------------------------------------
# Device scoring
# ---------------------------------------------------------------------------


class TestScoreDevice:
    def test_circuit_too_large(self):
        features = CircuitFeatures(num_qubits=200, depth=10)
        device_info = {"num_qubits": 100, "topology": "full", "is_simulator": False}
        score, reason = _score_device(features, device_info, "expected_fidelity")
        assert score == 0.0
        assert "needs 200" in reason

    def test_good_fit(self):
        features = CircuitFeatures(
            num_qubits=10, depth=5,
            program_communication=0.3,
        )
        device_info = {
            "num_qubits": 20,
            "topology": "full",
            "is_simulator": False,
        }
        score, reason = _score_device(features, device_info, "expected_fidelity")
        assert score > 0.5

    def test_simulator_lower_score(self):
        features = CircuitFeatures(num_qubits=5, depth=3)
        sim_info = {"num_qubits": 100, "topology": "full", "is_simulator": True}
        qpu_info = {"num_qubits": 100, "topology": "full", "is_simulator": False}
        sim_score, _ = _score_device(features, sim_info, "expected_fidelity")
        qpu_score, _ = _score_device(features, qpu_info, "expected_fidelity")
        assert qpu_score > sim_score


# ---------------------------------------------------------------------------
# Heuristic prediction
# ---------------------------------------------------------------------------


class TestPredictHeuristic:
    def test_returns_prediction(self):
        features = CircuitFeatures(num_qubits=5, depth=10, program_communication=0.5)
        prediction = _predict_heuristic(features, "expected_fidelity")
        assert isinstance(prediction, DevicePrediction)
        assert prediction.method == "heuristic"
        assert prediction.device != ""
        assert len(prediction.ranking) > 0

    def test_ranking_sorted(self):
        features = CircuitFeatures(num_qubits=5, depth=10)
        prediction = _predict_heuristic(features, "expected_fidelity")
        scores = [ds.score for ds in prediction.ranking]
        assert scores == sorted(scores, reverse=True)

    def test_high_connectivity_prefers_full(self):
        features = CircuitFeatures(
            num_qubits=10,
            depth=20,
            program_communication=0.9,
        )
        prediction = _predict_heuristic(features, "expected_fidelity")
        # Full-connectivity devices should rank higher
        top_device = prediction.device
        top_info = KNOWN_DEVICES.get(top_device, {})
        assert top_info.get("topology") == "full"

    def test_large_circuit_filters_small_devices(self):
        features = CircuitFeatures(num_qubits=50, depth=100)
        prediction = _predict_heuristic(features, "expected_fidelity")
        # All ranked devices should have enough qubits
        for ds in prediction.ranking:
            if ds.score > 0:
                device_info = KNOWN_DEVICES.get(ds.device, {})
                assert device_info.get("num_qubits", 0) >= 50

    def test_custom_device_list(self):
        features = CircuitFeatures(num_qubits=5, depth=3)
        prediction = _predict_heuristic(
            features, "expected_fidelity",
            devices=["ibm_torino", "iqm_garnet"],
        )
        assert len(prediction.ranking) == 2
        device_names = {ds.device for ds in prediction.ranking}
        assert device_names == {"ibm_torino", "iqm_garnet"}


# ---------------------------------------------------------------------------
# predict_device (top-level)
# ---------------------------------------------------------------------------


class TestPredictDevice:
    def test_predict_from_qasm(self):
        prediction = predict_device(BELL_CIRCUIT)
        assert isinstance(prediction, DevicePrediction)
        assert prediction.device != ""
        assert prediction.features is not None
        assert prediction.features.num_qubits == 2

    def test_predict_with_figure_of_merit(self):
        p1 = predict_device(GHZ_5, figure_of_merit="expected_fidelity")
        p2 = predict_device(GHZ_5, figure_of_merit="critical_depth")
        # Both should return valid predictions
        assert p1.device != ""
        assert p2.device != ""

    def test_predict_device_repr(self):
        prediction = predict_device(BELL_CIRCUIT)
        r = repr(prediction)
        assert "DevicePrediction" in r
        assert "heuristic" in r


# ---------------------------------------------------------------------------
# rank_devices
# ---------------------------------------------------------------------------


class TestRankDevices:
    def test_rank_returns_list(self):
        ranking = rank_devices(BELL_CIRCUIT)
        assert isinstance(ranking, list)
        assert len(ranking) > 0
        assert all(isinstance(ds, DeviceScore) for ds in ranking)

    def test_rank_sorted_descending(self):
        ranking = rank_devices(GHZ_5)
        scores = [ds.score for ds in ranking]
        assert scores == sorted(scores, reverse=True)


# ---------------------------------------------------------------------------
# DeviceScore / DevicePrediction dataclasses
# ---------------------------------------------------------------------------


class TestDeviceScore:
    def test_repr(self):
        ds = DeviceScore(device="ibm_torino", score=0.85, reason="good fit")
        r = repr(ds)
        assert "ibm_torino" in r
        assert "0.850" in r


class TestDevicePrediction:
    def test_repr(self):
        dp = DevicePrediction(device="iqm_garnet", method="heuristic")
        r = repr(dp)
        assert "iqm_garnet" in r
        assert "heuristic" in r


# ---------------------------------------------------------------------------
# KNOWN_DEVICES constant
# ---------------------------------------------------------------------------


class TestKnownDevices:
    def test_has_expected_devices(self):
        assert "ibm_torino" in KNOWN_DEVICES
        assert "iqm_garnet" in KNOWN_DEVICES
        assert "quantinuum_h1" in KNOWN_DEVICES

    def test_device_has_required_fields(self):
        for name, info in KNOWN_DEVICES.items():
            assert "num_qubits" in info, f"{name} missing num_qubits"
            assert "topology" in info, f"{name} missing topology"
            assert "is_simulator" in info, f"{name} missing is_simulator"


# ---------------------------------------------------------------------------
# Integration: analyze(predict_device=True)
# ---------------------------------------------------------------------------


class TestAnalyzeWithDevicePrediction:
    def test_analyze_with_prediction(self, monkeypatch):
        import arvak.nathan as nathan_mod

        mock_report = nathan_mod.AnalysisReport(
            summary="test",
            suggestions=[],
        )

        class MockClient:
            def analyze(self, **kwargs):
                return mock_report

        monkeypatch.setattr(nathan_mod, "_get_client", lambda: MockClient())
        monkeypatch.setattr(
            nathan_mod, "_to_qasm3",
            lambda c, l: ("OPENQASM 3.0;\nqubit[2] q;\nh q[0];\ncx q[0],q[1];", "qasm3"),
        )

        report = nathan_mod.analyze(
            "OPENQASM 3.0;",
            verify=False,
            optimize_clifford=False,
            predict_device=True,
        )
        assert report.recommended_device != ""
        assert len(report.device_ranking) > 0

    def test_analyze_prediction_disabled(self, monkeypatch):
        import arvak.nathan as nathan_mod

        mock_report = nathan_mod.AnalysisReport(suggestions=[])

        class MockClient:
            def analyze(self, **kwargs):
                return mock_report

        monkeypatch.setattr(nathan_mod, "_get_client", lambda: MockClient())
        monkeypatch.setattr(nathan_mod, "_to_qasm3", lambda c, l: ("OPENQASM 3.0;", "qasm3"))

        report = nathan_mod.analyze(
            "code", verify=False, optimize_clifford=False, predict_device=False,
        )
        assert report.recommended_device == ""
        assert report.device_ranking == []


# ---------------------------------------------------------------------------
# Display rendering with device ranking
# ---------------------------------------------------------------------------


class TestDisplayDeviceRanking:
    def test_device_ranking_in_html(self):
        from arvak.nathan.report import AnalysisReport
        from arvak.nathan.display import report_to_html

        report = AnalysisReport(
            recommended_device="quantinuum_h1",
            device_ranking=[
                DeviceScore(device="quantinuum_h1", score=0.9, reason="full connectivity"),
                DeviceScore(device="ibm_torino", score=0.6, reason="good fit"),
            ],
        )
        html = report_to_html(report)
        assert "Device Ranking" in html
        assert "quantinuum_h1" in html
        assert "ibm_torino" in html

    def test_no_device_ranking_section(self):
        from arvak.nathan.report import AnalysisReport
        from arvak.nathan.display import report_to_html

        report = AnalysisReport()
        html = report_to_html(report)
        assert "Device Ranking" not in html
