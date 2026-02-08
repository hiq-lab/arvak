#!/usr/bin/env bash
# =============================================================================
# Arvak Documentation & Code Integrity Audit
#
# Checks for stale naming, count mismatches, broken paths, CLI syntax errors,
# Python API inconsistencies, backend status accuracy, and version drift.
#
# Usage: bash scripts/audit.sh
# Exit code: 0 if all checks pass, 1 if any check fails.
# =============================================================================

set -uo pipefail
# Note: -e intentionally omitted — we handle errors per-check.

# --- colours (disabled when not a terminal) ----------------------------------
if [[ -t 1 ]]; then
    GREEN='\033[0;32m'; RED='\033[0;31m'; YELLOW='\033[0;33m'
    BOLD='\033[1m'; RESET='\033[0m'
else
    GREEN=''; RED=''; YELLOW=''; BOLD=''; RESET=''
fi

PASS_COUNT=0
FAIL_COUNT=0
WARN_COUNT=0
DETAILS=""

pass() { PASS_COUNT=$((PASS_COUNT + 1)); printf "${GREEN}[PASS]${RESET} %s\n" "$1"; }
fail() { FAIL_COUNT=$((FAIL_COUNT + 1)); printf "${RED}[FAIL]${RESET} %s\n" "$1"; DETAILS+="  - $1"$'\n'; }
warn() { WARN_COUNT=$((WARN_COUNT + 1)); printf "${YELLOW}[WARN]${RESET} %s\n" "$1"; }

# Project root (script lives in scripts/)
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo ""
echo "=== Arvak Documentation & Code Integrity Audit ==="
echo ""

# =============================================================================
# 1. Stale Naming Check
# =============================================================================
echo "--- 1. Stale Naming Check ---"

# Patterns use extended regex compatible with both GNU and BSD grep.
# We avoid \b (not portable); instead use patterns with enough context.
STALE_PATTERNS=(
    'hiq_to_'
    'to_hiq[^-]'
    'from_hiq[^-]'
    'hiq_circuit'
    'hiq\.from_qasm'
    'hiq\.Circuit'
    '"hiq-quantum"'
    '"HIQ Lab"'
    '"HIQ Team"'
    'hiq-lab\.org'
)

STALE_FOUND=0
for pattern in "${STALE_PATTERNS[@]}"; do
    hits=$(grep -rn --include='*.py' --include='*.md' --include='*.rs' --include='*.toml' \
        -E "$pattern" . \
        --exclude-dir=.git --exclude-dir=target \
        --exclude='CHANGELOG.md' 2>/dev/null \
        | grep -v 'hiq-lab/arvak' \
        | grep -v 'hiq-lab/HIQ' \
        | grep -v 'scripts/audit.sh' \
        || true)
    if [[ -n "$hits" ]]; then
        STALE_FOUND=1
        while IFS= read -r line; do
            [[ -z "$line" ]] && continue
            fail "Stale naming: $line"
        done <<< "$hits"
    fi
done

if [[ "$STALE_FOUND" -eq 0 ]]; then
    pass "Stale naming: no forbidden patterns found"
fi

# =============================================================================
# 2. Count Verification
# =============================================================================
echo ""
echo "--- 2. Count Verification ---"

PROTO_FILE="crates/arvak-grpc/proto/arvak.proto"

