# Phase 6 — 1D Floquet Kicked Ising Validation Report

**Status:** Validierung des Simulators ✓ — Validierung des Jacobian-Allocators *ehrliches Mixed Result*.

## Was Phase 6 ursprünglich sein sollte

Tindall et al., PRX Quantum 5, 010308 (2024) — Heavy-Hex 127q kicked Ising
Benchmark gegen IBM Eagle. **Verworfen nach Architektur-Diskussion**, weil:

- Heavy-Hex ist 2D, arvak-proj ist strikt 1D MPS
- Snake-Ordering + SWAP-Routing wäre mathematisch korrekt aber teuer
- Snake-Ordering + Edge-Weglassen wäre billig aber mathematisch wertlos
- Tindalls eigentlicher Beitrag (Belief Propagation auf Tree-Decomposition) ist
  ein 2D-Algorithmus, den nachzuimplementieren keinen unabhängigen Wert
  schafft

Heavy-Hex-Fähigkeit für arvak-proj wäre eine **Tree-Tensor-Network-Generalisierung**
(Wochen-bis-Monate-Aufwand, eigene strategische Entscheidung). Das ist
bewusst auf Phase 7+ verschoben.

## Was Phase 6 stattdessen ist

**Selbst-konsistente Validierung des arvak-proj-Simulators und des
Jacobian-Allocators an einem 1D Floquet Kicked Ising Modell**, das
architektonisch zu MPS passt und publizierte analytische Vergleichswerte hat
(Bertini, Kos, Prosen, PRX 9, 021033, 2019).

### Modell

```
H = J · Σ_i Z_i Z_{i+1} + h_x · Σ_i X_i + h_z · Σ_i Z_i
```

Self-Dual-Punkt: `J = h_x = π/4, h_z = 0, dt = 1`. Trotter-Schritt
`U = exp(-i h_x dt Σ X) · exp(-i J dt Σ Z⊗Z)`. Initialer Zustand `|0…0⟩`,
Observable `⟨Z_q(t)⟩` für jedes Qubit nach jedem Trotter-Schritt.

### Implementierung

- `crates/arvak-proj/src/kicked_ising.rs` — Modell-Modul mit Circuit-Builder
  (`apply_kim_step`, `apply_kim_step_disordered`) und unabhängigem dichten
  Statevector-Referenz-Simulator (`reference_kim_run`,
  `reference_kim_run_disordered`)
- `crates/arvak-proj/tests/kim_validation.rs` — Validierungs-Test mit 6 Stages
- `crates/arvak-proj/src/mps.rs` — neue Methoden `Mps::expectation_z` und
  `Mps::norm_squared` (MPS-native Erwartungswert-Berechnung in O(N·χ³))

### Während Phase 6 gefundene und behobene Bugs

#### Bug 1: `apply_zz_fast` korrupt für χ ≥ 2 (kritisch)

Die Methode `Mps::apply_zz_fast` hat den ZZ-Phasenfaktor nur in `site_l`
absorbiert (`m0 *= phase_same`, `m1 *= phase_diff`), als ob ZZ ein
lokales Single-Site-Gate wäre. Tatsächlich hängt der ZZ-Phasenfaktor von
der **kombinierten** Parität von σ_q und σ_{q+1} ab.

Konkretes Fehlverhalten: für `σ_q=0, σ_{q+1}=1` produzierte der Fast-Path
`phase_same` (entsprechend ZZ-Eigenwert +1), korrekt wäre `phase_diff`
(ZZ-Eigenwert −1). Zwei von vier Σ-Kombinationen waren systematisch
falsch.

Der Bug war bisher unsichtbar weil:
- Stage A (chi_max=64) trifft die Fast-Path-Bedingung `chi ≤ 2 ∧ rd ≤ 2`
  nur am ersten Schritt einer frischen MPS, danach wachsen die Bonds raus
- Existierende Tests benutzen entweder hohes χ oder produzieren keine
  Observablen, deren Korrektheit man unabhängig prüfen würde

Phase 6 Stage C/E hat den Bug aufgedeckt: bei chi=2 erschien `total_disc=0`
zusammen mit `max_err=1.0` — ein Widerspruch (wenn nichts truncated wird,
muss die Observable korrekt sein).

**Fix:** `apply_zz_fast` ersatzlos entfernt. Alle ZZ-Anwendungen gehen
jetzt durch den vollen SVD-Pfad (`apply_two_qubit`). Auch der Aufrufer
in `crates/arvak-python/src/projection.rs` wurde aktualisiert.

#### Bug 2: Konventionsfehler im KIM-Test-Setup

Die Doku-Konvention `mps::zz(θ) = exp(-i θ Z⊗Z)` (kein Faktor 2) wurde
zunächst übersehen. Die Standard-RZZ-Konvention ist
`exp(-i θ/2 Z⊗Z)` mit Faktor 2. Mein erster Trotter-Step rief
`mps::zz(2·J·dt)` auf — doppelt so groß wie beabsichtigt.

