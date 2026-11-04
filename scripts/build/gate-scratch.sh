# shellcheck shell=bash
# Issue 2239 — a gate's scratch trees outside the checkout, retired when
# nothing can need them.
#
# `check-template-copy-out` and `check-scaffold-builds` build in a fresh tree
# under `${TMPDIR:-/tmp}`, because a copy-out must have no ancestor that is the
# checkout (issue 1453). On a developer host /tmp is often a RAM-backed tmpfs,
# and 79 trees (15.5 GB) had piled up there on 2026-11-04: nothing removed the
# tree of a FAILED run, or of an INTERRUPTED one.
#
# The rule for a tree:
#
#   passed       removed by the gate as soon as it has its verdict (as before);
#   failed       KEPT — it is the evidence, and the failure message prints its
#                path — until a later run of the SAME gate and scope starts, or
#                CI asks for its output released (`NROS_CI_PRUNE_GATE_OUTPUTS`);
#   interrupted  removed on the way out (EXIT/INT/TERM), or, after a SIGKILL
#                that no trap sees, by the next run of the gate.
#
# So a host keeps at most the newest run's failures per gate and scope. A tree
# whose owner is still running is never touched, whoever asks: two worktrees
# may run the same gate at once.
#
# Every tree carries `.nros-gate-scratch` (gate, scope, owner pid and start
# time, verdict). A tree without one predates this file and is left alone: its
# owner cannot be proved dead.
#
# Usage, after sourcing:
#   nros_gate_scratch_init <gate> <scope>     once, before the first tree
#   nros_gate_scratch <var> <name>            mktemp -d "$TMPDIR/nros-<name>.XXXXXX"
#   nros_gate_scratch_failed <dir>            keep <dir> as failure evidence

_NROS_GATE_SCRATCH_MARK=".nros-gate-scratch"
_NROS_GATE_SCRATCH_GATE=""
_NROS_GATE_SCRATCH_SCOPE=""

_nros_gate_scratch_root() {
    printf '%s' "${TMPDIR:-/tmp}"
}

# A process's start time, so a recycled pid is not mistaken for the owner.
# Field 22 of /proc/<pid>/stat, read after the `)` that ends the command name
# (which may itself hold spaces). Empty where /proc is absent.
_nros_gate_scratch_start() {
    local stat
    stat="$(cat "/proc/$1/stat" 2>/dev/null)" || return 0
    stat="${stat##*) }"
    # shellcheck disable=SC2086
    set -- $stat
    printf '%s' "${20:-}"
}

_nros_gate_scratch_alive() {
    local pid="$1" start="$2" now
    kill -0 "$pid" 2>/dev/null || return 1
    [ -n "$start" ] || return 0
    now="$(_nros_gate_scratch_start "$pid")"
    [ -z "$now" ] || [ "$now" = "$start" ]
}

_nros_gate_scratch_field() {
    sed -n "s/^$2=//p" "$1/$_NROS_GATE_SCRATCH_MARK" 2>/dev/null | head -n1
}

# Remove what an earlier run of this gate left and no longer needs.
_nros_gate_scratch_sweep() {
    local d gate scope pid start verdict
    for d in "$(_nros_gate_scratch_root)"/nros-*.*; do
        [ -d "$d" ] && [ -f "$d/$_NROS_GATE_SCRATCH_MARK" ] || continue
        gate="$(_nros_gate_scratch_field "$d" gate)"
        [ "$gate" = "$_NROS_GATE_SCRATCH_GATE" ] || continue
        pid="$(_nros_gate_scratch_field "$d" pid)"
        start="$(_nros_gate_scratch_field "$d" start)"
        [ -n "$pid" ] && _nros_gate_scratch_alive "$pid" "$start" && continue
        verdict="$(_nros_gate_scratch_field "$d" verdict)"
        scope="$(_nros_gate_scratch_field "$d" scope)"
        if [ "$verdict" = "failed" ] && [ "$scope" != "$_NROS_GATE_SCRATCH_SCOPE" ]; then
            continue # another scope's evidence (a different --part or shard)
        fi
        rm -rf -- "$d"
    done
}

# On the way out: this run's trees that never reached a kept verdict.
_nros_gate_scratch_exit() {
    local d
    for d in "$(_nros_gate_scratch_root)"/nros-*.*; do
        [ -d "$d" ] && [ -f "$d/$_NROS_GATE_SCRATCH_MARK" ] || continue
        [ "$(_nros_gate_scratch_field "$d" pid)" = "$$" ] || continue
        [ "$(_nros_gate_scratch_field "$d" gate)" = "$_NROS_GATE_SCRATCH_GATE" ] || continue
        [ "$(_nros_gate_scratch_field "$d" verdict)" = "failed" ] && continue
        rm -rf -- "$d"
    done
}

nros_gate_scratch_init() {
    _NROS_GATE_SCRATCH_GATE="$1"
    _NROS_GATE_SCRATCH_SCOPE="$2"
    _nros_gate_scratch_sweep
    trap _nros_gate_scratch_exit EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
}

