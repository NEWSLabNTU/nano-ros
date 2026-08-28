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
# Scope: emitter code only — packages/**/src and cmake/*.cmake, tracked files.
# The test harness (packages/testing) is contributor-only and exempt, as are
# comments (lines whose string context is a `//` / `#` comment).
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# issue 0726 — the three searches below decide whether a line is a finding, and
# two of them decide it by ABSENCE (`|| continue`, and the bootstrap.sh
# neighbourhood). A grep that failed to start would therefore either skip a real
# offender or report one that is licensed. HERESTRINGS, not pipes: the helper
# must run in this shell for its `exit 2` to end the gate.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

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
done < <(git grep -n 'just setup' -- 'packages/cli/**/*.rs' 'packages/core/**/*.rs' \
             'packages/platform/**/*.rs' 'packages/boards/**/*.rs' 'cmake/*.cmake' \
             ':!**/tests/**' 2>/dev/null)

if [ "$fail" -ne 0 ]; then
    echo "check-emitter-just-spelling: user-reachable messages must name" >&2
    echo "  ./scripts/bootstrap.sh (contributors: just <recipe>) — see phase-368." >&2
    exit 1
fi
echo "check-emitter-just-spelling: OK (no bare-just prescriptions in emitter strings)"