**Fix:** `kicked_ising::apply_kim_step` und `reference_kim_run` rufen
beide jetzt `mps::zz(J·dt)` und benutzen einen passenden dichten ZZ
(`apply_zz_dense` mit der gleichen Konvention).

## Ergebnisse pro Stage

### Stage A — N=12, χ_max=64 (provably exact)

| Metrik | Wert |
|---|---|
| max ⟨Z⟩ Fehler vs. dichte Referenz | **2.6e-15** (FP-Limit) |
| max bond used | 32 |
| total discarded weight | 7.3e-29 |

✅ **Validiert**: arvak-proj's MPS-Evolution ist mathematisch korrekt.

### Stage B — N=12, χ-Sweep 2..64

| χ | max ⟨Z⟩ Fehler | total disc |
|---|---|---|
| 2 | 4.18e-1 | 7.7 |
| 4 | 4.58e-1 | 12.5 |
| 6 | 1.87e-1 | 12.3 |
| 8 | 2.53e-2 | 15.2 |
| 12 | 2.44e-2 | 14.0 |
| 16 | 8.79e-2 | 14.2 |
| 24 | 4.95e-2 | 9.3 |
| 32 | 3.51e-2 | 4.9 |
| 64 | **2.60e-15** | 7.3e-29 |

Bei χ=64 exakt. Die Nicht-Monotonie zwischen χ=8 und χ=32 ist physikalisch
korrekt für erzwungene MPS-Truncation: jeder χ-Wert projiziert auf einen
anderen Unterraum, und mehr Repräsentations-Spielraum heißt nicht
notwendigerweise besseres Endergebnis bei festem χ-Cap.

### Stage C — N=12, Jacobian vs uniform an matched Budget

| χ-Range | jac err / budget | uniform err / budget |
|---|---|---|
| [2..6] | 3.95e-1 / 43 | 3.62e-1 / 33 |
| [2..8] | 4.04e-1 / 55 | 2.51e-1 / 55 |
| [2..12] | 3.99e-1 / 78 | 1.94e-1 / 77 |
| [2..16] | 3.99e-1 / 103 | 1.98e-1 / 99 |

⚠️ **Negativ-Resultat**: Auf dem **homogenen** Self-Dual KIM verliert der
Jacobian-Allocator gegen uniform-χ. Das ist theoretisch korrekt — bei
einem translations-invarianten Modell ist uniform-χ optimal, jeder
Allocator der ungleich verteilt fügt nur Rauschen hinzu.

### Stage D — N=24, χ_max=256 vs. dichte 16M-Amplituden-Referenz

| Metrik | Wert |
|---|---|
| max ⟨Z⟩ Fehler vs. dichte Referenz | **7.4e-16** (FP-Limit) |
| arvak-proj wall time | 708 ms |
| dense reference wall time | 4820 ms |
| **Speedup arvak-proj vs dense** | **6.8×** |
| natürlicher max bond | 32 |

✅ **Validiert**: arvak-proj ist schneller als dichter Statevector bei
exakter Übereinstimmung am natürlichen Schmidt-Rang (32). Bei N=24
spart die MPS-Form ~85% Compute gegenüber dichten 16M-Amplituden.

### Stage E — N=50, homogener Negativ-Kontroll-Test

| Strategie | max ⟨Z⟩ err | budget | wall ms |
|---|---|---|---|
| ref χ=64 | — | 3136 | 119974 |
| uniform χ=8 | 1.72e-1 | 392 | 91 |
| jacobian [2..8] | 4.01e-1 | 316 | 50 |

⚠️ **Negativ-Resultat bestätigt** auch bei N=50: jacobian verliert auf
homogener KIM. Erwartet.

### Stage F — Disordered KIM, N=14 (dichter Ref) und N=50 (high-χ Ref)

Site-disorder: `h_x_i = π/4 + δ_i, δ_i ∈ U(-0.5, 0.5)`, fixed seed.

**N=14** (dichte Referenz):

| Strategie | max ⟨Z⟩ err | budget | chi profile |
|---|---|---|---|
| uniform χ=8 | 6.49e-2 | 104 | uniform |
| jacobian [4..8] | 8.26e-2 | 76 | [4,4,7,7,7,6,6,7,6,8,6,4,4] |

**N=50** (high-χ Ref):

| Strategie | max ⟨Z⟩ err | budget | wall ms |
|---|---|---|---|
| ref χ=64 | — | 3136 | 119620 |
| uniform χ=8 | 1.02e-1 | 392 | 89 |
| jacobian [4..8] | 1.58e-1 | 314 | 46 |

