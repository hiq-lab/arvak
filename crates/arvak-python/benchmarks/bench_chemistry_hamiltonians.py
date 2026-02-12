"""Benchmark: 100 molecular Hamiltonians through the Arvak compilation pipeline.

Hamiltonians sourced from recent quantum computing publications (2019-2025):
  - Google Quantum AI (Nature 2020, 2023): H2, H3+, H4/H6 chains
  - IBM Quantum (Nature 2017, PRX 2023): LiH, BeH2, H2O
  - Xanadu/PennyLane demos (2023-2025): H2, LiH, HeH+, H3+
  - Cao et al. Chem. Rev. 2019: N2, HF, NH3, CH4
  - Elfving et al. PRX Quantum 2021: BH3, Li2
  - O'Brien et al. PRX Quantum 2022: F2, C2H2

Pipeline: qml.qchem.molecular_hamiltonian() -> PennyLane tape -> Arvak circuit -> QASM
"""

import pytest
import numpy as np
import time

try:
    import arvak
    import pennylane as qml
    from arvak.integrations.pennylane import pennylane_to_arvak

    AVAILABLE = True
except ImportError:
    AVAILABLE = False

pytestmark = pytest.mark.skipif(not AVAILABLE, reason="arvak/pennylane not installed")


# ---------------------------------------------------------------------------
# Hamiltonian database — 100 entries
# (name, symbols, coords_flat, charge, mult, basis, active_electrons, active_orbitals, source)
# All coordinates in Angstroms.  active_electrons/active_orbitals = None means full-space.
# ---------------------------------------------------------------------------
MOLECULES = [
    # ── H2 (10): equilibrium + stretched geometries ──────────────────────
    ("H2_0.50A", ["H", "H"], [0, 0, 0, 0, 0, 0.50], 0, 1, "sto-3g", None, None,
     "Google 2020 / baseline"),
    ("H2_0.60A", ["H", "H"], [0, 0, 0, 0, 0, 0.60], 0, 1, "sto-3g", None, None,
     "Google 2020 / baseline"),
    ("H2_0.735A", ["H", "H"], [0, 0, 0, 0, 0, 0.735], 0, 1, "sto-3g", None, None,
     "Equilibrium geometry"),
    ("H2_0.90A", ["H", "H"], [0, 0, 0, 0, 0, 0.90], 0, 1, "sto-3g", None, None,
     "Google 2020 / stretched"),
    ("H2_1.00A", ["H", "H"], [0, 0, 0, 0, 0, 1.00], 0, 1, "sto-3g", None, None,
     "Google 2020 / stretched"),
    ("H2_1.20A", ["H", "H"], [0, 0, 0, 0, 0, 1.20], 0, 1, "sto-3g", None, None,
     "Google 2020 / stretched"),
    ("H2_1.50A", ["H", "H"], [0, 0, 0, 0, 0, 1.50], 0, 1, "sto-3g", None, None,
     "Google 2020 / stretched"),
    ("H2_1.80A", ["H", "H"], [0, 0, 0, 0, 0, 1.80], 0, 1, "sto-3g", None, None,
     "Google 2020 / stretched"),
    ("H2_2.00A", ["H", "H"], [0, 0, 0, 0, 0, 2.00], 0, 1, "sto-3g", None, None,
     "Google 2020 / near-dissociation"),
    ("H2_2.50A", ["H", "H"], [0, 0, 0, 0, 0, 2.50], 0, 1, "sto-3g", None, None,
     "Google 2020 / dissociation"),

    # ── HeH+ (5): simplest heteronuclear ─────────────────────────────────
    ("HeH+_0.50A", ["He", "H"], [0, 0, 0, 0, 0, 0.50], 1, 1, "sto-3g", None, None,
     "Xanadu demos"),
    ("HeH+_0.775A", ["He", "H"], [0, 0, 0, 0, 0, 0.775], 1, 1, "sto-3g", None, None,
     "Xanadu demos / equilibrium"),
    ("HeH+_1.00A", ["He", "H"], [0, 0, 0, 0, 0, 1.00], 1, 1, "sto-3g", None, None,
     "Xanadu demos"),
    ("HeH+_1.25A", ["He", "H"], [0, 0, 0, 0, 0, 1.25], 1, 1, "sto-3g", None, None,
     "Xanadu demos"),
    ("HeH+_1.50A", ["He", "H"], [0, 0, 0, 0, 0, 1.50], 1, 1, "sto-3g", None, None,
     "Xanadu demos"),

    # ── H3+ (5): equilateral triangle at various sizes ───────────────────
    ("H3+_0.70A", ["H", "H", "H"],
     [0, 0, 0, 0.70, 0, 0, 0.35, 0.70 * np.sqrt(3) / 2, 0],
     1, 1, "sto-3g", None, None, "Google 2020"),
    ("H3+_0.87A", ["H", "H", "H"],
     [0, 0, 0, 0.87, 0, 0, 0.435, 0.87 * np.sqrt(3) / 2, 0],
     1, 1, "sto-3g", None, None, "Google 2020 / equilibrium"),
    ("H3+_1.00A", ["H", "H", "H"],
     [0, 0, 0, 1.00, 0, 0, 0.50, 1.00 * np.sqrt(3) / 2, 0],
     1, 1, "sto-3g", None, None, "Google 2020"),
    ("H3+_1.20A", ["H", "H", "H"],
     [0, 0, 0, 1.20, 0, 0, 0.60, 1.20 * np.sqrt(3) / 2, 0],
     1, 1, "sto-3g", None, None, "Google 2020"),
    ("H3+_1.50A", ["H", "H", "H"],
     [0, 0, 0, 1.50, 0, 0, 0.75, 1.50 * np.sqrt(3) / 2, 0],
     1, 1, "sto-3g", None, None, "Google 2020"),

    # ── H4 linear chain (5) ──────────────────────────────────────────────
    ("H4_0.80A", ["H", "H", "H", "H"],
     [0, 0, 0, 0, 0, 0.80, 0, 0, 1.60, 0, 0, 2.40], 0, 1, "sto-3g", None, None,
     "Google 2023 / hydrogen chain"),
    ("H4_1.00A", ["H", "H", "H", "H"],
     [0, 0, 0, 0, 0, 1.00, 0, 0, 2.00, 0, 0, 3.00], 0, 1, "sto-3g", None, None,
     "Google 2023 / hydrogen chain"),
    ("H4_1.20A", ["H", "H", "H", "H"],
     [0, 0, 0, 0, 0, 1.20, 0, 0, 2.40, 0, 0, 3.60], 0, 1, "sto-3g", None, None,
     "Google 2023 / hydrogen chain"),
    ("H4_1.50A", ["H", "H", "H", "H"],
     [0, 0, 0, 0, 0, 1.50, 0, 0, 3.00, 0, 0, 4.50], 0, 1, "sto-3g", None, None,
     "Google 2023 / hydrogen chain"),
    ("H4_2.00A", ["H", "H", "H", "H"],
     [0, 0, 0, 0, 0, 2.00, 0, 0, 4.00, 0, 0, 6.00], 0, 1, "sto-3g", None, None,
     "Google 2023 / hydrogen chain"),

    # ── H6 linear chain (3) ──────────────────────────────────────────────
    ("H6_0.80A", ["H", "H", "H", "H", "H", "H"],
     [0, 0, 0, 0, 0, 0.80, 0, 0, 1.60, 0, 0, 2.40, 0, 0, 3.20, 0, 0, 4.00],
     0, 1, "sto-3g", 4, 4, "Google 2023 / scaling test"),
    ("H6_1.00A", ["H", "H", "H", "H", "H", "H"],
     [0, 0, 0, 0, 0, 1.00, 0, 0, 2.00, 0, 0, 3.00, 0, 0, 4.00, 0, 0, 5.00],
     0, 1, "sto-3g", 4, 4, "Google 2023 / scaling test"),
    ("H6_1.50A", ["H", "H", "H", "H", "H", "H"],
     [0, 0, 0, 0, 0, 1.50, 0, 0, 3.00, 0, 0, 4.50, 0, 0, 6.00, 0, 0, 7.50],
     0, 1, "sto-3g", 4, 4, "Google 2023 / scaling test"),

    # ── LiH (10): primary benchmark (IBM, Google) ────────────────────────
    ("LiH_0.80A", ["Li", "H"], [0, 0, 0, 0, 0, 0.80], 0, 1, "sto-3g", 2, 5,
     "IBM 2017 / VQE benchmark"),
    ("LiH_1.00A", ["Li", "H"], [0, 0, 0, 0, 0, 1.00], 0, 1, "sto-3g", 2, 5,
     "IBM 2017"),
    ("LiH_1.20A", ["Li", "H"], [0, 0, 0, 0, 0, 1.20], 0, 1, "sto-3g", 2, 5,
     "IBM 2017"),
    ("LiH_1.546A", ["Li", "H"], [0, 0, 0, 0, 0, 1.546], 0, 1, "sto-3g", 2, 5,
     "Equilibrium geometry"),
    ("LiH_1.80A", ["Li", "H"], [0, 0, 0, 0, 0, 1.80], 0, 1, "sto-3g", 2, 5,
     "IBM 2017"),
    ("LiH_2.00A", ["Li", "H"], [0, 0, 0, 0, 0, 2.00], 0, 1, "sto-3g", 2, 5,
     "IBM 2017"),
    ("LiH_2.30A", ["Li", "H"], [0, 0, 0, 0, 0, 2.30], 0, 1, "sto-3g", 2, 5,
     "IBM 2017"),
    ("LiH_2.50A", ["Li", "H"], [0, 0, 0, 0, 0, 2.50], 0, 1, "sto-3g", 2, 5,
     "Google 2020"),
    ("LiH_3.00A", ["Li", "H"], [0, 0, 0, 0, 0, 3.00], 0, 1, "sto-3g", 2, 5,
     "Google 2020 / dissociation"),
    ("LiH_3.50A", ["Li", "H"], [0, 0, 0, 0, 0, 3.50], 0, 1, "sto-3g", 2, 5,
     "Google 2020 / dissociation"),

    # ── BeH2 (8): symmetric stretch (IBM) ────────────────────────────────
    ("BeH2_1.00A", ["Be", "H", "H"],
     [0, 0, 0, 0, 0, 1.00, 0, 0, -1.00], 0, 1, "sto-3g", 2, 3,
     "IBM 2017"),
    ("BeH2_1.10A", ["Be", "H", "H"],
     [0, 0, 0, 0, 0, 1.10, 0, 0, -1.10], 0, 1, "sto-3g", 2, 3,
     "IBM 2017"),
    ("BeH2_1.20A", ["Be", "H", "H"],
     [0, 0, 0, 0, 0, 1.20, 0, 0, -1.20], 0, 1, "sto-3g", 2, 3,
     "IBM 2017"),
    ("BeH2_1.334A", ["Be", "H", "H"],
     [0, 0, 0, 0, 0, 1.334, 0, 0, -1.334], 0, 1, "sto-3g", 2, 3,
     "Equilibrium geometry"),
    ("BeH2_1.50A", ["Be", "H", "H"],
     [0, 0, 0, 0, 0, 1.50, 0, 0, -1.50], 0, 1, "sto-3g", 2, 3,
     "IBM 2017"),
    ("BeH2_1.80A", ["Be", "H", "H"],
     [0, 0, 0, 0, 0, 1.80, 0, 0, -1.80], 0, 1, "sto-3g", 2, 3,
     "IBM 2017"),
    ("BeH2_2.00A", ["Be", "H", "H"],
     [0, 0, 0, 0, 0, 2.00, 0, 0, -2.00], 0, 1, "sto-3g", 2, 3,
     "IBM 2017"),
    ("BeH2_2.50A", ["Be", "H", "H"],
     [0, 0, 0, 0, 0, 2.50, 0, 0, -2.50], 0, 1, "sto-3g", 2, 3,
     "IBM 2017 / dissociation"),

    # ── HF (5): polar diatomic ───────────────────────────────────────────
    ("HF_0.70A", ["H", "F"], [0, 0, 0, 0, 0, 0.70], 0, 1, "sto-3g", 6, 4,
     "Cao et al. 2019"),
    ("HF_0.80A", ["H", "F"], [0, 0, 0, 0, 0, 0.80], 0, 1, "sto-3g", 6, 4,
     "Cao et al. 2019"),
    ("HF_0.917A", ["H", "F"], [0, 0, 0, 0, 0, 0.917], 0, 1, "sto-3g", 6, 4,
     "Equilibrium geometry"),
    ("HF_1.10A", ["H", "F"], [0, 0, 0, 0, 0, 1.10], 0, 1, "sto-3g", 6, 4,
     "Cao et al. 2019"),
    ("HF_1.50A", ["H", "F"], [0, 0, 0, 0, 0, 1.50], 0, 1, "sto-3g", 6, 4,
     "Cao et al. 2019 / stretched"),

    # ── BH3 (3): planar molecule ─────────────────────────────────────────
    ("BH3_1.10A", ["B", "H", "H", "H"],
     [0, 0, 0, 1.10, 0, 0, -0.55, 1.10 * np.sqrt(3) / 2, 0, -0.55, -1.10 * np.sqrt(3) / 2, 0],
     0, 1, "sto-3g", 2, 4, "Elfving et al. 2021"),
    ("BH3_1.19A", ["B", "H", "H", "H"],
     [0, 0, 0, 1.19, 0, 0, -0.595, 1.19 * np.sqrt(3) / 2, 0, -0.595, -1.19 * np.sqrt(3) / 2, 0],
     0, 1, "sto-3g", 2, 4, "Equilibrium geometry"),
    ("BH3_1.30A", ["B", "H", "H", "H"],
     [0, 0, 0, 1.30, 0, 0, -0.65, 1.30 * np.sqrt(3) / 2, 0, -0.65, -1.30 * np.sqrt(3) / 2, 0],
     0, 1, "sto-3g", 2, 4, "Elfving et al. 2021"),

    # ── H2O (8): O-H stretch + angle variations (the workhorse) ─────────
    ("H2O_0.80A", ["O", "H", "H"],
     [0, 0, 0, 0.757 * (0.80 / 0.957), 0.586 * (0.80 / 0.957), 0,
      -0.757 * (0.80 / 0.957), 0.586 * (0.80 / 0.957), 0],
     0, 1, "sto-3g", 4, 4, "IBM 2017"),
    ("H2O_0.90A", ["O", "H", "H"],
     [0, 0, 0, 0.757 * (0.90 / 0.957), 0.586 * (0.90 / 0.957), 0,
      -0.757 * (0.90 / 0.957), 0.586 * (0.90 / 0.957), 0],
     0, 1, "sto-3g", 4, 4, "IBM 2017"),
    ("H2O_eq", ["O", "H", "H"],
     [0, 0, 0, 0.757, 0.586, 0, -0.757, 0.586, 0],
     0, 1, "sto-3g", 4, 4, "IBM 2017 / equilibrium"),
    ("H2O_1.10A", ["O", "H", "H"],
     [0, 0, 0, 0.757 * (1.10 / 0.957), 0.586 * (1.10 / 0.957), 0,
      -0.757 * (1.10 / 0.957), 0.586 * (1.10 / 0.957), 0],
     0, 1, "sto-3g", 4, 4, "IBM 2017 / stretched"),
    ("H2O_1.30A", ["O", "H", "H"],
     [0, 0, 0, 0.757 * (1.30 / 0.957), 0.586 * (1.30 / 0.957), 0,
      -0.757 * (1.30 / 0.957), 0.586 * (1.30 / 0.957), 0],
     0, 1, "sto-3g", 4, 4, "IBM 2017 / stretched"),
    ("H2O_1.50A", ["O", "H", "H"],
     [0, 0, 0, 0.757 * (1.50 / 0.957), 0.586 * (1.50 / 0.957), 0,
      -0.757 * (1.50 / 0.957), 0.586 * (1.50 / 0.957), 0],
     0, 1, "sto-3g", 4, 4, "IBM 2017 / dissociation"),
    ("H2O_narrow", ["O", "H", "H"],
     [0, 0, 0, 0.586, 0.757, 0, -0.586, 0.757, 0],
     0, 1, "sto-3g", 4, 4, "Angle variation (narrow)"),
    ("H2O_wide", ["O", "H", "H"],
     [0, 0, 0, 0.900, 0.350, 0, -0.900, 0.350, 0],
     0, 1, "sto-3g", 4, 4, "Angle variation (wide)"),

    # ── NH3 (5): pyramidal molecule ──────────────────────────────────────
    ("NH3_0.90A", ["N", "H", "H", "H"],
     [0, 0, 0, 0, -0.9377 * (0.90 / 1.012), -0.3816 * (0.90 / 1.012),
      0.8121 * (0.90 / 1.012), 0.4689 * (0.90 / 1.012), -0.3816 * (0.90 / 1.012),
      -0.8121 * (0.90 / 1.012), 0.4689 * (0.90 / 1.012), -0.3816 * (0.90 / 1.012)],
     0, 1, "sto-3g", 2, 4, "Cao et al. 2019"),
    ("NH3_1.012A", ["N", "H", "H", "H"],
     [0, 0, 0, 0, -0.9377, -0.3816, 0.8121, 0.4689, -0.3816, -0.8121, 0.4689, -0.3816],
     0, 1, "sto-3g", 2, 4, "Equilibrium geometry"),
    ("NH3_1.10A", ["N", "H", "H", "H"],
     [0, 0, 0, 0, -0.9377 * (1.10 / 1.012), -0.3816 * (1.10 / 1.012),
      0.8121 * (1.10 / 1.012), 0.4689 * (1.10 / 1.012), -0.3816 * (1.10 / 1.012),
      -0.8121 * (1.10 / 1.012), 0.4689 * (1.10 / 1.012), -0.3816 * (1.10 / 1.012)],
     0, 1, "sto-3g", 2, 4, "Cao et al. 2019"),
    ("NH3_1.20A", ["N", "H", "H", "H"],
     [0, 0, 0, 0, -0.9377 * (1.20 / 1.012), -0.3816 * (1.20 / 1.012),
      0.8121 * (1.20 / 1.012), 0.4689 * (1.20 / 1.012), -0.3816 * (1.20 / 1.012),
      -0.8121 * (1.20 / 1.012), 0.4689 * (1.20 / 1.012), -0.3816 * (1.20 / 1.012)],
     0, 1, "sto-3g", 2, 4, "Cao et al. 2019"),
    ("NH3_1.40A", ["N", "H", "H", "H"],
     [0, 0, 0, 0, -0.9377 * (1.40 / 1.012), -0.3816 * (1.40 / 1.012),
      0.8121 * (1.40 / 1.012), 0.4689 * (1.40 / 1.012), -0.3816 * (1.40 / 1.012),
      -0.8121 * (1.40 / 1.012), 0.4689 * (1.40 / 1.012), -0.3816 * (1.40 / 1.012)],
     0, 1, "sto-3g", 2, 4, "Cao et al. 2019 / stretched"),

    # ── Li2 (5): homonuclear metal dimer ─────────────────────────────────
    ("Li2_2.00A", ["Li", "Li"], [0, 0, 0, 0, 0, 2.00], 0, 1, "sto-3g", 2, 4,
     "Elfving et al. 2021"),
    ("Li2_2.40A", ["Li", "Li"], [0, 0, 0, 0, 0, 2.40], 0, 1, "sto-3g", 2, 4,
     "Elfving et al. 2021"),
    ("Li2_2.673A", ["Li", "Li"], [0, 0, 0, 0, 0, 2.673], 0, 1, "sto-3g", 2, 4,
     "Equilibrium geometry"),
    ("Li2_3.00A", ["Li", "Li"], [0, 0, 0, 0, 0, 3.00], 0, 1, "sto-3g", 2, 4,
     "Elfving et al. 2021"),
    ("Li2_4.00A", ["Li", "Li"], [0, 0, 0, 0, 0, 4.00], 0, 1, "sto-3g", 2, 4,
     "Elfving et al. 2021 / dissociation"),

    # ── N2 (5): triple bond dissociation ─────────────────────────────────
    ("N2_0.80A", ["N", "N"], [0, 0, 0, 0, 0, 0.80], 0, 1, "sto-3g", 2, 4,
     "Cao et al. 2019"),
    ("N2_0.90A", ["N", "N"], [0, 0, 0, 0, 0, 0.90], 0, 1, "sto-3g", 2, 4,
     "Cao et al. 2019"),
    ("N2_1.098A", ["N", "N"], [0, 0, 0, 0, 0, 1.098], 0, 1, "sto-3g", 2, 4,
     "Equilibrium geometry"),
    ("N2_1.40A", ["N", "N"], [0, 0, 0, 0, 0, 1.40], 0, 1, "sto-3g", 2, 4,
     "Cao et al. 2019 / stretched"),
    ("N2_1.80A", ["N", "N"], [0, 0, 0, 0, 0, 1.80], 0, 1, "sto-3g", 2, 4,
     "Cao et al. 2019 / dissociation"),

    # ── OH- (3): closed-shell hydroxide anion ────────────────────────────
    ("OH-_0.80A", ["O", "H"], [0, 0, 0, 0, 0, 0.80], -1, 1, "sto-3g", 2, 2,
     "O'Brien et al. 2022 / hydroxide"),
    ("OH-_0.970A", ["O", "H"], [0, 0, 0, 0, 0, 0.970], -1, 1, "sto-3g", 2, 2,
     "Equilibrium geometry"),
    ("OH-_1.20A", ["O", "H"], [0, 0, 0, 0, 0, 1.20], -1, 1, "sto-3g", 2, 2,
     "O'Brien et al. 2022 / stretched"),

    # ── CH2 singlet (3): carbene ─────────────────────────────────────────
    ("CH2_1.00A", ["C", "H", "H"],
     [0, 0, 0, 0, 1.00, 0, 0, -1.00 * np.cos(np.radians(50)), 1.00 * np.sin(np.radians(50))],
     0, 1, "sto-3g", 2, 4, "Singlet carbene"),
    ("CH2_1.085A", ["C", "H", "H"],
     [0, 0, 0, 0, 1.085, 0, 0, -1.085 * np.cos(np.radians(50)), 1.085 * np.sin(np.radians(50))],
     0, 1, "sto-3g", 2, 4, "Equilibrium singlet carbene"),
    ("CH2_1.20A", ["C", "H", "H"],
     [0, 0, 0, 0, 1.20, 0, 0, -1.20 * np.cos(np.radians(50)), 1.20 * np.sin(np.radians(50))],
     0, 1, "sto-3g", 2, 4, "Singlet carbene / stretched"),

    # ── CH4 (3): tetrahedral methane ─────────────────────────────────────
    ("CH4_1.00A", ["C", "H", "H", "H", "H"],
     [0, 0, 0,
      0.577, 0.577, 0.577,
      -0.577, -0.577, 0.577,
      0.577, -0.577, -0.577,
      -0.577, 0.577, -0.577],
     0, 1, "sto-3g", 2, 4, "Cao et al. 2019"),
    ("CH4_1.089A", ["C", "H", "H", "H", "H"],
     [0, 0, 0,
      0.629, 0.629, 0.629,
      -0.629, -0.629, 0.629,
      0.629, -0.629, -0.629,
      -0.629, 0.629, -0.629],
     0, 1, "sto-3g", 2, 4, "Equilibrium geometry"),
    ("CH4_1.20A", ["C", "H", "H", "H", "H"],
     [0, 0, 0,
      0.693, 0.693, 0.693,
      -0.693, -0.693, 0.693,
      0.693, -0.693, -0.693,
      -0.693, 0.693, -0.693],
     0, 1, "sto-3g", 2, 4, "Cao et al. 2019 / stretched"),

    # ── C2H2 (2): acetylene ──────────────────────────────────────────────
    ("C2H2_eq", ["C", "C", "H", "H"],
     [0, 0, 0.601, 0, 0, -0.601, 0, 0, 1.665, 0, 0, -1.665],
     0, 1, "sto-3g", 2, 4, "O'Brien et al. 2022"),
    ("C2H2_1.30A", ["C", "C", "H", "H"],
     [0, 0, 0.650, 0, 0, -0.650, 0, 0, 1.714, 0, 0, -1.714],
     0, 1, "sto-3g", 2, 4, "O'Brien et al. 2022 / stretched"),

    # ── F2 (3): fluorine dimer ───────────────────────────────────────────
    ("F2_1.00A", ["F", "F"], [0, 0, 0, 0, 0, 1.00], 0, 1, "sto-3g", 4, 3,
     "O'Brien et al. 2022"),
    ("F2_1.412A", ["F", "F"], [0, 0, 0, 0, 0, 1.412], 0, 1, "sto-3g", 4, 3,
     "Equilibrium geometry"),
    ("F2_2.00A", ["F", "F"], [0, 0, 0, 0, 0, 2.00], 0, 1, "sto-3g", 4, 3,
     "O'Brien et al. 2022 / dissociation"),

    # ── Extra LiH with different active space (2) ────────────────────────
    ("LiH_eq_fullAS", ["Li", "H"], [0, 0, 0, 0, 0, 1.546], 0, 1, "sto-3g", 4, 6,
     "Full active space LiH"),
    ("LiH_2.00A_fullAS", ["Li", "H"], [0, 0, 0, 0, 0, 2.00], 0, 1, "sto-3g", 4, 6,
     "Full active space LiH stretched"),

    # ── Extra H2O with different active space (2) ────────────────────────
    ("H2O_eq_small", ["O", "H", "H"],
     [0, 0, 0, 0.757, 0.586, 0, -0.757, 0.586, 0],
     0, 1, "sto-3g", 2, 2, "Minimal active space"),
    ("H2O_eq_large", ["O", "H", "H"],
     [0, 0, 0, 0.757, 0.586, 0, -0.757, 0.586, 0],
     0, 1, "sto-3g", 6, 5, "Larger active space"),

    # ── Extra N2 with larger active space (2) ────────────────────────────
    ("N2_eq_large", ["N", "N"], [0, 0, 0, 0, 0, 1.098], 0, 1, "sto-3g", 6, 6,
     "Larger active space N2"),
    ("N2_1.40A_large", ["N", "N"], [0, 0, 0, 0, 0, 1.40], 0, 1, "sto-3g", 6, 6,
     "Larger active space N2 stretched"),

    # ── Extra BeH2 with different active space (2) ───────────────────────
    ("BeH2_eq_4e4o", ["Be", "H", "H"],
     [0, 0, 0, 0, 0, 1.334, 0, 0, -1.334], 0, 1, "sto-3g", 4, 4,
     "Larger active space BeH2"),
    ("BeH2_2.00A_4e4o", ["Be", "H", "H"],
     [0, 0, 0, 0, 0, 2.00, 0, 0, -2.00], 0, 1, "sto-3g", 4, 4,
     "Larger active space BeH2 stretched"),

    # ── Extra HF with larger active space (2) ────────────────────────────
    ("HF_eq_large", ["H", "F"], [0, 0, 0, 0, 0, 0.917], 0, 1, "sto-3g", 8, 5,
     "Larger active space HF"),
]