nros_gate_scratch() {
    local __var="$1" __name="$2" __dir
    __dir="$(mktemp -d "$(_nros_gate_scratch_root)/nros-$__name.XXXXXX")" || return 1
    printf 'gate=%s\nscope=%s\npid=%s\nstart=%s\n' \
        "$_NROS_GATE_SCRATCH_GATE" "$_NROS_GATE_SCRATCH_SCOPE" "$$" \
        "$(_nros_gate_scratch_start "$$")" > "$__dir/$_NROS_GATE_SCRATCH_MARK"
    printf -v "$__var" '%s' "$__dir"
}

nros_gate_scratch_failed() {
    [ -f "$1/$_NROS_GATE_SCRATCH_MARK" ] && echo "verdict=failed" >> "$1/$_NROS_GATE_SCRATCH_MARK"
    return 0
}

# The negative controls: `bash scripts/build/gate-scratch.sh --self-test`.
# Each case plants trees in a private root, runs the rule, and asserts which
# ones survive, so a sweep that removes too much (a live run's tree, another
# scope's evidence, a tree it cannot attribute) fails as loudly as one that
# removes too little.
_nros_gate_scratch_self_test() {
    local root fail=0 dead live
    root="$(mktemp -d "${TMPDIR:-/tmp}/nros-gate-scratch-test.XXXXXX")" || return 2
    true & dead=$!; wait "$dead"
    sleep 30 & live=$!

    plant() { # <name> <gate> <scope> <pid> [verdict]
        mkdir -p "$root/nros-$1.x"
        printf 'gate=%s\nscope=%s\npid=%s\nstart=\n' "$2" "$3" "$4" > "$root/nros-$1.x/$_NROS_GATE_SCRATCH_MARK"
        [ -z "${5:-}" ] || echo "verdict=$5" >> "$root/nros-$1.x/$_NROS_GATE_SCRATCH_MARK"
    }
    expect() { # <name> present|absent <case>
        local got=absent
        [ -d "$root/nros-$1.x" ] && got=present
        if [ "$got" != "$2" ]; then
            echo "gate-scratch self-test FAIL: $3 — nros-$1 is $got, want $2" >&2
            fail=1
        fi
    }

    plant orphan g s1 "$dead"
    plant old-fail g s1 "$dead" failed
    plant other-scope g s2 "$dead" failed
    plant running g s1 "$live"
    plant other-gate h s1 "$dead"
    mkdir -p "$root/nros-unmarked.x"
    (
        TMPDIR="$root"
        _NROS_GATE_SCRATCH_GATE=g _NROS_GATE_SCRATCH_SCOPE=s1
        _nros_gate_scratch_sweep
    )
    expect orphan absent "an interrupted run's tree (owner dead, no verdict)"
    expect old-fail absent "an earlier failure of the same scope"
    expect other-scope present "another scope's failure evidence"
    expect running present "a tree whose owner is still running"
    expect other-gate present "another gate's tree"
    expect unmarked present "a tree with no marker (owner unprovable)"

    # The exit path: a run's unverdicted tree goes, its failure stays.
    local here
    here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/gate-scratch.sh"
    TMPDIR="$root" bash -c '
        . "$1"
        nros_gate_scratch_init g exit
        nros_gate_scratch a run-a; nros_gate_scratch b run-b
        nros_gate_scratch_failed "$b"
        printf "%s\n%s\n" "$a" "$b" > "$2/names"
    ' _ "$here" "$root"
    if [ -d "$(sed -n 1p "$root/names")" ]; then
        echo "gate-scratch self-test FAIL: a finished run's unverdicted tree survived its exit" >&2; fail=1
    fi
    if [ ! -d "$(sed -n 2p "$root/names")" ]; then
        echo "gate-scratch self-test FAIL: a finished run's failure evidence was removed at exit" >&2; fail=1
    fi

    # The interrupt path: SIGINT mid-run removes the tree.
    TMPDIR="$root" bash -c '
        . "$1"
        nros_gate_scratch_init g int
        nros_gate_scratch a run-int
        echo "$a" > "$2/int"
        kill -INT $$
        sleep 5
    ' _ "$here" "$root" || true
    if [ -d "$(cat "$root/int")" ]; then
        echo "gate-scratch self-test FAIL: an interrupted run's tree survived SIGINT" >&2; fail=1
    fi

    kill "$live" 2>/dev/null; wait "$live" 2>/dev/null
    rm -rf -- "$root"
    [ "$fail" -eq 0 ] && echo "gate-scratch self-test OK: sweep keeps live, other-scope, other-gate and unmarked trees; exit and SIGINT keep only failures."
    return "$fail"
}

if [ "${BASH_SOURCE[0]}" = "$0" ] && [ "${1:-}" = "--self-test" ]; then
    _nros_gate_scratch_self_test
    exit $?
fi