if [[ -f "$PROTO_FILE" ]]; then
    # 2a. RPC count
    PROTO_RPC_COUNT=$(grep -cE '^\s*rpc\s+' "$PROTO_FILE" 2>/dev/null || echo 0)

    # Check README claims about RPC count
    README_RPC_CLAIM=$(grep -oE '\*\*[0-9]+ gRPC RPCs\*\*' README.md 2>/dev/null | grep -oE '[0-9]+' || echo "")
    if [[ -n "$README_RPC_CLAIM" ]]; then
        if [[ "$README_RPC_CLAIM" -eq "$PROTO_RPC_COUNT" ]]; then
            pass "RPC count: README claims $README_RPC_CLAIM, proto has $PROTO_RPC_COUNT"
        else
            fail "RPC count: README claims $README_RPC_CLAIM gRPC RPCs but proto has $PROTO_RPC_COUNT"
        fi
    else
        warn "RPC count: no RPC count claim found in README.md"
    fi

    # Check gRPC crate README claims  (e.g. "7 unary + 3 streaming RPCs")
    GRPC_README="crates/arvak-grpc/README.md"
    if [[ -f "$GRPC_README" ]]; then
        GRPC_RPC_CLAIM=$(grep -oE '[0-9]+ unary \+ [0-9]+ streaming' "$GRPC_README" 2>/dev/null | head -1 || echo "")
        if [[ -n "$GRPC_RPC_CLAIM" ]]; then
            UNARY=$(echo "$GRPC_RPC_CLAIM" | sed -E 's/^([0-9]+) unary.*/\1/')
            STREAMING=$(echo "$GRPC_RPC_CLAIM" | sed -E 's/.*\+ ([0-9]+) streaming/\1/')
            CLAIMED_TOTAL=$((UNARY + STREAMING))
            if [[ "$CLAIMED_TOTAL" -eq "$PROTO_RPC_COUNT" ]]; then
                pass "RPC count (gRPC README): claims ${UNARY}+${STREAMING}=$CLAIMED_TOTAL, proto has $PROTO_RPC_COUNT"
            else
                fail "RPC count (gRPC README): claims ${UNARY}+${STREAMING}=$CLAIMED_TOTAL but proto has $PROTO_RPC_COUNT"
            fi
        fi
    fi

    # 2b. Proto message count
    PROTO_MSG_COUNT=$(grep -cE '^\s*message\s+[A-Z]' "$PROTO_FILE" 2>/dev/null || echo 0)

    CHANGELOG_MSG_CLAIM=$(grep -oE 'with [0-9]+ messages' CHANGELOG.md 2>/dev/null | head -1 | grep -oE '[0-9]+' || echo "")
    if [[ -n "$CHANGELOG_MSG_CLAIM" ]]; then
        if [[ "$CHANGELOG_MSG_CLAIM" -eq "$PROTO_MSG_COUNT" ]]; then
            pass "Proto message count: CHANGELOG claims $CHANGELOG_MSG_CLAIM, proto has $PROTO_MSG_COUNT"
        else
            fail "Proto message count: CHANGELOG claims $CHANGELOG_MSG_CLAIM but proto has $PROTO_MSG_COUNT"
        fi
    fi
else
    warn "Proto file not found at $PROTO_FILE"
fi

# 2c. CLI subcommand count
CLI_MAIN="crates/arvak-cli/src/main.rs"
if [[ -f "$CLI_MAIN" ]]; then
    CLI_CMD_COUNT=$(sed -n '/^enum Commands/,/^}/p' "$CLI_MAIN" \
        | grep -cE '^\s+[A-Z][a-z]' 2>/dev/null || echo 0)
    if [[ "$CLI_CMD_COUNT" -ge 1 ]]; then
        pass "CLI commands: found $CLI_CMD_COUNT subcommands in Commands enum"
    else
        warn "CLI commands: could not parse Commands enum"
    fi
fi

# 2d. Compilation pass count
PASS_DIR="crates/arvak-compile/src"
if [[ -d "$PASS_DIR" ]]; then
    PASS_IMPL_COUNT=$(grep -rE 'impl\s+Pass\s+for\s+' "$PASS_DIR" 2>/dev/null \
        | grep -v 'TestPass' \
        | grep -cv '^\s*//' 2>/dev/null || echo 0)
    if [[ "$PASS_IMPL_COUNT" -ge 1 ]]; then
        pass "Compilation passes: found $PASS_IMPL_COUNT Pass implementations (excluding TestPass)"
    else
        warn "Compilation passes: could not find Pass implementations"
    fi
fi

# =============================================================================
# 3. Path & File Existence
# =============================================================================
echo ""
echo "--- 3. Path & File Existence ---"

PATH_CHECK_FAIL=0
LINKS_CHECKED=0

check_markdown_links() {
    local file="$1"
    local base_dir
    base_dir="$(dirname "$file")"

    grep -oE '\[[^]]*\]\([^)]+\)' "$file" 2>/dev/null | while IFS= read -r link; do
        target=$(echo "$link" | sed -E 's/\[[^]]*\]\(([^)]+)\)/\1/' | sed 's/#.*//' | sed 's/ ".*//')
        # Skip URLs, anchors-only, empty
        case "$target" in
            https://*|http://*|mailto:*) continue ;;
            "") continue ;;
        esac
        # Resolve relative path
        if [[ ! -e "$base_dir/$target" ]]; then
            echo "$file -> $target"
        fi
    done || true
}