assert len(MOLECULES) == 100, f"Expected 100 molecules, got {len(MOLECULES)}"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _total_electrons(symbols, charge):
    """Compute total electrons from atomic symbols and charge."""
    Z = {"H": 1, "He": 2, "Li": 3, "Be": 4, "B": 5, "C": 6, "N": 7, "O": 8, "F": 9}
    return sum(Z[s] for s in symbols) - charge


def _build_hamiltonian(mol):
    """Build Hamiltonian and return (H, n_qubits, active_electrons)."""
    name, symbols, coords, charge, mult, basis, ae, ao, source = mol
    kwargs = dict(charge=charge, mult=mult, basis=basis)
    if ae is not None:
        kwargs["active_electrons"] = ae
        kwargs["active_orbitals"] = ao
    H, n_qubits = qml.qchem.molecular_hamiltonian(
        symbols, np.array(coords, dtype=float), **kwargs
    )
    electrons = ae if ae is not None else _total_electrons(symbols, charge)
    return H, n_qubits, electrons


def _build_vqe_circuit(mol):
    """Build VQE circuit and compile through Arvak. Returns (arvak_circuit, qasm)."""
    H, n_qubits, electrons = _build_hamiltonian(mol)

    hf_state = qml.qchem.hf_state(electrons=electrons, orbitals=n_qubits)
    singles, doubles = qml.qchem.excitations(electrons=electrons, orbitals=n_qubits)

    dev = qml.device("default.qubit", wires=n_qubits)

    @qml.qnode(dev)
    def circuit(params):
        qml.BasisState(hf_state, wires=range(n_qubits))
        for i, d in enumerate(doubles):
            qml.DoubleExcitation(params[i], wires=d)
        for i, s in enumerate(singles):
            qml.SingleExcitation(params[len(doubles) + i], wires=s)
        return qml.expval(H)

    n_params = len(singles) + len(doubles)
    params = np.zeros(n_params)

    arvak_circuit = pennylane_to_arvak(circuit, params)
    qasm = arvak.to_qasm(arvak_circuit)
    return arvak_circuit, qasm, n_qubits


