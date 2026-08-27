#!/usr/bin/env bash
# Reader for config/rust-targets.txt — the ONE list of cross targets.
#
# Issue 0833. Both the installer (`just workspace rust-targets`) and the
# verifier (`just doctor`) source this, so they cannot disagree the way they did
# for two phases. Sourced, not executed.

# nros_rust_targets [kind] — print one target per line.
#   kind: "rustup" (default) — targets with a prebuilt rust-std, installable
#         "build-std"        — Tier 3 / custom-JSON targets, nothing to install
#         "all"              — every declared target
nros_rust_targets() {
    local kind="${1:-rustup}"
    local root
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
    local list="$root/config/rust-targets.txt"
    [ -r "$list" ] || {
        echo "nros_rust_targets: missing $list" >&2
        return 2
    }
    awk -v kind="$kind" '
        /^[[:space:]]*#/ || /^[[:space:]]*$/ { next }
        { if (kind == "all" || $2 == kind) print $1 }
    ' "$list"
}
