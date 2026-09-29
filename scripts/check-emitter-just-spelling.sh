#!/usr/bin/env bash
# User-reachable tool messages must not prescribe a bare `just` recipe.
#
# phase-368: the book's user track dropped `just` (a contributor dependency),
# and the front door (`./scripts/bootstrap.sh`) builds everything the quick
# start needs — but fifteen ERROR STRINGS still told users to run
# `just setup-cli` / `just setup-launch-resolve`, including the exact error a
# fresh user hits first (the clean-container probe hit two of them). Those
# were fixed by naming the user spelling first with the contributor recipe as
# an alias; this gate keeps the class closed: any Rust/CMake STRING that
# prescribes a `just setup*` recipe must, in the same string-bearing line or
# its neighbors, also name `bootstrap.sh`.
#
# Scope: every tracked Rust and CMake file, by KIND (`scripts/lib/file_kinds.py`,
# phase-472 W5) — not a directory list. The list was `packages/{cli,core,
# platform,boards}/**/*.rs` + `cmake/*.cmake`, and the messages had spread to
# `zephyr/`, `packages/**/cmake`, `packages/drivers`, `packages/api`. Stated
# narrowing: `tests/` directories and `packages/testing/` (the contributor-only
# harness), and comments (lines whose string context is a `//` / `#` comment).
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
GATE_ROOT="$PWD"

# issue 0726 — the three searches below decide whether a line is a finding, and
# two of them decide it by ABSENCE (`|| continue`, and the bootstrap.sh
# neighbourhood). A grep that failed to start would therefore either skip a real
# offender or report one that is licensed. HERESTRINGS, not pipes: the helper
# must run in this shell for its `exit 2` to end the gate.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

# population <root> — NUL-separated emitter files under <root>.
population() {
    (cd "$1" && python3 "$GATE_ROOT/scripts/lib/file_kinds.py" -z rust cmake \
        --exclude-part tests --exclude-prefix packages/testing/)
}

# scan <root> — `file:line:text` of every `just setup` line in the population.
scan() {
    local files
    files="$(population "$1" | tr '\0' '\n')" || return 2
    (cd "$1" && printf '%s\n' "$files" | xargs -d '\n' grep -n -H 'just setup' -- 2>/dev/null) || true
}

# Negative control on the normal path (phase-472 W5/W9): a prescription in
# `zephyr/CMakeLists.txt` — outside the old directory list — is in the scan.
self_test() {
    local t out
    t="$(mktemp -d)"
    mkdir -p "$t/zephyr" "$t/cmake"
    printf '%s\n' 'message(FATAL_ERROR "run `just setup-cli`")' > "$t/zephyr/CMakeLists.txt"
    printf '%s\n' 'set(X 1)' > "$t/cmake/a.cmake"
    printf '%s\n' 'mod m' > "$t/justfile"
    git -C "$t" init -q && git -C "$t" add -A
    out="$(scan "$t")"
    rm -rf "$t"
    case "$out" in
        *zephyr/CMakeLists.txt:1:*) ;;
        *) echo "check-emitter-just-spelling SELFTEST FAILED: zephyr/ is not in the population" >&2
           exit 1 ;;
    esac
}

self_test
# shellcheck source=scripts/lib/population.sh
source scripts/lib/population.sh
n="$(population "$PWD" | tr -cd '\0' | wc -c)"
nros_require_population "$n" "Rust/CMake emitter file(s)" check-emitter-just-spelling || exit 1

fail=0
while IFS=: read -r file line text; do
    # comment lines are not emitters
    case "$text" in
        *'//'*'just setup'*) stripped="${text%%//*}";;
        *) stripped="$text";;
    esac
    nros_grep_q '"[^"]*just setup' <<<"$stripped" || continue
    # a cmake `#` comment
    nros_grep_q -E '^[[:space:]]*#' <<<"$stripped" && continue
    # licensed when bootstrap.sh appears within +/-3 lines
    lo=$((line > 3 ? line - 3 : 1))
    neighbourhood="$(sed -n "${lo},$((line + 3))p" "$file")"
    if nros_grep_q 'bootstrap\.sh' <<<"$neighbourhood"; then
        continue
    fi
    echo "  $file:$line: prescribes a just recipe with no user spelling nearby" >&2
    echo "      $text" | cut -c1-110 >&2
    fail=1
done < <(scan "$PWD")

if [ "$fail" -ne 0 ]; then
    echo "check-emitter-just-spelling: user-reachable messages must name" >&2
    echo "  ./scripts/bootstrap.sh (contributors: just <recipe>) — see phase-368." >&2
    exit 1
fi
echo "check-emitter-just-spelling: OK (no bare-just prescriptions in emitter strings)"