# ---------------------------------------------------------------------------
# Test classes
# ---------------------------------------------------------------------------


class TestHamiltonianGeneration:
    """Test that all 100 Hamiltonians build without error."""

    @pytest.mark.parametrize("mol", MOLECULES, ids=[m[0] for m in MOLECULES])
    def test_build_hamiltonian(self, mol):
        H, n_qubits, electrons = _build_hamiltonian(mol)
        assert n_qubits >= 2, f"Expected >= 2 qubits, got {n_qubits}"
        assert len(H.operands) > 0, "Hamiltonian has no terms"
        assert electrons >= 1, f"Expected >= 1 electron, got {electrons}"


class TestArvakCompilation:
    """Test that VQE circuits for all 100 Hamiltonians compile through Arvak."""

    @pytest.mark.parametrize("mol", MOLECULES, ids=[m[0] for m in MOLECULES])
    def test_compile_vqe_circuit(self, mol):
        arvak_circuit, qasm, n_qubits = _build_vqe_circuit(mol)
        assert "OPENQASM" in qasm, "QASM output missing OPENQASM header"
        assert arvak_circuit.num_qubits == n_qubits, (
            f"Qubit mismatch: arvak={arvak_circuit.num_qubits}, expected={n_qubits}"
        )


class TestCompilationThroughput:
    """Benchmark compilation speed across all 100 Hamiltonians."""

    def test_total_throughput(self):
        results = []
        total_start = time.time()

        for mol in MOLECULES:
            name = mol[0]
            t0 = time.time()
            try:
                arvak_circuit, qasm, n_qubits = _build_vqe_circuit(mol)
                dt = time.time() - t0
                results.append({
                    "name": name,
                    "qubits": n_qubits,
                    "qasm_len": len(qasm),
                    "time_s": dt,
                    "ok": True,
                })
            except Exception as e:
                dt = time.time() - t0
                results.append({
                    "name": name,
                    "qubits": 0,
                    "qasm_len": 0,
                    "time_s": dt,
                    "ok": False,
                })

        total_time = time.time() - total_start
        passed = sum(1 for r in results if r["ok"])
        failed = sum(1 for r in results if not r["ok"])
        avg_time = np.mean([r["time_s"] for r in results if r["ok"]])
        max_qubits = max((r["qubits"] for r in results if r["ok"]), default=0)

        print(f"\n{'=' * 60}")
        print(f"  Arvak Chemistry Benchmark — {len(MOLECULES)} Hamiltonians")
        print(f"{'=' * 60}")
        print(f"  Passed:        {passed}/{len(MOLECULES)}")
        print(f"  Failed:        {failed}")
        print(f"  Total time:    {total_time:.1f}s")
        print(f"  Avg per mol:   {avg_time:.3f}s")
        print(f"  Max qubits:    {max_qubits}")
        print(f"{'=' * 60}")

        if failed:
            for r in results:
                if not r["ok"]:
                    print(f"  FAILED: {r['name']}")

        assert passed == len(MOLECULES), f"{failed} molecules failed compilation"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