BROKEN_LINKS=""

if [[ -f README.md ]]; then
    result=$(check_markdown_links "README.md")
    [[ -n "$result" ]] && BROKEN_LINKS+="$result"$'\n'
fi

for doc in docs/*.md; do
    [[ -f "$doc" ]] || continue
    result=$(check_markdown_links "$doc")
    [[ -n "$result" ]] && BROKEN_LINKS+="$result"$'\n'
done

# Trim trailing newlines
BROKEN_LINKS=$(echo "$BROKEN_LINKS" | sed '/^$/d')

if [[ -n "$BROKEN_LINKS" ]]; then
    while IFS= read -r broken; do
        [[ -z "$broken" ]] && continue
        fail "Broken link: $broken"
        PATH_CHECK_FAIL=$((PATH_CHECK_FAIL + 1))
        LINKS_CHECKED=$((LINKS_CHECKED + 1))
    done <<< "$BROKEN_LINKS"
else
    pass "Path existence: all checked markdown links resolve"
fi

# Check notebook directory
NOTEBOOK_DIR="crates/arvak-python/notebooks"
if [[ -d "$NOTEBOOK_DIR" ]]; then
    NOTEBOOK_COUNT=$(find "$NOTEBOOK_DIR" -name '*.ipynb' | wc -l | tr -d ' ')
    if [[ "$NOTEBOOK_COUNT" -ge 1 ]]; then
        pass "Notebooks: found $NOTEBOOK_COUNT notebooks in $NOTEBOOK_DIR"
    else
        warn "Notebooks: no .ipynb files found in $NOTEBOOK_DIR"
    fi
fi

# =============================================================================
# 4. CLI Syntax Check
# =============================================================================
echo ""
echo "--- 4. CLI Syntax Check ---"

CLI_SYNTAX_FAIL=0

# 4a. Check for positional args after "arvak compile" or "arvak run" (should use --input/-i)
for doc in README.md docs/*.md; do
    [[ -f "$doc" ]] || continue
    hits=$(grep -nE 'arvak\s+compile\s+[^-]' "$doc" 2>/dev/null \
        | grep -v '\-\-' \
        | grep -v '\-i ' \
        | grep -v '^\s*#' \
        | grep -v 'cargo' \
        || true)
    if [[ -n "$hits" ]]; then
        while IFS= read -r line; do
            [[ -z "$line" ]] && continue
            fail "CLI syntax (positional arg): $doc:$line"
            CLI_SYNTAX_FAIL=$((CLI_SYNTAX_FAIL + 1))
        done <<< "$hits"
    fi
done

# 4b. Check for non-existent subcommands in code blocks
NON_EXISTENT_CMDS=("arvak auth" "arvak login" "arvak config" "arvak init" "arvak deploy")
for cmd in "${NON_EXISTENT_CMDS[@]}"; do
    for doc in README.md docs/*.md; do
        [[ -f "$doc" ]] || continue
        hits=$(grep -nF "$cmd" "$doc" 2>/dev/null \
            | grep -v '^\s*#' \
            | grep -v 'Planned' \
            | grep -v 'TODO' \
            || true)
        if [[ -n "$hits" ]]; then
            while IFS= read -r line; do
                [[ -z "$line" ]] && continue
                fail "Non-existent CLI command '$cmd': $doc:$line"
                CLI_SYNTAX_FAIL=$((CLI_SYNTAX_FAIL + 1))
            done <<< "$hits"
        fi
    done
done

# 4c. Check for phantom subcommands (submit, status, result) that aren't in the Commands enum
PHANTOM_CMDS=("arvak submit " "arvak status " "arvak result ")
for cmd in "${PHANTOM_CMDS[@]}"; do
    for doc in README.md docs/*.md; do
        [[ -f "$doc" ]] || continue
        hits=$(grep -nF "$cmd" "$doc" 2>/dev/null || true)
        if [[ -n "$hits" ]]; then
            while IFS= read -r line; do
                [[ -z "$line" ]] && continue
                fail "Phantom CLI command '${cmd% }': $doc:$line"
                CLI_SYNTAX_FAIL=$((CLI_SYNTAX_FAIL + 1))
            done <<< "$hits"
        fi
    done
done

if [[ "$CLI_SYNTAX_FAIL" -eq 0 ]]; then
    pass "CLI syntax: no positional-arg misuse or phantom commands found in docs"
fi

# =============================================================================
# 5. Python API Consistency
# =============================================================================
echo ""
echo "--- 5. Python API Consistency ---"

PY_API_FAIL=0
PY_ROOT="crates/arvak-python/python/arvak"

if [[ -d "$PY_ROOT" ]]; then
    # 5a. Check each integration converter exports match __init__.py imports
    for framework_dir in "$PY_ROOT"/integrations/*/; do
        [[ -d "$framework_dir" ]] || continue
        framework=$(basename "$framework_dir")
        [[ "$framework" == "__pycache__" ]] && continue
        [[ "$framework" == "_base" ]] && continue

        init_file="$framework_dir/__init__.py"
        converter_file="$framework_dir/converter.py"

        [[ -f "$init_file" ]] || continue
        [[ -f "$converter_file" ]] || continue

        # Get public function names defined in converter.py (skip _private functions)
        converter_fns=$(grep -E '^def [a-z]' "$converter_file" 2>/dev/null | grep -v '^def _' | sed 's/^def //' | sed 's/(.*//' || true)

        # Get function names imported from .converter in __init__.py
        init_imports=$(grep -E 'from \.converter import ' "$init_file" 2>/dev/null \
            | sed 's/.*from \.converter import //' \
            | tr ',' '\n' \
            | sed 's/^[[:space:]]*//' \
            | sed 's/[[:space:]]*$//' || true)

        for fn in $converter_fns; do
            [[ -z "$fn" ]] && continue
            if ! echo "$init_imports" | grep -qw "$fn" 2>/dev/null; then
                fail "Python API: $framework/converter.py defines '$fn' but __init__.py doesn't import it"
                PY_API_FAIL=$((PY_API_FAIL + 1))
            fi
        done
    done

    # 5b. Check for "import arvak" then "hiq." usage (undefined name bug)
    hiq_hits=""
    while IFS= read -r pyfile; do
        [[ -z "$pyfile" ]] && continue
        pyfile_clean=$(echo "$pyfile" | sed 's/:.*//')
        matches=$(grep -nE 'hiq\.' "$pyfile_clean" 2>/dev/null || true)
        if [[ -n "$matches" ]]; then
            hiq_hits+="$pyfile_clean: $matches"$'\n'
        fi
    done < <(grep -rlE 'import arvak' "$PY_ROOT" 2>/dev/null || true)

    if [[ -n "$hiq_hits" ]]; then
        while IFS= read -r line; do
            [[ -z "$line" ]] && continue
            fail "Python API: uses 'hiq.' after importing arvak: $line"
            PY_API_FAIL=$((PY_API_FAIL + 1))
        done <<< "$hiq_hits"
    fi

    if [[ "$PY_API_FAIL" -eq 0 ]]; then
        pass "Python API: converter exports match __init__.py imports, no hiq. references"
    fi
