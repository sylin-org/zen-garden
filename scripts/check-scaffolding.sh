#!/usr/bin/env bash
#
# check-scaffolding.sh — Validates docs/scaffolding.md invariants.
#
# Part of ARCH-0017 (DDD Monolith Epic). Enforces the scaffolding contract:
# every scaffold marked `status: removed` must no longer match its `check`
# patterns in the repository; every scaffold marked `status: active` is
# logged informationally.
#
# Usage:
#
#   ./scripts/check-scaffolding.sh [--verbose]
#
# Exit codes:
#
#   0  — all `removed` scaffolds pass their checks
#   1  — one or more `removed` scaffolds still match their check patterns
#   2  — scaffolding.md is malformed or unreadable
#
# Optional pre-commit hook installation:
#
#   ln -s ../../scripts/check-scaffolding.sh .git/hooks/pre-commit
#
# Book XX (Epilogue) wires this script into CI alongside layering lints.
# Until then, enforcement is advisory — run locally or on demand.
#
# Dependencies: bash 4+, grep (with -E -r support), awk. POSIX-compatible;
# works on Linux, macOS, and Git Bash on Windows. No YAML parser required —
# the script extracts metadata from the fenced yaml blocks with a small
# state machine.

set -euo pipefail

# ── Resolve repo root ──────────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TRACKER="${REPO_ROOT}/docs/scaffolding.md"

# ── Options ────────────────────────────────────────────────────────────
VERBOSE=0
for arg in "$@"; do
    case "$arg" in
        -v|--verbose) VERBOSE=1 ;;
        -h|--help)
            sed -n '3,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            echo "Usage: $0 [--verbose]" >&2
            exit 2
            ;;
    esac
done

# ── Sanity checks ──────────────────────────────────────────────────────
if [[ ! -f "$TRACKER" ]]; then
    echo "ERROR: scaffolding tracker not found at $TRACKER" >&2
    exit 2
fi

# ── Colors (if terminal) ───────────────────────────────────────────────
if [[ -t 1 ]]; then
    C_RED=$'\033[0;31m'
    C_GREEN=$'\033[0;32m'
    C_YELLOW=$'\033[0;33m'
    C_BLUE=$'\033[0;34m'
    C_BOLD=$'\033[1m'
    C_RESET=$'\033[0m'
else
    C_RED=""
    C_GREEN=""
    C_YELLOW=""
    C_BLUE=""
    C_BOLD=""
    C_RESET=""
fi

log_info()    { echo "${C_BLUE}[info]${C_RESET} $*"; }
log_ok()      { echo "${C_GREEN}[ ok ]${C_RESET} $*"; }
log_warn()    { echo "${C_YELLOW}[warn]${C_RESET} $*"; }
log_err()     { echo "${C_RED}[fail]${C_RESET} $*" >&2; }
log_verbose() { if [[ $VERBOSE -eq 1 ]]; then echo "       $*"; fi; }

# ── Search helper ──────────────────────────────────────────────────────
# Runs `grep -E -r` against a path with a pattern. Returns 0 (match found)
# or 1 (no match) or 2 (error). Honors .rs file filtering when the path is
# a directory and likely contains source code, but grep -r walks everything
# by default.
search_pattern() {
    local pattern="$1"
    local path="$2"
    local abs_path="${REPO_ROOT}/${path}"

    if [[ ! -e "$abs_path" ]]; then
        return 2
    fi

    if [[ -d "$abs_path" ]]; then
        # Directory: recursive grep, limit to Rust sources to reduce noise.
        # We exclude target/ and other build dirs defensively.
        grep -E -r \
            --include='*.rs' \
            --exclude-dir='target' \
            --exclude-dir='.git' \
            --quiet \
            -- "$pattern" "$abs_path" 2>/dev/null
    else
        grep -E --quiet -- "$pattern" "$abs_path" 2>/dev/null
    fi
}

# Returns count of matching lines (at most 10 for the summary output).
count_matches() {
    local pattern="$1"
    local path="$2"
    local abs_path="${REPO_ROOT}/${path}"

    if [[ -d "$abs_path" ]]; then
        grep -E -r \
            --include='*.rs' \
            --exclude-dir='target' \
            --exclude-dir='.git' \
            -c \
            -- "$pattern" "$abs_path" 2>/dev/null \
            | awk -F: '$2 > 0 { sum += $2 } END { print sum + 0 }'
    else
        grep -E -c -- "$pattern" "$abs_path" 2>/dev/null || echo 0
    fi
}

