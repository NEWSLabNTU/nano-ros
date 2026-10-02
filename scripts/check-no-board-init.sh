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
# issue 1615 (W6): per IMPORTED PATH (use-trees expanded across lines, comments
# stripped — `scripts/lib/per_item.py`), plus any qualified path in code. A
# `use nros_board_common::{` with `board_init::BoardInit` on the next line was
# invisible to the line-at-a-time scan.
scan_rc=0
scan_out="$(LEGACY="$legacy" python3 - <<'PY'
import os, re, sys
sys.path.insert(0, "scripts/lib")
import comments, file_kinds, per_item

LEG = re.compile(r"\b(%s)\b" % os.environ["LEGACY"])

def hits_in(text):
    code = comments.strip_comments(text, "rust")
    out = set()
    for p, off in per_item.rust_use_paths(code):
        if p.startswith("nros_board_common::") and LEG.search(p):
            out.add((code.count("\n", 0, off) + 1, p))
    for m in re.finditer(r"\bnros_board_common::[A-Za-z0-9_:]+", code):
        if LEG.search(m.group(0)):
            out.add((code.count("\n", 0, m.start()) + 1, m.group(0)))
    return sorted(out)

assert hits_in("use nros_board_common::{\n    board_init::BoardInit,\n};\n")
assert not hits_in("// use nros_board_common::board_init;\nuse nros_board_common::x;\n")
for rel in file_kinds.files_of_kind("rust"):
    if not rel.startswith(("packages/", "examples/")):
        continue
    for n, p in hits_in(open(rel, errors="replace").read()):
        print(f"{rel}:{n}:{p}")
PY
)" || scan_rc=$?
if [ "$scan_rc" -ne 0 ]; then
    echo "✗ no-board-init: the scan itself failed (rc=$scan_rc) — not a pass." >&2
    exit 1
fi
violations=()
[ -n "$scan_out" ] && mapfile -t violations <<<"$scan_out"

if [ "${#violations[@]}" -gt 0 ]; then
    echo "✗ no-board-init: the retired nros_board_common::board_init API is used:" >&2
    printf '   %s\n' "${violations[@]}" >&2
    echo "   board_init is DELETED. Impl nros_platform::board::* (Rust boards) or" >&2
    echo "   export the <nros/board.h> C ABI via nros_board_export! instead." >&2
    exit 1
fi
echo "✓ no-board-init: the retired board_init API stays dead."
