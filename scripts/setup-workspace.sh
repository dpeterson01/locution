#!/usr/bin/env bash
# setup-workspace.sh
# Seeds the per-machine memory pack at .local/memory/ from templates in
# templates/memory/. Idempotent: copies a template only when the matching
# destination file is missing. Never overwrites an existing file.
#
# Usage:
#   ./scripts/setup-workspace.sh           # create any missing pack files
#   ./scripts/setup-workspace.sh --check   # report status only, copy nothing
#
# Exit codes:
#   0  pack is complete (all four files present, or all created this run)
#   1  --check with one or more files missing

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || cd "$(dirname "$0")/.." && pwd)"
TEMPLATE_DIR="$REPO_ROOT/templates/memory"
DEST_DIR="$REPO_ROOT/.local/memory"

PACK_FILES=(
  "recent-context.md"
  "preferences.md"
  "project-context.md"
  "roster.md"
)

CHECK_ONLY=false
if [[ "${1:-}" == "--check" ]]; then
  CHECK_ONLY=true
fi

if [[ ! -d "$TEMPLATE_DIR" ]]; then
  echo "error: template dir not found at $TEMPLATE_DIR" >&2
  exit 2
fi

created=0
present=0
missing=0

if [[ "$CHECK_ONLY" == false ]]; then
  mkdir -p "$DEST_DIR"
fi

for name in "${PACK_FILES[@]}"; do
  dest="$DEST_DIR/$name"
  src="$TEMPLATE_DIR/$name"

  if [[ -f "$dest" ]]; then
    present=$(( present + 1 ))
    echo "ok:      .local/memory/$name"
    continue
  fi

  if [[ "$CHECK_ONLY" == true ]]; then
    missing=$(( missing + 1 ))
    echo "missing: .local/memory/$name"
    continue
  fi

  if [[ ! -f "$src" ]]; then
    echo "warn: no template for $name (expected $src), skipping" >&2
    missing=$(( missing + 1 ))
    continue
  fi

  cp "$src" "$dest"
  created=$(( created + 1 ))
  echo "created: .local/memory/$name"
done

total=${#PACK_FILES[@]}

if [[ "$CHECK_ONLY" == true ]]; then
  echo ""
  echo "memory pack: $present/$total present, $missing missing"
  [[ "$missing" -eq 0 ]] && exit 0 || exit 1
fi

echo ""
echo "memory pack: $present already present, $created created"
[[ "$missing" -gt 0 ]] && echo "  ($missing skipped — template missing)"
echo ""
echo "Edit .local/memory/ files to pre-fill your context."
echo "They are gitignored and never leave this machine."