else
    warn "Python API: directory $PY_ROOT not found"
fi

# =============================================================================
# 6. Feature Status / Backend Table Audit
# =============================================================================
echo ""
echo "--- 6. Backend Status Table Audit ---"

if [[ -f README.md ]]; then
    CLI_BACKENDS_FILE="crates/arvak-cli/src/commands/backends.rs"

    if [[ -f "$CLI_BACKENDS_FILE" ]]; then
        # Backends accessible via CLI
        CLI_BACKENDS=$(grep -oE 'style\("[a-z]+"\)' "$CLI_BACKENDS_FILE" 2>/dev/null \
            | sed 's/style("//' | sed 's/")//' | sort -u || true)

        # Extract Backend Support table rows (between "## Backend Support" and next "##")
        # Then filter for ✅ rows that are NOT Library-only
        README_PROD_BACKENDS=$(sed -n '/^## Backend Support/,/^## /p' README.md 2>/dev/null \
            | grep '✅' \
            | grep -v 'Library-only' \
            | grep -v '⚠️' \
            | grep '|' \
            || true)

        if [[ -n "$README_PROD_BACKENDS" ]]; then
            while IFS= read -r row; do
                [[ -z "$row" ]] && continue
                backend_name=$(echo "$row" | awk -F'|' '{print $2}' | xargs | tr '[:upper:]' '[:lower:]')
                backend_key=$(echo "$backend_name" | awk '{print $1}')

                if echo "$CLI_BACKENDS" | grep -qi "$backend_key" 2>/dev/null; then
                    : # Found in CLI
                elif [[ "$backend_key" == "iqm" ]]; then
                    : # iqm covers LUMI/LRZ variants
                else
                    warn "Backend '$backend_name' (marked ✅) not directly referenced in CLI backends.rs"
                fi
            done <<< "$README_PROD_BACKENDS"
        fi

        pass "Backend table: production backends cross-referenced with CLI"
    else
        warn "Backend table: CLI backends file not found"
    fi
