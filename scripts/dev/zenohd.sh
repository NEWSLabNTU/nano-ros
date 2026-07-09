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
    newest="$(ls -1 "${NROS_HOME:-$HOME/.nros}"/sdk/zenohd/*/bin/zenohd 2>/dev/null | sort -V | tail -1 || true)"
    if [ -n "$newest" ] && [ -x "$newest" ]; then
        printf '%s\n' "$newest"
        return 0
    fi
    if command -v zenohd >/dev/null 2>&1; then
        command -v zenohd
        return 0
    fi
    printf 'ERROR: zenohd not found. Run `just zenohd setup` (per-checkout build/zenohd/) or `nros setup native --rmw zenoh` (SDK store).\n' >&2
    return 1
}
