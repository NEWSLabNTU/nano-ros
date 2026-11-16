#!/usr/bin/env bash
# Shared jobserver fan-out for build scripts whose units are independent.
#
# WHY THIS EXISTS
#
# Several fixture stages walk a list of independent units — stage a tree, run
# `cargo check`, run a `cmake --build` — in a plain `while read` loop. On a
# 32-core host that leaves the machine ~95 % idle: measured on
# `compile-check-fixtures.sh`, 5 of 27 rows in 10 minutes with ONE rustc
# running and a load average of 7.
#
# The repo already had the right answer twice (`workspace-fixtures-build.sh`,
# `zephyr-fixture-make-driver.sh`): generate a make graph and run it under
# pinned make 4.4's FIFO jobserver. This is that idiom, extracted, so a third
# and fourth caller do not become a third and fourth spelling of it — the
# drift class CLAUDE.md names.
#
# WHY A JOBSERVER RATHER THAN `xargs -P` / GNU parallel
#
# The units themselves spawn cargo and cmake, which are parallel too. A plain
# `-P N` multiplies: N units × each tool's own width oversubscribes the box and
# thrashes. Under a jobserver every descendant draws from ONE token pool, so
# total concurrency stays at the budget no matter how the work is shaped.
#
# USAGE
#
#   source scripts/build/jobserver-pool.sh
#   nros_pool_run <label> <<'EOF'
#   bash scripts/build/thing.sh --id a
#   bash scripts/build/thing.sh --id b
#   EOF
#
# One command per line, each self-contained (units re-invoke their own script
# narrowed to one id). Returns non-zero if any unit fails. Falls back to a
# serial walk — same order, same output — when a pool is unavailable or
# pointless, so callers need no second code path.

# Issue 1880 — the CLI that answers `sdk-path`, from the ONE ladder
# (`nros_cli_bin`, scripts/build/cargo.sh), never PATH alone: a linked worktree
# inherits its parent's PATH, whose `nros` reads the PARENT's index pins.
_NROS_POOL_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
_nros_pool_cli() {
    if ! declare -F nros_cli_bin >/dev/null 2>&1; then
        # shellcheck source=cargo.sh
        . "$_NROS_POOL_LIB_DIR/cargo.sh" || return 1
    fi
    nros_cli_bin 2>/dev/null
}

# Is a pool worth setting up?
#   * NROS_JOBSERVER=1 means an OUTER jobserver already owns the tokens; nesting
#     a second pool would double-count them.
#   * pinned make 4.4 is required for `--jobserver-style=fifo`; the system make
#     may be 4.3, whose pipe-based jobserver child tools cannot join.
# The pinned make's / ninja's absolute path, or empty. CONSTRUCTED via `nros
# dev sdk path` (issue 0625) rather than searched: a system make 4.3 first on
# PATH would answer, and its pipe-FD jobserver is the thing that does not work.
#
# phase-489 W13 adds ONE other answer: a copy the CI images bake under
# `/usr/local` (`index_tool.py install-source make`, `install ninja`), which is
# in no store. `index_tool.py resolve` accepts a PATH copy only when it passes
# the index row's own smoke probe (`GNU Make 4.4.1`, `1.13.2`), so a system make
# 4.3 or ninja 1.10 still never answers. It is also the answer when no CLI is
# built yet, which is every image job before `setup-cli`.
_nros_pinned_tool() { # <tool> <program>
    local cli dir found
    if cli="$(_nros_pool_cli)" && dir="$("$cli" dev sdk path "$1" 2>/dev/null)" \
        && [ -n "$dir" ] && [ -x "$dir/bin/$2" ]; then
        printf '%s\n' "$dir/bin/$2"
        return 0
    fi
    found="$(python3 "$_NROS_POOL_LIB_DIR/../lib/index_tool.py" resolve "$1" "$2" 2>/dev/null)" \
        || return 1
    [ -x "$found" ] || return 1
    printf '%s\n' "$found"
}
nros_pinned_make() { _nros_pinned_tool make make; }
# ninja >=1.13 is the half that JOINS make 4.4's fifo jobserver.
nros_pinned_ninja() { _nros_pinned_tool ninja ninja; }

