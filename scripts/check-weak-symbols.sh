#!/usr/bin/env bash
# Issue 0050 / phase-247 W2 — fast source-level weak-symbol gate.
#
# Scans owned C/C++/asm sources for weak declarations (`__attribute__((weak))`
# / `.weak`) and fails when a file outside the audited allowlist introduces one,
# or a listed file's weak-decl count drifts (a weak symbol added/removed without
# re-audit). Buildless + sub-second — fits the `just check` aggregate (cf. the
# other scripts/check-*.sh gates). The deeper per-platform *image* gate is
# scripts/check-weak-symbols-image.sh (needs prebuilt fixtures, runs under CI).
#
# Allowlist source of truth: scripts/weak-symbols-allowlist.txt (shared with
# nros-tests/tests/weak_symbol_audit.rs).

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"

allowlist="$script_dir/weak-symbols-allowlist.txt"
[ -f "$allowlist" ] || { echo "weak-source: missing $allowlist" >&2; exit 1; }

# Expected counts keyed by path (from the allowlist, comments/blank stripped).
declare -A expected
while read -r count path _rest; do
    [ -z "${count:-}" ] && continue
    case "$count" in \#*) continue ;; esac
    expected["$path"]="$count"
done < <(sed -E 's/#.*//' "$allowlist")

# phase-386 W1 — every audited row must declare `body:<kind>`, the answer to
# "if nobody overrides this, is the weak body CORRECT?". The existing
# override-default/optional-hook classification answers a DIFFERENT question
# (is a strong def guaranteed) and the two are independent, so a row can be
# correctly classified there and still hide a stub that lies.
#
# Validated here rather than left as prose because an unchecked column drifts:
# a new row would simply omit it, and the axis would decay into a comment on
# the subset of rows that happened to get one. `silent-wrong` is deliberately
# NOT an accepted value — that state is the bug this axis exists to surface,
# and a row needing it should be fixed instead (phase-386 W2 removed the two
# that had it).
missing_body=""
bad_body=""
while IFS= read -r line; do
    case "$line" in \#*|"") continue;; esac
    rowpath=$(printf '%s' "$line" | awk '{print $2}')
    [ -n "$rowpath" ] || continue
    body=$(printf '%s' "$line" | sed -nE 's/.*body:([a-z-]+).*/\1/p')
    if [ -z "$body" ]; then
        missing_body="$missing_body  $rowpath"$'\n'
    else
        case "$body" in
            correct|reports-failure|self-enforcing) ;;
            *) bad_body="$bad_body  $rowpath -> body:$body"$'\n' ;;
        esac
    fi
done < "$allowlist"

if [ -n "$missing_body" ] || [ -n "$bad_body" ]; then
    echo "weak-source: allowlist rows with a missing/invalid \`body:\` axis:" >&2
    [ -n "$missing_body" ] && { echo "  MISSING:" >&2; printf '%s' "$missing_body" >&2; }
    [ -n "$bad_body" ] && { echo "  INVALID:" >&2; printf '%s' "$bad_body" >&2; }
    echo >&2
    echo "  Answer: if nobody overrides it, is the weak body CORRECT?" >&2
    echo "    body:correct         a valid runtime state; nothing is missing" >&2
    echo "    body:reports-failure says so in a form the CALLER understands" >&2
    echo "    body:self-enforcing  misuse faults immediately; do not 'fix' it" >&2
    echo >&2
    echo "  There is no body:silent-wrong. A row that would need it is the bug" >&2
    echo "  this axis exists to surface — fix the stub, do not label it." >&2
    exit 1
fi

# Walk owned C/C++/asm, skipping vendored / build / generated trees.
declare -A actual
while IFS= read -r f; do
    # Strip comments before counting. The attribute is DISCUSSED in prose next
    # to nearly every real use — "`__attribute__((weak))` so a C/C++ image can
    # define this symbol strongly" — and counting those made the gate report
    # drift that does not exist: phase-366 added one such sentence to the
    # threadx port and its count went 8 -> 9 with no new weak symbol, while
    # posix's single weak decl counted as 2. A gate that cries wolf on its own
    # documentation gets bypassed (issue 0555 makes the same point).
    #
    # `cpp -fpreprocessed` removes comments without expanding anything, so
    # `#include`s and macros are untouched and a `.S` file survives it. Falls
    # back to the raw file if cpp is unavailable or chokes.
    stripped=$(cpp -fpreprocessed -dD -P "$f" 2>/dev/null) || stripped=$(cat "$f")
    n=$(printf '%s\n' "$stripped" | grep -cE '__attribute__\(\(weak\)\)|\.weak ' || true)
    [ "${n:-0}" -gt 0 ] && actual["$f"]="$n"
done < <(git ls-files 'packages/**' \
            | grep -E '\.(c|cpp|cc|h|hpp|S|s)$' \
            | grep -vE '/(target|build|generated|zenoh-pico|mbedtls|third-party)/')

fails=0

# Unexpected (new unaudited site) + drifted counts.
for f in "${!actual[@]}"; do
    if [ -z "${expected[$f]:-}" ]; then
        echo "  FAIL  $f: ${actual[$f]} weak decl(s) — NEW unaudited weak-symbol site." >&2
        echo "        Audit it (override-default vs optional-hook, strong-def source), then add to $allowlist." >&2
        fails=$((fails + 1))
    elif [ "${actual[$f]}" != "${expected[$f]}" ]; then
        echo "  FAIL  $f: weak-decl count ${actual[$f]}, allowlist expects ${expected[$f]} — re-audit + update $allowlist." >&2
        fails=$((fails + 1))
    fi
done

# Stale allowlist entries (file moved / weak removed).
for f in "${!expected[@]}"; do
    if [ -z "${actual[$f]:-}" ]; then
        echo "  FAIL  $f: allowlisted but no weak decl found — drop it from $allowlist." >&2
        fails=$((fails + 1))
    fi
done

if [ "$fails" -gt 0 ]; then
    echo "weak-source: FAILED ($fails) — weak-symbol allowlist out of date (issue 0050)." >&2
    exit 1
fi
echo "weak-source: ${#actual[@]} audited weak-symbol files OK."
exit 0
