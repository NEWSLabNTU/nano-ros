#!/usr/bin/env bash
# Phase 208.D.4 / 208.D.8 / 212.H.6 — string-convention guards.
#
# Catches strings that should not appear anywhere in user-facing surfaces
# (book/, integrations/, packages/, examples/, scripts/, just/, integrations/):
#
#   1. `aeon/nano-ros` — the wrong GitHub org (real is NEWSLabNTU/nano-ros).
#      Surfaced via the Phase 208 book audit (P11). 208.D.4.
#
# Phase 212.H.6 reintroduces a PlatformIO adapter (ahead-of-vendor codegen
# path); the former 208.D.8 PlatformIO ban is therefore retired.
#
# Roadmap + archived phase docs may reference these strings historically; they
# are excluded by directory.
#
# Exit 1 on hit, 0 on clean. Pure static lint (grep); seconds to run.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

# Scratch goes in `$repo/tmp/` (gitignored), not the system `/tmp` —
# CLAUDE.md's rule, and it matters most for a gate: these run on shared
# hosts where a leftover `/tmp/string-conv-hits.*` outlives the run that
# made it.
mkdir -p tmp

fail=0

scan() {
    local label="$1"; shift
    local pattern="$1"; shift
    local hits
    hits="$(mktemp tmp/string-conv-hits.XXXXXX)"
    # Paths to scan are the remaining args.
    if git grep -nE "$pattern" -- "$@" \
            ':!docs/roadmap/archived/*' \
            ':!docs/roadmap/phase-208-*' \
            ':!scripts/ci/string-conventions-check.sh' \
            ':!.git/*' \
        > "$hits" 2>/dev/null; then
        echo "::error::$label: forbidden string found"
        cat "$hits"
        fail=1
    fi
    rm -f "$hits"
}

scan "aeon/nano-ros (real org = NEWSLabNTU/nano-ros)" \
     'aeon/nano-ros' \
     'book/' 'integrations/' 'packages/' 'examples/' 'scripts/' \
     'just/' 'justfile' 'docs/'

if [ "$fail" -eq 0 ]; then
    echo "string conventions: OK"
fi
exit "$fail"
