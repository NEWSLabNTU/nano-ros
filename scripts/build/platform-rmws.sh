#!/usr/bin/env bash
#
# Which RMW backends does a PLATFORM's lane actually build?
#
# Source this and call `nros_platform_rmws <platform>`; it echoes the backend
# names, one per line, derived from `examples/fixtures.toml`.
#
# WHY THIS EXISTS. `nros setup <board>` provisions `board.packages ∪
# rmw.packages` and defaults `--rmw` to zenoh. Every platform setup recipe took
# that default, and no recipe anywhere passed `--rmw` — so the flag and the
# `[rmw.*]` rows existed and nothing used them.
#
# Six platforms build a non-zenoh coordinate (freertos, freertos-posix, linux,
# threadx-linux, threadx-riscv64, zephyr), and for those the host tools of the
# other backend were never installed. The freertos nightly died at cmake
# configure with
#
#     idlc (Cyclone DDS IDL compiler, a host tool) not found.
#
# on a lane that builds `freertos-cyclonedds-s32z270-freertos`. `[rmw.cyclonedds]`
# declares exactly what that needs — `cyclonedds` (which carries idlc),
# `cyclonedds-src`, `rosidl` — and calls itself "a complete provisioner"; the
# packages were declared and simply never requested.
#
# DERIVED, not listed. A hand-written map here would be a second copy of
# `fixtures.toml`'s `rmw =` fields and would drift the first time a row moves —
# the failure mode CLAUDE.md names for the sizes-header mirror and the fixture
# probes. `fixtures.toml` already says which coordinates a platform builds, so
# it is the only honest source for which backends that platform needs.
#
# Deliberately NOT silent on failure: a platform with no rows is a caller error
# (a typo'd name), and returning "nothing to provision" for it would reproduce
# the bug this fixes one level up.

# nros_platform_rmws <platform>
#
# Echoes one backend per line, sorted and deduped. Exit 1 with a message if the
# platform has no fixture rows at all.
nros_platform_rmws() {
    local platform="${1:?nros_platform_rmws: platform}"
    local root
    root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

    local out
    # `coords` is the same subcommand the lane filters use, so this reads the
    # manifest through the one parser rather than re-implementing TOML in awk.
    # Its fields are separated by 0x1f, NOT tab — a tab-split silently yields
    # one field and an empty answer, which looks exactly like "no rows".
    out="$(python3 "$root/scripts/build/fixtures-manifest.py" coords 2>/dev/null \
        | awk -F'\x1f' -v p="$platform" '$2 == p { print $4 }' \
        | grep -v '^$' \
        | sort -u)"

    if [ -z "$out" ]; then
        echo "nros_platform_rmws: no fixture rows for platform '$platform'" >&2
        echo "  (known platforms come from examples/fixtures.toml)" >&2
        return 1
    fi
    printf '%s\n' "$out"
}

# nros_setup_board_rmws <board> <platform>
#
# `nros setup <board> --rmw <r>` for every backend the platform's lane builds.
#
# Replaces a bare `nros setup <board>`, which takes `--rmw`'s zenoh default and
# so provisions the zenoh host tools only. The calls are cumulative and
# idempotent — each resolves `board.packages ∪ rmw.packages`, and the board half
# is the same set every time — so this costs one extra resolve per extra
# backend and installs nothing twice.
#
# Ordered by `sort -u`, which puts `cyclonedds` before `zenoh`; nothing depends
# on the order, but a stable one keeps the log diffable between runs.
nros_setup_board_rmws() {
    local board="${1:?nros_setup_board_rmws: board}"
    local platform="${2:?nros_setup_board_rmws: platform}"
    local rmws
    rmws="$(nros_platform_rmws "$platform")" || return 1
    local r
    while IFS= read -r r; do
        [ -n "$r" ] || continue
        echo "  nros setup $board --rmw $r"
        nros setup "$board" --rmw "$r" || return 1
    done <<<"$rmws"
}
