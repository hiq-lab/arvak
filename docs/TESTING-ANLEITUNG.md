# Arvak Notebooks testen — Anleitung fuer Carolin

## Was du brauchst

- Einen Mac oder Linux-Rechner
- Python 3 (auf dem Mac schon vorinstalliert)
- Das Arvak-Repository (klonen oder von Daniel bekommen)
- Internetzugang (fuer die Paket-Installation)

## Schritt fuer Schritt

### 1. Terminal oeffnen

- **Mac**: Spotlight (Cmd+Leertaste) → "Terminal" eingeben → Enter
- **Linux**: Ctrl+Alt+T

### 2. Zum Projekt navigieren

Wenn Daniel dir das Repo schon auf den Rechner gelegt hat:

```bash
cd ~/Projects/Arvak-project
```

Falls du es erst klonen musst:

```bash
cd ~/Projects
git clone https://github.com/hiq-lab/arvak.git Arvak-project
cd Arvak-project
```

### 3. Test starten

Diesen Befehl kopieren und ins Terminal einfuegen:

```bash
bash scripts/test-notebooks.sh
```

Das Skript macht alles automatisch:
- Richtet eine Python-Umgebung ein
- Installiert alle noetiigen Pakete
- Fuehrt jedes Notebook aus
- Zeigt dir das Ergebnis

### 4. Ergebnis lesen

Du siehst etwas wie:

```
  01_core_arvak.ipynb                           PASS
  02_qiskit_integration.ipynb                   PASS
  03_qrisp_integration.ipynb                    FAIL
  ...

  Ergebnis: 5/7 bestanden, 2 fehlgeschlagen
```

### 5. Bericht an Daniel schicken

Der Bericht wird automatisch gespeichert in `notebooks-report.txt`.

**Option A** — Datei direkt schicken:
Die Datei `notebooks-report.txt` im Projektordner an Daniel senden.

**Option B** — Text kopieren:
Den Terminal-Output markieren (Cmd+A), kopieren (Cmd+C) und in eine Nachricht an Daniel einfuegen.

## Haeufige Probleme

### "python3: command not found"

Python ist nicht installiert. Auf dem Mac:
```bash
xcode-select --install
```

### "Permission denied"

```bash
chmod +x scripts/test-notebooks.sh
bash scripts/test-notebooks.sh
```

### Das Skript haengt laenger als 5 Minuten bei einem Notebook

Ctrl+C druecken und den bisherigen Output an Daniel schicken.

## Nochmal ausfuehren

Einfach den gleichen Befehl nochmal eingeben — die Umgebung wird wiederverwendet:

```bash
bash scripts/test-notebooks.sh
```

Nach einem Update von Daniel:

```bash
git pull
bash scripts/test-notebooks.sh
```
