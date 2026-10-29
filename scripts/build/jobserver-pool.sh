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
# The pinned make's absolute path, or empty. CONSTRUCTED via `nros sdk-path`
# (issue 0625) rather than searched, and deliberately NOT taken from PATH: a
# system make 4.3 first on PATH would answer, and its pipe-FD jobserver is the
# thing that does not work. `$repo_root` is unused now that the binary lives in
# the store rather than `third-party/make/`.
nros_pinned_make() {
    local cli dir
    cli="$(_nros_pool_cli)" || return 1
    dir="$("$cli" sdk-path make 2>/dev/null)" || return 1
    [ -n "$dir" ] && [ -x "$dir/bin/make" ] || return 1
    printf '%s\n' "$dir/bin/make"
}

# The pinned ninja's absolute path, or empty — `nros_pinned_make`'s sibling, for
# the same reason: constructed from the index pin, never searched on PATH. ninja
# >=1.13 is the half that JOINS make 4.4's fifo jobserver; the index pins 1.13.x,
# so existence is the check here and the version floor is the pin's.
nros_pinned_ninja() {
    local cli dir
    cli="$(_nros_pool_cli)" || return 1
    dir="$("$cli" sdk-path ninja 2>/dev/null)" || return 1
    [ -n "$dir" ] && [ -x "$dir/bin/ninja" ] || return 1
    printf '%s\n' "$dir/bin/ninja"
}

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
    # `Cargo.lock`s, the `nros sync` output of the board-image leaves it lints)
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
    nros_guard_exec "$(nros_pool_guard_name "$label")" \
        python3 "$repo_root/scripts/build/make-stall-watchdog.py" \
        --label "jobserver-pool $label" --diag-dir "$work_root" -- \
        env -u MAKEFLAGS -u CARGO_MAKEFLAGS \
        PATH="$(nros_pinned_tool_path_prefix "$pinned_make"):$PATH" \
        "$pinned_make" -j"$pool_jobs" --jobserver-style=fifo \
        -f "$makefile" || rc=$?
    rm -f "$makefile"
    return $rc
}