# The PATH prefix a pool's CHILDREN need — issue 2119 (1638's class, one
# driver further). A pool runs the pinned make by ABSOLUTE path, but its
# children find their make by NAME: a cmake "Unix Makefiles" configure caches
# whatever `gmake` (searched before `make`) PATH resolves, and that make joins
# the pool's `--jobserver-auth=fifo:` as a client. The system make 4.3 cannot
# parse a fifo jobserver and dies with `invalid --jobserver-auth string`. So a
# script that runs the pinned make with `-j` puts THAT make's directory first —
# derived from the make it runs, never resolved a second time — plus the pinned
# ninja's (>= 1.13, the half that joins a fifo jobserver) when it exists.
# Prints `<make dir>[:<ninja dir>]`; use as `export PATH="$(nros_pinned_tool_path_prefix
# "$make_bin"):$PATH"`. Gate: `check-pinned-make-path-exported`.
nros_pinned_tool_path_prefix() {
    local make_bin="${1:?make binary}" prefix ninja
    prefix="$(dirname "$make_bin")"
    if ninja="$(nros_pinned_ninja)"; then
        prefix="$prefix:$(dirname "$ninja")"
    fi
    printf '%s\n' "$prefix"
}

# phase-535 W3 — JOIN an inherited pool rather than open a second one.
#
# `NROS_INHERIT_JOBSERVER=1` already told a stage "your tools draw from an
# outer token pool" (issue 0726's pooled `build-test-fixtures`), and the stage
# recipes act on it by dropping their explicit cmake width. The fan-out
# drivers below them did not: each ran its own pinned make with `-j<budget>`
# behind `env -u MAKEFLAGS`, i.e. a SECOND scheduler with a full budget of its
# own, so two pooled stages could run twice the budget between them.
#
# A make that finds a fifo jobserver in MAKEFLAGS and is given no `-j` is a
# CLIENT of it: its first job runs in the slot its parent spent on it, every
# further job takes a token from the same pool. So under the flag, and only
# when the fifo it names exists, a driver keeps MAKEFLAGS and drops `-j`.
# Succeeds (exit 0) when that is the case.
nros_inherited_jobserver() {
    [ "${NROS_INHERIT_JOBSERVER:-}" = "1" ] || return 1
    local flags=" ${MAKEFLAGS:-} " fifo
    case "$flags" in
        *" --jobserver-auth=fifo:"*) ;;
        *) return 1 ;;
    esac
    fifo="${flags##* --jobserver-auth=fifo:}"
    fifo="${fifo%% *}"
    [ -p "$fifo" ]
}

# The make arguments a fan-out driver runs its pool with: `-j<n>
# --jobserver-style=fifo` to open a pool, nothing to join an inherited one
# (`nros_inherited_jobserver`). One line per argument.
nros_pool_make_args() {
    local jobs="${1:?jobs}"
    if nros_inherited_jobserver; then
        return 0
    fi
    printf '%s\n' "-j$jobs" "--jobserver-style=fifo"
}

nros_pool_available() {
    local units="$2"
    [ "${NROS_JOBSERVER:-}" = "1" ] && return 1
    [ "${units:-0}" -gt 1 ] || return 1
    local pinned_make
    pinned_make="$(nros_pinned_make)" || return 1
    "$pinned_make" --version | head -1 | grep -q "4.4" || return 1
    return 0
}

# The subtree-guard name a pool runs under — issue 1886. One guard per LABEL:
# a label names one script's unit list, i.e. one artifact set, which is what
# the guard's refusal protects. A label is a bare token at every call site
# (`check-examples`, `compile-check`, `no-std`, `px4-companion`,
# `build-all-extras`); anything else is folded to `_` so it stays one path
# segment of the lock file.
nros_pool_guard_name() {
    local label="${1:?label}"
    printf 'jobserver-pool-%s\n' "${label//[^A-Za-z0-9_.-]/_}"
}

