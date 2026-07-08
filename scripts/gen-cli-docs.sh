#!/usr/bin/env bash
# Regenerate docs/cli.md from the CLI's own --help output.
# Run after changing arvak-cli commands or flags:
#   ./scripts/gen-cli-docs.sh
set -euo pipefail

cd "$(dirname "$0")/.."
cargo build -p arvak-cli --quiet
BIN=target/debug/arvak
OUT=docs/cli.md

{
  echo "# CLI Reference"
  echo
  echo "> Generated from \`arvak --help\` by [\`scripts/gen-cli-docs.sh\`](../scripts/gen-cli-docs.sh)."
  echo "> Do not edit by hand — rerun the script after CLI changes."
  echo
  echo '```text'
  "$BIN" --help
  echo '```'
  for cmd in compile run submit status result auth wait eval backends; do
    echo
    echo "## arvak $cmd"
    echo
    echo '```text'
    "$BIN" "$cmd" --help
    echo '```'
  done
} > "$OUT"

echo "wrote $OUT"