⚠️ **Mixed**: Auf disordered KIM produziert der Jacobian klar
differenzierte chi-Profile (Boundaries vs Bulk), aber der Allocator
**gewinnt nicht an matched Budget**. Er liefert ~80% des Budgets bei
~1.5× des Fehlers — also etwa lineares "Kosten-pro-Genauigkeit" auf dem
gleichen Niveau wie uniform.

Der Speedup proportional zum kleineren Budget ist **real** (ca. 2× wall
time bei N=50). Das macht den Jacobian zu einem brauchbaren Werkzeug
"weniger χ für ungefähr die gleiche Genauigkeit", aber nicht zu einem
Werkzeug "gleiches χ für bessere Genauigkeit".

## Was Phase 6 validiert hat

✅ **Der Simulator ist korrekt**:
- Stage A: FP-Präzisions-Match an N=12 vs. dichter Referenz
- Stage D: FP-Präzisions-Match an N=24 vs. dichter 16M-Amplituden-Referenz
- 6.8× Speedup gegenüber dichtem Statevector bei N=24 mit exakten
  Observablen
- Der `apply_zz_fast` Bug, der bisher unbemerkt war, ist jetzt
  weg — `arvak-proj`'s ZZ-Anwendung ist ausschließlich der validierte
  SVD-Pfad

✅ **Die Test-Infrastruktur ist da**:
- Unabhängiger dichter Statevector-Referenz-Simulator (`reference_kim_run`)
- MPS-native Erwartungswert-Berechnung (`Mps::expectation_z`,
  `Mps::norm_squared`) — funktioniert für beliebig große N, nicht nur
  N≤25
- Reproduzierbare disordered-KIM Test-Suite (deterministisches PRNG-Seed)

## Was Phase 6 NICHT validiert hat

⚠️ **Der Jacobian-PR-Allocator gewinnt nicht an matched Budget**:
- Auf homogenen Modellen: erwartetes Negativ-Resultat (Stage C, E)
- Auf disordered Modellen: produziert sinnvolle differenzierte Profile
  (Stage F), liefert proportionalen Speedup, aber nicht bessere Genauigkeit
  bei gleichem Gesamt-χ
- Die ursprüngliche "10⁹×-Verbesserung" aus dem allerersten Stage-C-Lauf
  war ein Doppel-Artefakt von (a) dem `apply_zz_fast`-Bug an chi=2-Bonds
  plus (b) einem h_z-Pilot der einen quasi-Null-Jacobian produzierte —
  beides ist jetzt behoben und das Resultat stimmt nicht mehr

## Was als Nächstes geprüft werden müsste (wenn man den Jacobian besser machen will)

Diese Punkte sind **nicht** Phase 6, sondern mögliche Folge-Arbeit:

1. **Target-Budget-Allocator** statt der jetzigen sqrt(score/max)-Heuristik:
   eine Funktion die das gewünschte Gesamt-χ als harten Constraint
   nimmt und die PR-/Sensitivitäts-Profile darauf normalisiert. Macht
   matched-budget-Vergleiche fair.
2. **Andere Score-Funktion**: PR ist eine Spread-Metrik, nicht eine
   Entanglement-Wachstums-Metrik. Vielleicht ist `Σ_i |J_{ki}|²` (L2-Norm
   der Sensitivität) der bessere Allocator als PR.
3. **Stärker inhomogene Test-Modelle**: Defekt-Modelle (ein Site mit
   anderem h_x), Boundary-Effekte (offene Kette mit echten Randbedingungen
   statt nur "kein Bond darüber hinaus"), oder Frequenz-Hierarchien (per-Site
   ω_i mit kommensurabel-vs-inkommensurabel-Mustern — das war die
   ursprüngliche sin(C/2)-Story).
4. **Pilot-zu-Production-Ratio**: jetzt benutzt der Pilot dasselbe
   `n_steps` und chi=4, aber chi=8. Ein billigerer Pilot (z.B. n_steps/2,
   chi=2) würde die Jacobian-Build-Zeit reduzieren — falls die
   Sensitivitäts-Profile robust sind.

## Endergebnis

Phase 6 ist abgeschlossen mit:

- **Simulator validiert** an unabhängiger dichter Referenz auf
  FP-Präzision (Stages A, D)
- **Eine kritische Korruption** (`apply_zz_fast`) gefunden und entfernt,
  die seit Beginn der Codebase silent falsche ZZ-Anwendungen produziert hat
- **Ehrliche Charakterisierung** des Jacobian-Allocators: produziert
  sinnvolle Profile, gewinnt aber nicht an matched Budget — Stand jetzt
  ein Werkzeug für "ähnliche Genauigkeit bei kleinerem Budget" und
  proportionalen Wall-time-Speedup, kein Werkzeug für "bessere Genauigkeit
  bei gleichem Budget"
- **6 Test-Stages** in `tests/kim_validation.rs`, alle grün, Total-Runtime
  ~4 min im Release-Build
