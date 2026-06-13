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

# Walk owned C/C++/asm, skipping vendored / build / generated trees.
declare -A actual
while IFS= read -r f; do
    n=$(grep -cE '__attribute__\(\(weak\)\)|\.weak ' "$f" 2>/dev/null || true)
    [ "${n:-0}" -gt 0 ] && actual["$f"]="$n"
done < <(find packages -type f \
            \( -name '*.c' -o -name '*.cpp' -o -name '*.cc' -o -name '*.h' \
               -o -name '*.hpp' -o -name '*.S' -o -name '*.s' \) \
            -not -path '*/target/*' -not -path '*/build/*' -not -path '*/generated/*' \
            -not -path '*/zenoh-pico/*' -not -path '*/mbedtls/*' -not -path '*/third-party/*')

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
