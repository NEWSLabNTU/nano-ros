# shellcheck shell=bash
# Issue 2100 — a gate's build output, released as soon as the gate is done
# with it, on a lane that asks.
#
# `host-tests`' tier-1 gates job runs the 26 `check build` gates at once on a
# GitHub-hosted runner. Reclaim leaves about 75 G free there, and that is a
# property of the hosted image, not something we provision. The gates' own
# output summed to about 110 G, measured cold one gate at a time on
# 2026-10-27, so the runner ran out of disk on 4 of 7 nights. Three gates held
# 73 G of it, and all of it was output nothing read again once the gate had
# its verdict:
#
#   linux-check        36 G  one clippy target dir per example leaf (49)
#   cli-tests          18 G  the CLI workspace's debug test target
#   template-copy-out  13 G  the copies' shared cargo, sizes and metadata output
#
# Release is OPT-IN (`NROS_CI_PRUNE_GATE_OUTPUTS=1`). A CI lane starts cold, so
# it gives up nothing. A developer's tree keeps its incremental builds, which
# is the reason these directories persist at all.
#
# Usage: source this file, then `nros_gate_prune <dir>...`. A path outside this
# checkout is refused rather than removed: the switch says "release my own
# output", never "delete whatever I was handed".

_NROS_GATE_PRUNE_REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"

nros_gate_prune_enabled() {
    [ "${NROS_CI_PRUNE_GATE_OUTPUTS:-}" = "1" ]
}

nros_gate_prune() {
    nros_gate_prune_enabled || return 0
    local d real
    for d in "$@"; do
        [ -e "$d" ] || continue
        real="$(cd "$d" 2>/dev/null && pwd -P)" || continue
        case "$real/" in
            "$_NROS_GATE_PRUNE_REPO"/?*/) ;;
            *)
                echo "nros_gate_prune: refusing $d — not inside $_NROS_GATE_PRUNE_REPO" >&2
                continue
                ;;
        esac
        rm -rf -- "$real"
    done
}
