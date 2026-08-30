#!/usr/bin/env bash
#
# Phase 313 W6 (#0243) — forbid the retired `nros_board_common::board_init` API.
#
# The legacy board-entry traits (`Board` / `BoardInit` / `BoardPrint` /
# `BoardExit` / `BoardEntry` / `DirectExec`) + the generic direct-exec `run`
# lived in `nros_board_common::board_init` and were deleted in favour of TWO
# canonical board APIs: the Rust-rich `nros_platform::board` surface (session /
# executor sizing / tiers) and the `<nros/board.h>` C ABI (`nros-board-cffi`,
# emitted via `nros_board_export!`). This gate keeps board_init from creeping
# back — a new board that reaches for `nros_board_common::BoardInit` etc. must
# instead impl `nros_platform::board::*` (Rust) or export the C ABI.
#
# The trait NAMES (`BoardInit`/`BoardPrint`/`BoardExit`/`BoardEntry`) are ALSO
# legitimate under `nros_platform`, so a violation is only a legacy trait token
# appearing on the same (non-comment) line as `nros_board_common`. `ThreadxConfig`
# (a config trait that was never part of board_init) stays allowed.
#
# Hooked from `just check` via `just check no-board-init`.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# issue 0726 — both filters below are `grep -q … || continue`, so a grep that
# failed to start SKIPS a line rather than inventing one. That direction is
# quieter and worse: the gate reports the retired API stays dead having examined
# nothing. `nros_grep_q` exits 2 rather than returning "no match", and the
# searches are HERESTRINGS so its `exit` is not trapped in a pipeline subshell.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

# Legacy board_init tokens. `board_init` (the module) + the distinct trait /
# marker names. Bare `Board` is intentionally omitted (too broad: `BoardConfig`
# etc.); the module path `::board_init` covers the `Board` super-trait's home.
legacy='board_init|BoardInit|BoardPrint|BoardExit|BoardEntry|DirectExec'

# Tracked Rust sources only (git index → no build/target/_deps traversal;
# submodules list as one gitlink, so third-party is excluded for free). Skip
# generated + this script's doc.
mapfile -t files < <(
    git ls-files 'packages/**/*.rs' 'examples/**/*.rs' \
        | grep -vE '/(generated|third-party)/'
)

violations=()
for f in "${files[@]}"; do
    [ -f "$f" ] || continue
    while IFS= read -r ln; do
        [ -z "$ln" ] && continue
        num="${ln%%:*}"
        text="${ln#*:}"
        # Strip a trailing line comment, then a full-line doc/comment.
        code="${text%%//*}"
        stripped="$(printf '%s' "$code" | sed 's/^[[:space:]]*//')"
        case "$stripped" in ''|'*'*|'#'*) continue;; esac
        nros_grep_q -E "nros_board_common" <<<"$code" || continue
        nros_grep_q -E "\b($legacy)\b" <<<"$code" || continue
        violations+=("$f:$num:$stripped")
    done < <(grep -nE "nros_board_common" "$f" 2>/dev/null || true)
done

if [ "${#violations[@]}" -gt 0 ]; then
    echo "✗ no-board-init: the retired nros_board_common::board_init API is used:" >&2
    printf '   %s\n' "${violations[@]}" >&2
    echo "   board_init is DELETED. Impl nros_platform::board::* (Rust boards) or" >&2
    echo "   export the <nros/board.h> C ABI via nros_board_export! instead." >&2
    exit 1
fi
echo "✓ no-board-init: the retired board_init API stays dead."
