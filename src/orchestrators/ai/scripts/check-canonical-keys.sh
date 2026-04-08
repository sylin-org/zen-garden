#!/usr/bin/env bash
#
# CI guard: reject canonical-key string literals outside `src/domain/keys/`.
#
# Rule (ADR §ACCEPTANCE-5): every canonical field path referenced at
# runtime must be declared as a `FieldPath` constant in
# `src/domain/keys/`. String literals matching the canonical-key
# pattern are rejected elsewhere — they bypass the constant-based
# vocabulary and break refactor safety.
#
# Exempt contexts:
# - `src/domain/keys/*` (authoritative definitions).
# - `src/domain/primitive.rs` (authoritative `Primitive::dotted` mapping).
# - Doc comments (`//!`, `///`).
# - Test modules (lines inside `#[cfg(test)]` blocks or `mod tests`).
# - Log macros (`tracing::*!`, `info!`, etc.).
# - `format!`, `panic!`, `write!`, `writeln!`, `anyhow!`, `bail!`.
# - `OrchestratorError::new(...)` error messages.
# - Doc examples in `.md` files under `docs/`.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="${ROOT}/src"

PATTERNS='"(text|image|audio|usage|timing|meta|job|stream)[.][a-z][a-z0-9_.]*"'

# Files to scan.
mapfile -t FILES < <(find "${SRC}" -type f -name '*.rs' \
    -not -path "${SRC}/domain/keys/*" \
    -not -path "${SRC}/domain/primitive.rs")

VIOLATIONS=0
SCANNED=0

for file in "${FILES[@]}"; do
    SCANNED=$((SCANNED + 1))
    # Scan for pattern matches with awk so we can track test-module state.
    while IFS= read -r hit; do
        line_no="${hit%%:*}"
        content="${hit#*:}"
        # Strip leading whitespace for prefix checks.
        stripped="${content#"${content%%[![:space:]]*}"}"
        case "${stripped}" in
            '//!'*|'///'*|'//'*) continue ;;
        esac
        case "${content}" in
            *tracing::*!*|*info!*|*warn!*|*error!*|*debug!*|*trace!*) continue ;;
            *format\!*|*panic\!*|*write\!*|*writeln\!*|*anyhow\!*|*bail\!*) continue ;;
            *OrchestratorError::new*) continue ;;
        esac
        echo "VIOLATION: ${file}:${line_no}:${content}"
        VIOLATIONS=$((VIOLATIONS + 1))
    done < <(awk -v pat="${PATTERNS}" '
        BEGIN { in_test = 0 }
        /#\[cfg\(test\)\]/ { in_test = 1; next }
        /^mod tests/ { in_test = 1; next }
        { if (!in_test && $0 ~ pat) print NR ":" $0 }
    ' "${file}")
done

if [[ ${VIOLATIONS} -gt 0 ]]; then
    echo ""
    echo "Found ${VIOLATIONS} canonical-key magic-string violation(s)."
    echo "Every canonical field path must be a FieldPath constant in src/domain/keys/."
    exit 1
fi

echo "Canonical-key guard: clean (scanned ${SCANNED} files)."
exit 0