fi

# =============================================================================
# 7. Version Consistency
# =============================================================================
echo ""
echo "--- 7. Version Consistency ---"

# Source of truth: workspace Cargo.toml
CARGO_VERSION=""
if [[ -f Cargo.toml ]]; then
    CARGO_VERSION=$(grep -E '^version\s*=' Cargo.toml 2>/dev/null | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
fi

if [[ -z "$CARGO_VERSION" ]]; then
    fail "Version: could not extract version from Cargo.toml"
else
    pass "Cargo.toml workspace version: $CARGO_VERSION"

    # 7a. README badge version
    if [[ -f README.md ]]; then
        README_BADGE_VER=$(grep -oE 'version-[0-9]+\.[0-9]+\.[0-9]+' README.md 2>/dev/null | head -1 | sed 's/version-//')
        if [[ -n "$README_BADGE_VER" ]]; then
            if [[ "$README_BADGE_VER" == "$CARGO_VERSION" ]]; then
                pass "README badge version: $README_BADGE_VER (matches)"
            else
                fail "README badge version: $README_BADGE_VER != Cargo.toml $CARGO_VERSION"
            fi
        else
            warn "README badge: no version badge found"
        fi
    fi

    # 7b. pyproject.toml version
    PYPROJECT="crates/arvak-python/pyproject.toml"
    if [[ -f "$PYPROJECT" ]]; then
        PY_VERSION=$(grep -E '^version\s*=' "$PYPROJECT" 2>/dev/null | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
        if [[ -n "$PY_VERSION" ]]; then
            if [[ "$PY_VERSION" == "$CARGO_VERSION" ]]; then
                pass "pyproject.toml version: $PY_VERSION (matches)"
            else
                fail "pyproject.toml version: $PY_VERSION != Cargo.toml $CARGO_VERSION"
            fi
        fi
    fi

    # 7c. CHANGELOG latest heading
    if [[ -f CHANGELOG.md ]]; then
        CHANGELOG_VER=$(grep -E '^## \[[0-9]+\.[0-9]+\.[0-9]+\]' CHANGELOG.md 2>/dev/null \
            | head -1 | sed -E 's/.*\[([0-9.]+)\].*/\1/')
        if [[ -n "$CHANGELOG_VER" ]]; then
            if [[ "$CHANGELOG_VER" == "$CARGO_VERSION" ]]; then
                pass "CHANGELOG latest version: $CHANGELOG_VER (matches)"
            else
                warn "CHANGELOG latest version: $CHANGELOG_VER (Cargo.toml is $CARGO_VERSION)"
            fi
        fi
    fi
fi

# =============================================================================
# Summary
# =============================================================================
echo ""
echo "==========================================="
TOTAL=$((PASS_COUNT + FAIL_COUNT))
printf "Summary: ${GREEN}%d passed${RESET}, ${RED}%d failed${RESET}, ${YELLOW}%d warnings${RESET} (%d total checks)\n" \
    "$PASS_COUNT" "$FAIL_COUNT" "$WARN_COUNT" "$TOTAL"

if [[ "$FAIL_COUNT" -gt 0 ]]; then
    echo ""
    echo "Failures:"
    printf "%s" "$DETAILS"
    exit 1
fi

echo ""
echo "All checks passed."
exit 0