# Shows first few match locations for error output.
show_sample_matches() {
    local pattern="$1"
    local path="$2"
    local abs_path="${REPO_ROOT}/${path}"

    if [[ -d "$abs_path" ]]; then
        grep -E -r -n \
            --include='*.rs' \
            --exclude-dir='target' \
            --exclude-dir='.git' \
            -- "$pattern" "$abs_path" 2>/dev/null \
            | head -5 \
            | sed "s|${REPO_ROOT}/||" \
            | sed 's/^/             /'
    else
        grep -E -n -- "$pattern" "$abs_path" 2>/dev/null \
            | head -5 \
            | sed 's/^/             /'
    fi
}

# ── Counters ───────────────────────────────────────────────────────────
active_count=0
removed_count=0
failure_count=0

# ── Parser state (reset per entry) ────────────────────────────────────
in_yaml=0
cur_id=""
cur_status=""
cur_trigger=""
cur_title=""
cur_pattern=""
declare -a pat_list=()
declare -a path_list=()
in_check=0
in_paths=0

reset_entry() {
    cur_id=""
    cur_status=""
    cur_trigger=""
    cur_title=""
    cur_pattern=""
    pat_list=()
    path_list=()
    in_check=0
    in_paths=0
}

run_checks_for_entry() {
    local id="$1"
    local status="$2"
    local trigger="$3"
    local title="$4"
    local i pat path total

    if [[ "$status" == "active" ]]; then
        active_count=$((active_count + 1))
        log_warn "scaffold ACTIVE: ${C_BOLD}${id}${C_RESET} — ${title}"
        log_verbose "       removal trigger: ${trigger}"
        return 0
    fi

    if [[ "$status" == "removed" ]]; then
        removed_count=$((removed_count + 1))
        log_verbose "checking removed scaffold: ${id}"

        if [[ ${#pat_list[@]} -eq 0 ]]; then
            log_warn "scaffold ${id} is marked removed but has no check patterns"
            return 0
        fi

        local entry_failures=0
        for i in "${!pat_list[@]}"; do
            pat="${pat_list[$i]}"
            path="${path_list[$i]}"

            if [[ ! -e "${REPO_ROOT}/${path}" ]]; then
                log_verbose "  skipped: path '${path}' does not exist"
                continue
            fi

            if search_pattern "$pat" "$path"; then
                total=$(count_matches "$pat" "$path")
                log_err "scaffold ${id}: pattern still matches"
                log_err "  pattern: ${pat}"
                log_err "  path:    ${path}"
                log_err "  match count: ${total}"
                log_err "  sample locations:"
                show_sample_matches "$pat" "$path" >&2
                entry_failures=$((entry_failures + 1))
            else
                log_verbose "  ok: pattern '${pat}' returns no matches in ${path}"
            fi
        done

        if [[ $entry_failures -gt 0 ]]; then
            failure_count=$((failure_count + 1))
        else
            log_ok "scaffold REMOVED: ${C_BOLD}${id}${C_RESET} — clean"
        fi
        return 0
    fi

    log_warn "scaffold ${id} has unknown status: ${status}"
    return 0
}

# ── Parse ──────────────────────────────────────────────────────────────
# Read the tracker, extract entries, run checks after each entry closes.

log_info "parsing ${TRACKER#$REPO_ROOT/}"

last_heading=""
# Only parse entries inside the "Active scaffolds" or "Removed scaffolds"
# sections. This skips the "Entry schema" section which contains an
# example entry that the parser would otherwise mistake for a real one.
in_entries_section=0

while IFS= read -r line || [[ -n "$line" ]]; do
    # H2 heading — section boundary
    if [[ "$line" =~ ^\#\#[[:space:]]+(Active[[:space:]]+scaffolds|Removed[[:space:]]+scaffolds)[[:space:]]*$ ]]; then
        # If we had an entry in progress, finalize it before switching sections.
        if [[ -n "$cur_id" ]]; then
            run_checks_for_entry "$cur_id" "$cur_status" "$cur_trigger" "$cur_title"
            reset_entry
        fi
        in_entries_section=1
        continue
    fi

    # H2 heading — any other section closes entry parsing
    if [[ "$line" =~ ^\#\#[[:space:]]+[A-Za-z] && "$in_entries_section" -eq 1 ]]; then
        if [[ ! "$line" =~ ^\#\#[[:space:]]+(Active[[:space:]]+scaffolds|Removed[[:space:]]+scaffolds) ]]; then
            if [[ -n "$cur_id" ]]; then
                run_checks_for_entry "$cur_id" "$cur_status" "$cur_trigger" "$cur_title"
                reset_entry
            fi
            in_entries_section=0
        fi
        continue
    fi

    # Only parse H3 entries while inside an entries section.
    if [[ $in_entries_section -eq 0 ]]; then
        continue
    fi

    # H3 heading: "### <id>: <title>"
    if [[ "$line" =~ ^\#\#\#[[:space:]]+([a-zA-Z0-9_-]+):[[:space:]]+(.+)$ ]]; then
        # If we had an entry in progress, finalize it first.
        if [[ -n "$cur_id" ]]; then
            run_checks_for_entry "$cur_id" "$cur_status" "$cur_trigger" "$cur_title"
            reset_entry
        fi
        last_heading="${BASH_REMATCH[1]}"
        cur_title="${BASH_REMATCH[2]}"
        continue
    fi

    # Fence open — start of yaml metadata block
    if [[ "$line" == '```yaml' ]]; then
        in_yaml=1
        continue
    fi

    # Fence close — end of yaml metadata block
    if [[ "$line" == '```' && $in_yaml -eq 1 ]]; then
        in_yaml=0
        in_check=0
        in_paths=0
        continue
    fi

    # Inside yaml block
    if [[ $in_yaml -eq 1 ]]; then
        if [[ "$line" =~ ^id:[[:space:]]+(.+)$ ]]; then
            cur_id="${BASH_REMATCH[1]}"
            if [[ "$cur_id" != "$last_heading" ]]; then
                log_warn "id in yaml (${cur_id}) does not match heading (${last_heading})"
            fi
            in_check=0
            in_paths=0
            continue
        fi

        if [[ "$line" =~ ^status:[[:space:]]+(.+)$ ]]; then
            cur_status="${BASH_REMATCH[1]}"
            in_check=0
            in_paths=0
            continue
        fi

        if [[ "$line" =~ ^removal_trigger:[[:space:]]+(.+)$ ]]; then
            cur_trigger="${BASH_REMATCH[1]}"
            in_check=0
            in_paths=0
            continue
        fi

        if [[ "$line" =~ ^removal_commit:[[:space:]]+(.+)$ ]]; then
            in_check=0
            in_paths=0
            continue
        fi

        if [[ "$line" =~ ^introduced_in:[[:space:]]+(.+)$ ]]; then
            in_check=0
            in_paths=0
            continue
        fi

        if [[ "$line" == "check:" ]]; then
            in_check=1
            in_paths=0
            continue
        fi

        if [[ $in_check -eq 1 ]]; then
            # '  - pattern: "<regex>"'
            if [[ "$line" =~ ^[[:space:]]+-[[:space:]]+pattern:[[:space:]]+\"(.+)\"[[:space:]]*$ ]]; then
                cur_pattern="${BASH_REMATCH[1]}"
                # Unescape yaml double-backslash into single backslash
                cur_pattern="${cur_pattern//\\\\/\\}"
                in_paths=0
                continue
            fi

            # '    paths:'
            if [[ "$line" =~ ^[[:space:]]+paths:[[:space:]]*$ ]]; then
                in_paths=1
                continue
            fi

            # '      - <path>'
            if [[ $in_paths -eq 1 && "$line" =~ ^[[:space:]]+-[[:space:]]+(.+)$ ]]; then
                local_path="${BASH_REMATCH[1]}"
                if [[ -n "$cur_pattern" ]]; then
                    pat_list+=("$cur_pattern")
                    path_list+=("$local_path")
                fi
                continue
            fi
        fi
    fi
done < "$TRACKER"

# Finalize the last entry
if [[ -n "$cur_id" ]]; then
    run_checks_for_entry "$cur_id" "$cur_status" "$cur_trigger" "$cur_title"
fi

# ── Summary ────────────────────────────────────────────────────────────
echo
echo "${C_BOLD}Summary${C_RESET}"
echo "  active scaffolds:  ${active_count}"
echo "  removed scaffolds: ${removed_count}"
echo "  failures:          ${failure_count}"

if [[ $failure_count -gt 0 ]]; then
    echo
    log_err "scaffolding check FAILED — ${failure_count} removed scaffolds still present in the tree"
    exit 1
fi

if [[ $removed_count -eq 0 && $active_count -eq 0 ]]; then
    log_warn "no scaffolds parsed — tracker may be empty or malformed"
fi

log_ok "scaffolding check PASSED"
exit 0
