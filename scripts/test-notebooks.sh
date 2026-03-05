#!/usr/bin/env bash
# ============================================================
# Arvak Notebook-Tester
#
# Testet alle Jupyter-Notebooks automatisch und erstellt
# einen Bericht. Funktioniert auf macOS und Linux.
#
# Benutzung:
#   cd ~/Projects/Arvak-project   # oder wo das Repo liegt
#   bash scripts/test-notebooks.sh
#
# Was passiert:
#   1. Erstellt eine temporaere Python-Umgebung
#   2. Installiert arvak + alle Abhaengigkeiten von PyPI
#   3. Fuehrt jedes Notebook aus
#   4. Zeigt Ergebnis pro Notebook (PASS / FAIL)
#   5. Speichert den Bericht in notebooks-report.txt
# ============================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NOTEBOOK_DIR="$PROJECT_DIR/crates/arvak-python/notebooks"
REPORT_FILE="$PROJECT_DIR/notebooks-report.txt"
VENV_DIR="$PROJECT_DIR/.venv-notebook-test"

echo "============================================"
echo "  Arvak Notebook-Tester"
echo "============================================"
echo ""

# --- Schritt 1: Python-Umgebung ---
echo "[1/4] Python-Umgebung wird eingerichtet..."

# Prefer Python 3.12+ (system python3 on macOS is often 3.9 which is too old)
PYTHON=""
for candidate in python3.13 python3.12 python3.11 python3; do
    if command -v "$candidate" &>/dev/null; then
        ver=$("$candidate" -c "import sys; print(sys.version_info[:2])" 2>/dev/null)
        major=$("$candidate" -c "import sys; print(sys.version_info[0])" 2>/dev/null)
        minor=$("$candidate" -c "import sys; print(sys.version_info[1])" 2>/dev/null)
        if [ "$major" -ge 3 ] && [ "$minor" -ge 10 ]; then
            PYTHON="$candidate"
            break
        fi
    fi
done

if [ -z "$PYTHON" ]; then
    echo "FEHLER: Python >= 3.10 wird benoetigt."
    echo "Auf dem Mac: brew install python@3.12"
    exit 1
fi

echo "  Verwende: $($PYTHON --version 2>&1)"

if [ -d "$VENV_DIR" ]; then
    # Check that existing venv uses a good Python
    VENV_VER=$("$VENV_DIR/bin/python" -c "import sys; print(sys.version_info[1])" 2>/dev/null || echo "0")
    if [ "$VENV_VER" -lt 10 ]; then
        echo "  (Alte Umgebung wird neu erstellt)"
        rm -rf "$VENV_DIR"
        "$PYTHON" -m venv "$VENV_DIR"
    else
        echo "  (Vorhandene Umgebung wird wiederverwendet)"
    fi
else
    "$PYTHON" -m venv "$VENV_DIR"
fi
source "$VENV_DIR/bin/activate"

# --- Schritt 2: Pakete installieren ---
echo "[2/4] Pakete werden installiert (kann 1-2 Minuten dauern)..."
pip install --quiet --upgrade pip
pip install --quiet "arvak[all]" jupyter nbconvert 2>&1 | tail -3

# --- Schritt 3: Notebooks ausfuehren ---
echo "[3/4] Notebooks werden getestet..."
echo ""

cd "$NOTEBOOK_DIR"

PASS=0
FAIL=0
TOTAL=0
RESULTS=""
TIMESTAMP=$(date +"%Y-%m-%d %H:%M")

for nb in *.ipynb; do
    TOTAL=$((TOTAL + 1))
    printf "  %-45s " "$nb"

    OUTPUT_FILE=$(mktemp)
    if jupyter nbconvert --to notebook --execute \
        --ExecutePreprocessor.timeout=120 \
        --ExecutePreprocessor.kernel_name=python3 \
        "$nb" --output /tmp/nb-test-out.ipynb \
        > "$OUTPUT_FILE" 2>&1; then
        echo "PASS"
        PASS=$((PASS + 1))
        RESULTS="${RESULTS}PASS  $nb\n"
    else
        echo "FAIL"
        FAIL=$((FAIL + 1))
        ERROR=$(tail -30 "$OUTPUT_FILE" | grep -E '(Error|Exception|Traceback|ModuleNotFoundError)' | head -5)
        if [ -z "$ERROR" ]; then
            ERROR=$(tail -5 "$OUTPUT_FILE")
        fi
        RESULTS="${RESULTS}FAIL  $nb\n"
        RESULTS="${RESULTS}      Fehler: $(echo "$ERROR" | head -3)\n\n"
    fi
    rm -f "$OUTPUT_FILE"
done

# --- Schritt 4: Bericht ---
echo ""
echo "============================================"
echo "  Ergebnis: $PASS/$TOTAL bestanden, $FAIL fehlgeschlagen"
echo "============================================"

{
    echo "Arvak Notebook-Testbericht"
    echo "========================="
    echo "Datum: $TIMESTAMP"
    echo "Python: $(python3 --version 2>&1)"
    echo "arvak:  $(pip show arvak 2>/dev/null | grep Version || echo 'nicht installiert')"
    echo "System: $(uname -s) $(uname -m)"
    echo ""
    echo "Ergebnis: $PASS/$TOTAL bestanden, $FAIL fehlgeschlagen"
    echo ""
    echo "Details:"
    echo "--------"
    echo -e "$RESULTS"
} > "$REPORT_FILE"

cat "$REPORT_FILE"

echo ""
echo "Bericht gespeichert in: $REPORT_FILE"

if [ $FAIL -gt 0 ]; then
    echo ""
    echo "--- NAECHSTER SCHRITT ---"
    echo "Bitte schicke die Datei '$REPORT_FILE' an Daniel"
    echo "oder kopiere den Text oben in eine Nachricht."
    exit 1
else
    echo ""
    echo "Alle Notebooks funktionieren!"
fi
