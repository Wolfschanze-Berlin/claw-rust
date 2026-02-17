#!/usr/bin/env bash
# Stop hook: Check if structural changes were made without updating README.md
# Returns JSON: {"ok":true} or {"ok":false,"reason":"..."}

set -euo pipefail

# Get staged + unstaged changes (what happened this session)
CHANGED_FILES=$(git diff --name-only HEAD 2>/dev/null || true)
STAGED_FILES=$(git diff --cached --name-only 2>/dev/null || true)
UNTRACKED=$(git ls-files --others --exclude-standard 2>/dev/null || true)

ALL_CHANGES=$(printf '%s\n%s\n%s' "$CHANGED_FILES" "$STAGED_FILES" "$UNTRACKED" | sort -u | grep -v '^$' || true)

if [ -z "$ALL_CHANGES" ]; then
  echo '{"ok":true}'
  exit 0
fi

# Check for structural changes
STRUCTURAL=false

# 1. New crates/modules added or removed
if echo "$ALL_CHANGES" | grep -qE '^crates/[^/]+/(Cargo\.toml|src/lib\.rs)$'; then
  STRUCTURAL=true
fi

# 2. New top-level directories created
if echo "$ALL_CHANGES" | grep -qE '^[^/]+/$'; then
  STRUCTURAL=true
fi

# 3. Root Cargo.toml workspace changes
if echo "$ALL_CHANGES" | grep -qE '^Cargo\.toml$'; then
  if git diff HEAD -- Cargo.toml 2>/dev/null | grep -qE '^\+.*members|^\+.*\[workspace'; then
    STRUCTURAL=true
  fi
fi

# 4. Dependencies added to any Cargo.toml
if echo "$ALL_CHANGES" | grep -qE 'Cargo\.toml$'; then
  if git diff HEAD -- '*.toml' 2>/dev/null | grep -qE '^\+.*\[dependencies'; then
    STRUCTURAL=true
  fi
fi

# 5. Config schema changes
if echo "$ALL_CHANGES" | grep -qE '(config\.rs|config\.json|\.env\.example)'; then
  STRUCTURAL=true
fi

if [ "$STRUCTURAL" = false ]; then
  echo '{"ok":true}'
  exit 0
fi

# Structural changes detected — check if README was also modified
if echo "$ALL_CHANGES" | grep -qiE '^README\.md$'; then
  echo '{"ok":true}'
  exit 0
fi

echo '{"ok":false,"reason":"Structural changes detected (new crates, Cargo.toml changes, or config updates) but README.md was not updated."}'