# nros_pool_run <label> [work_root]   — commands on stdin, one per line.
nros_pool_run() {
    local label="$1"
    local script_dir repo_root
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    repo_root="$(cd "$script_dir/../.." && pwd)"
    local work_root="${2:-${NROS_BUILD_LOG_DIR:-$repo_root/build}/jobserver-pool}"

    local -a cmds=()
    local line
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        cmds+=("$line")
    done

    local n=${#cmds[@]}
    [ "$n" -gt 0 ] || return 0

    if ! nros_pool_available "$repo_root" "$n"; then
        # Serial fallback: one outer jobserver already owns the tokens, or
        # there is nothing to gain. Same commands, same order.
        local rc=0 c
        for c in "${cmds[@]}"; do
            bash -c "$c" || rc=1
        done
        return $rc
    fi

    mkdir -p "$work_root"
    local pool_jobs makefile i
    pool_jobs="${NROS_BUILD_JOBS:-$(nproc 2>/dev/null || echo 8)}"
    makefile="$work_root/$label-$$.mk"

    {
        printf '# Generated by jobserver-pool.sh (%s, %d unit(s))\n' "$label" "$n"
        printf '.DELETE_ON_ERROR:\n.PHONY: all'
        for ((i = 0; i < n; i++)); do printf ' u%d' "$i"; done
        printf '\n\nall:'
        for ((i = 0; i < n; i++)); do printf ' u%d' "$i"; done
        printf '\n\n'
        for ((i = 0; i < n; i++)); do
            printf 'u%d:\n' "$i"
            # `+` marks the recipe as recursive so make hands it a jobserver
            # token set; `$` is doubled so make does not eat shell expansions.
            printf '\t+@%s\n\n' "${cmds[$i]//$/$$}"
        done
    } > "$makefile"

    echo "$label: $n unit(s), jobserver pool=$pool_jobs (fifo)"
    local rc=0
    # issue 0762 — one process group for the pool, so a killed launcher does not
    # leave the units running. Passthrough when already inside a guarded tree.
    source "$repo_root/scripts/build/subtree-guard.sh"
    # issue 1403 — stall watchdog inside the guard (see make-stall-watchdog.py).
    #
    # issue 1886 — the guard is keyed on the pool's LABEL, never on "a pool".
    # The guard's lock means "this tree's <name> build is running; a second one
    # would corrupt its artifacts", so its key must name an artifact SET. One
    # constant name for every pool claimed that any two pools in a tree share
    # artifacts, which nothing measured: `just check build` fans out
    # `linux-check` (pool `check-examples`: `build/example-lint/<leaf>`, leaf
    # `Cargo.lock`s, the `nros configure` output of the board-image leaves it lints)
    # and `source-gates` (pool `compile-check`: `build/compile-check-fixtures/
    # <id>`), disjoint write sets, and whichever started second was REFUSED —
    # `ci gate` red on main, each gate green alone. Two pools with the SAME
    # label are one script over one artifact set, and still refuse: a second
    # `just linux check` in another terminal is the case this guard exists for.
    # `nros_pool_guard_name` is the one spelling, and the selftest
    # (`subtree_guard.sh`) drives `nros_pool_run` itself, not the name.
    #
    # issue 2119 — the units find the SAME make by name (see
    # `nros_pinned_tool_path_prefix`). Set on the pool's own command, not
    # exported, so the caller's shell keeps its PATH after the pool returns.
    local pinned_make
    pinned_make="$(nros_pinned_make)"
    # phase-535 W3 — join an inherited pool instead of opening a second one
    # (`nros_inherited_jobserver`); otherwise a fresh pool, as before.
    local -a make_args env_unset=()
    mapfile -t make_args < <(nros_pool_make_args "$pool_jobs")
    nros_inherited_jobserver || env_unset=(-u MAKEFLAGS -u CARGO_MAKEFLAGS)
    nros_guard_exec "$(nros_pool_guard_name "$label")" \
        python3 "$repo_root/scripts/build/make-stall-watchdog.py" \
        --label "jobserver-pool $label" --diag-dir "$work_root" -- \
        env "${env_unset[@]}" \
        PATH="$(nros_pinned_tool_path_prefix "$pinned_make"):$PATH" \
        "$pinned_make" "${make_args[@]}" \
        -f "$makefile" || rc=$?
    rm -f "$makefile"
    return $rc
}

# phase-489 W13 — ONE token pool for a whole build RUN.
#
#   nros_pool_own <label> <tokens> <command...>
#
# The outermost driver of a run (`gate-lane.sh`, the `build-test-fixtures`
# driver) runs its command under a pool it owns: pinned make 4.4 with
# `-j<tokens> --jobserver-style=fifo` and one `+` recipe, so the command and
# every descendant see `MAKEFLAGS=... --jobserver-auth=fifo:<path>` (and the
# same in `CARGO_MAKEFLAGS`). Only the FIFO form: a path survives nextest,
# `bash -c`, `env` and `setsid`; an inherited fd pair did not (191 x `cannot
# open file descriptor 3` in one nightly). `NROS_INHERIT_JOBSERVER=1` tells
# every stage below to JOIN it (`nros_inherited_jobserver`), and the pinned
# make's and ninja's dirs go first on PATH, so a child that runs `make` by name
# runs a make that can read a FIFO pool (issue 2119; a make 4.3 dies on it).
#
# Already inside a pool: the command joins it and nothing is nested. No pinned
# make 4.4: the command runs as before, and the line says the pool is OFF and
# why. The command's own exit status is returned, never make's.
nros_pool_own() {
    local label="${1:?label}" tokens="${2:?tokens}"
    shift 2
    if nros_inherited_jobserver; then
        echo "$label: token pool: joining the run's pool (${MAKEFLAGS##*--jobserver-auth=})"
        "$@"
        return
    fi
    local make_bin
    if ! make_bin="$(nros_pinned_make)" \
        || ! "$make_bin" --version 2>/dev/null | head -1 | grep -F 'GNU Make 4.4' >/dev/null; then
        echo "$label: token pool OFF: no pinned make 4.4 (neither the SDK store nor a baked copy" \
            "passes the index probe); each tool runs at its own width. Provision: nros setup --tool make"
        "$@"
        return
    fi
    local dir rc=0 cmd="" a
    dir="$(mktemp -d "${TMPDIR:-/tmp}/nros-token-pool.XXXXXX")" || { "$@"; return; }
    for a in "$@"; do cmd+=" $(printf '%q' "$a")"; done
    # `$` doubled so make leaves the shell's expansions alone. The recipe
    # records the command's status and exits 0, so make never reports a
    # failure of its own over it. `MAKELEVEL` is dropped: a make below that
    # sees it acts as a sub-make and prints `Entering directory` lines that
    # the same command run outside a pool does not.
    {
        printf '.PHONY: run\nrun:\n'
        printf '\t+@unset MAKELEVEL MFLAGS; CARGO_MAKEFLAGS="$$MAKEFLAGS" NROS_INHERIT_JOBSERVER=1 %s; echo $$? >%q\n' \
            "${cmd//\$/\$\$}" "$dir/rc"
    } >"$dir/run.mk"
    echo "$label: token pool: $tokens tokens (make 4.4 fifo jobserver, $make_bin)"
    # issue 1403 — the owner is a jobserver make, so it runs under the stall
    # watchdog like every other one. Its one child is the command, so the
    # watchdog's "no live child" condition holds only if that has exited.
    local diag="${NROS_BUILD_LOG_DIR:-$_NROS_POOL_LIB_DIR/../../build}/jobserver-pool"
    mkdir -p "$diag"
    python3 "$_NROS_POOL_LIB_DIR/make-stall-watchdog.py" \
        --label "token pool $label" --diag-dir "$diag" -- \
        env -u MAKEFLAGS -u CARGO_MAKEFLAGS -u MFLAGS -u CMAKE_BUILD_PARALLEL_LEVEL \
        PATH="$(nros_pinned_tool_path_prefix "$make_bin"):$PATH" \
        "$make_bin" -j"$tokens" --jobserver-style=fifo -f "$dir/run.mk" || rc=$?
    if [ -s "$dir/rc" ]; then
        rc="$(cat "$dir/rc")"
    elif [ "$rc" -eq 0 ]; then
        rc=1
    fi
    rm -rf "$dir"
    return "$rc"
}
