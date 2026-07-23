#!/usr/bin/env bash
#
# Phase 251 — forbid `--allow-multiple-definition` in the nano-ros build system.
#
# The flag lets two different functions with the same name coexist and binds
# callers to whichever copy the linker picks first (archive order / --gc-sections
# dependent) — the #48-class wrong-copy hazard. The safe default is "duplicate
# defined symbol => link error". This gate enforces that: a real link-flag use in
# a build file fails unless the file is in the audited allowlist
# (scripts/allow-multiple-def-allowlist.txt), each entry carrying a reason + issue.
# Target: empty allowlist.
#
# Scope: the build system only (CMakeLists.txt + *.cmake + scripts/*.sh +
# just/*.just + justfile). A COMMENT that merely names the flag is not a use
# (the flag appears in many rationale comments / docs / Rust test prose).
#
# Hooked from `just check` via `just check-no-allow-multiple-def`.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

allowlist="scripts/allow-multiple-def-allowlist.txt"
pat='allow.multiple.definition'

# Build-file roots. Rust/C sources + docs reference the flag in prose only.
# #138 — also scan every in-tree CMakeLists.txt under examples/ + packages/ (the
# original scope missed them, so the flag slipped into the 6 threadx-riscv64 rust
# example CMakeLists unnoticed). Exclude build-output + generated trees (they hold
# CMake-materialised copies) and vendored third-party (never scanned).
mapfile -t files < <(
    {
        echo "CMakeLists.txt"
        find cmake scripts just -type f \( -name '*.cmake' -o -name '*.sh' -o -name '*.just' \) 2>/dev/null
        # PRUNE the build trees so find does not DESCEND into them — `-not
        # -path` alone only filters output while still traversing the millions
        # of files under build-workspace-fixtures/target (same fix as the
        # justfile/native.just finds).
        find examples packages \
            \( -name build -o -name 'build-*' -o -name target -o -name 'target-*' \
               -o -name generated -o -name third-party -o -name _deps -o -name cargo-target \) -prune -o \
            -type f \( -name 'CMakeLists.txt' -o -name '*.cmake' \) \
            -not -path '*/build/*' -not -path '*/build-*/*' -not -path '*/target/*' \
            -not -path '*/generated/*' -not -path '*/third-party/*' -not -path '*/_deps/*' -print 2>/dev/null
        echo "justfile"
    } | sort -u
)

# Allowlisted paths (text before '#', trimmed; skip comment/blank lines).
mapfile -t allowed < <(grep -vE '^\s*(#|$)' "$allowlist" | sed 's/#.*//; s/[[:space:]]*$//' | sed '/^$/d')
is_allowed() { local f; for f in "${allowed[@]}"; do [ "$f" = "$1" ] && return 0; done; return 1; }

# A line is a COMMENT (not a use) if its first non-space char is # or // or *.
is_comment() {
    local t; t="$(printf '%s' "$1" | sed 's/^[[:space:]]*//')"
    case "$t" in '#'*|'//'*'*'*) return 0;; '*'*) return 0;; '//'*) return 0;; esac
    return 1
}

violations=()
used_files=()
for f in "${files[@]}"; do
    [ -f "$f" ] || continue
    # The gate + its allowlist name the flag by design — never self-flag.
    case "$f" in
        scripts/check-no-allow-multiple-def.sh|scripts/allow-multiple-def-allowlist.txt) continue;;
    esac
    file_has_use=0
    while IFS= read -r ln; do
        [ -z "$ln" ] && continue
        text="${ln#*:}"
        is_comment "$text" && continue
        file_has_use=1
        if ! is_allowed "$f"; then
            violations+=("$f:$ln")
        fi
    done < <(grep -nE "$pat" "$f" 2>/dev/null || true)
    [ "$file_has_use" -eq 1 ] && used_files+=("$f")
done

# Drift the other way: an allowlisted file that no longer uses the flag should be
# removed from the allowlist (keeps the list honest + shrinking).
stale=()
for a in "${allowed[@]}"; do
    found=0; for u in "${used_files[@]:-}"; do [ "$u" = "$a" ] && found=1 && break; done
    [ "$found" -eq 0 ] && stale+=("$a")
done

rc=0
if [ "${#violations[@]}" -gt 0 ]; then
    echo "✗ no-allow-multiple-def: forbidden flag used in non-allowlisted build file(s):" >&2
    printf '   %s\n' "${violations[@]}" >&2
    echo "   Remove the flag (a duplicate defined symbol must be a link error), or — if" >&2
    echo "   genuinely unavoidable — add the file to $allowlist with a reason + issue." >&2
    rc=1
fi
if [ "${#stale[@]}" -gt 0 ]; then
    echo "✗ no-allow-multiple-def: allowlisted file(s) no longer use the flag — drop them from $allowlist:" >&2
    printf '   %s\n' "${stale[@]}" >&2
    rc=1
fi
if [ "$rc" -eq 0 ]; then
    n="${#allowed[@]}"
    if [ "$n" -eq 0 ]; then
        echo "✓ no-allow-multiple-def: zero uses — invariant fully enforced."
    else
        echo "✓ no-allow-multiple-def: $n audited exception(s), all allowlisted (target: 0)."
    fi
fi
exit "$rc"
