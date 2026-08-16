# Check skip ledger — issue 0650 (second half).
#
# The lane protocol in `lane-skip.sh` answers "this fixture lane built nothing"
# with exit 78, because a lane that skips has produced no artifact and the
# driver must not record OK. A CHECK is different in one way that matters: it
# claims no artifact, and `check-fast` carries a documented property —
#
#   "a pristine detached worktree with no CLI, no sources and no `nros sync`
#    runs this lane green in 23s"
#
# — so a gate that hard-fails on a missing optional tool would break the very
# guarantee that makes the fast tier worth running. Six such gates therefore
# printed a skip line and exited 0: `check-abi-bindings` without `bindgen`,
# `dep-chain` without ROS 2, `check-board-projections` without the in-tree CLI,
# `colcon-parity` without colcon, and the two doxygen recipes.
#
# The defect is not the exit code. It is that `just check` then prints
# "All checks passed!" — a sentence that is false about the gates that did not
# run, and the one a reader remembers. Same shape as the lane's false "fixtures
# built", one tier over.
#
# So: a skipped check is RECORDED, and the lane REPORTS the ledger at the end.
# Exit codes are unchanged, the bare-worktree property holds, and the summary
# says what was not verified instead of implying everything was.
#
# Usage:
#   nros_check_skip_reset                     # first step of the lane
#   nros_check_skip <name> "<reason>"         # in the gate, before `exit 0`
#   nros_check_skip_report "<success line>"   # instead of that success echo

if ! command -v nros_build_dir >/dev/null 2>&1; then
    # shellcheck source=scripts/build/build-root.sh
    . "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/build-root.sh"
fi

_nros_check_skip_file() {
    printf '%s/checks.skipped' "$(nros_build_dir "$NROS_KIND_CHECK_SKIPS")"
}

nros_check_skip_reset() {
    local f
    f="$(_nros_check_skip_file)"
    mkdir -p "$(dirname "$f")"
    : > "$f"
}

# nros_check_skip <name> <reason…>
#
# Records and announces. Does NOT exit — the caller decides, because some sites
# skip a whole recipe and others skip one step of it.
nros_check_skip() {
    local name="${1:?nros_check_skip: name}"
    shift
    local reason="$*"
    local f
    f="$(_nros_check_skip_file)"
    mkdir -p "$(dirname "$f")"
    printf '%s\t%s\n' "$name" "$reason" >> "$f"
    echo "[SKIPPED] ${name}: ${reason}"
}

# nros_check_skip_report <success-line>
#
# The lane's closing sentence. With an empty ledger it is the success line
# unchanged; otherwise the success line is QUALIFIED by name, because "All
# checks passed!" over a gate that never ran is the false statement this exists
# to remove. Exit status is unchanged either way (0): these are missing tools,
# not failures, and `check-fast` must stay green on a bare worktree.
nros_check_skip_report() {
    local success="$*"
    local f
    f="$(_nros_check_skip_file)"
    if [ ! -s "${f}" ]; then
        [ -n "$success" ] && echo "$success"
        return 0
    fi
    local n
    n="$(grep -c . "$f")"
    echo "${success} — but ${n} check(s) did NOT run, so they verified nothing:"
    while IFS=$'\t' read -r name reason; do
        [ -n "$name" ] || continue
        echo "  - ${name}: ${reason}"
    done < "$f"
    echo "  Install what they need, or accept that this green is narrower than it looks."
}
