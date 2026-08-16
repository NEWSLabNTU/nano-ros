#!/usr/bin/env bash
# Resolve the provisioned zenohd router binary (issue #168).
#
# zenohd lands in two places depending on the setup route:
#   - `just zenohd setup` (contributor route; what the test harness reads) →
#     build/zenohd/zenohd — deliberately OFF the global PATH, like build/qemu.
#   - `nros setup native --rmw zenoh` (user route, README Quick Start) →
#     ~/.nros/sdk/zenohd/<version>/bin/zenohd — also kept off PATH by
#     activate.sh (it only exports cross-gcc/genromfs/sccache store dirs).
#
# Recipes must therefore never invoke bare `zenohd`. `nros_zenohd_bin` echoes
# the resolved binary — per-checkout build/ pin first, then the newest SDK
# store install, then a PATH zenohd — or errors with both setup hints.
nros_zenohd_bin() {
    local root newest
    root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
    if [ -x "$root/build/zenohd/zenohd" ]; then
        printf '%s\n' "$root/build/zenohd/zenohd"
        return 0
    fi
    # phase-365 W3b — the PINNED zenohd, not the newest installed.
    #
    # This was `ls .../sdk/zenohd/*/bin/zenohd | sort -V | tail -1`: the same
    # newest-wins search that gave Corrosion the wrong version 155 times in one
    # configure (issue 0625). The store is shared between checkouts while the
    # pin is per-project, so "newest installed" answers a question this project
    # did not ask — and zenohd is version-sensitive (rmw_zenoh_cpp compat pins
    # it to 1.7.2, per CLAUDE.md).
    pinned="$(nros sdk-path zenohd 2>/dev/null || true)"
    if [ -n "$pinned" ] && [ -x "$pinned/bin/zenohd" ]; then
        printf '%s\n' "$pinned/bin/zenohd"
        return 0
    fi
    if command -v zenohd >/dev/null 2>&1; then
        command -v zenohd
        return 0
    fi
    printf 'ERROR: zenohd not found. Run `just zenohd setup` (per-checkout build/zenohd/) or `nros setup native --rmw zenoh` (SDK store).\n' >&2
    return 1
}
