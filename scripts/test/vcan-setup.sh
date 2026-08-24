#!/usr/bin/env bash
# RFC-0080 / phase-377 W3 — bring up a virtual CAN interface for the CAN link
# tests, so the whole transport can be exercised with no hardware.
#
#   ./scripts/test/vcan-setup.sh            # create and bring up vcan0
#   ./scripts/test/vcan-setup.sh vcan1      # a different name
#   ./scripts/test/vcan-setup.sh --down     # tear down vcan0
#   ./scripts/test/vcan-setup.sh --status   # report without changing anything
#
# Needs root to load the module and create the link; re-runs are harmless.
set -euo pipefail

# issue 0726 — `grep -q` in a conditional cannot tell a NON-MATCH (exit 1) from
# a grep that FAILED TO RUN (exit >= 2). Under a saturated fan-out that
# difference has already produced a false, specific claim once. `nros_grep_q`
# keeps 0/1 and turns >= 2 into a fatal.
# shellcheck source=scripts/lib/grep-q.sh
source "$(dirname "${BASH_SOURCE[0]}")/../lib/grep-q.sh"

DEV="vcan0"
ACTION="up"

for arg in "$@"; do
    case "$arg" in
        --down) ACTION="down" ;;
        --status) ACTION="status" ;;
        -h | --help)
            sed -n '2,10p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        -*)
            echo "unknown option: $arg" >&2
            exit 2
            ;;
        *) DEV="$arg" ;;
    esac
done

have_dev() { ip link show "$DEV" >/dev/null 2>&1; }

# `nros_grep_q` ends a tool failure with `exit 2` — but the right-hand side of a
# PIPE runs in a subshell, so that exits only the subshell and the caller gets a
# plain non-zero it cannot tell from "no match". Every use here is a pipeline,
# so each one reads `$?` explicitly; `if ! … | nros_grep_q …` would restore the
# very conflation issue 0726 is about while leaving the gate green.
#
# The `exit 2` below DOES end the script: `is_up` is called from the main shell,
# not from a pipeline.
is_up() {
    ip -br link show "$DEV" 2>/dev/null | nros_grep_q '\b\(UP\|UNKNOWN\)\b'
    case $? in
        0) return 0 ;;
        1) return 1 ;;
        *)
            echo "[vcan-setup] could not read the link state of $DEV — refusing" >&2
            echo "             to report it DOWN on a grep that did not run" >&2
            exit 2
            ;;
    esac
}

status() {
    if ! have_dev; then
        echo "  $DEV: absent"
        return 1
    fi
    ip -br link show "$DEV" | sed 's/^/  /'
    is_up || {
        echo "  ($DEV exists but is DOWN)"
        return 1
    }
    return 0
}

# Root is needed for modprobe and `ip link add`. Re-exec under sudo rather than
# failing halfway through, but only when something actually has to change.
need_root() {
    if [ "$(id -u)" -ne 0 ]; then
        echo "[vcan-setup] needs root; re-running under sudo"
        exec sudo -- "$0" "$@"
    fi
}

case "$ACTION" in
    status)
        echo "[vcan-setup] status"
        status || exit 1
        exit 0
        ;;

    down)
        if ! have_dev; then
            echo "[vcan-setup] $DEV already absent"
            exit 0
        fi
        need_root "$@"
        ip link set down "$DEV" 2>/dev/null || true
        ip link delete "$DEV" type vcan
        echo "[vcan-setup] removed $DEV"
        exit 0
        ;;

    up)
        if have_dev && is_up; then
            echo "[vcan-setup] $DEV already up"
            status
            exit 0
        fi

        need_root "$@"

        lsmod 2>/dev/null | nros_grep_q '^vcan\b'
        case $? in
            0) ;;  # already loaded
            1)
                # Not fatal if it is built in rather than a module.
                modprobe vcan 2>/dev/null || true
                ;;
            *)
                echo "[vcan-setup] could not read the module list — refusing to" >&2
                echo "             guess whether vcan is loaded" >&2
                exit 2
                ;;
        esac
        if ! modinfo vcan >/dev/null 2>&1; then
            # `modinfo` failing means there is no such MODULE; vcan may still be
            # built INTO the kernel, and then the module list is what settles it.
            lsmod 2>/dev/null | nros_grep_q '^vcan\b'
            case $? in
                0) ;;  # built in / already inserted
                1)
                    echo "[vcan-setup] the vcan module is not available on this kernel" >&2
                    echo "             (Debian/Ubuntu: linux-modules-extra-\$(uname -r))" >&2
                    exit 1
                    ;;
                *)
                    echo "[vcan-setup] could not read the module list — refusing to" >&2
                    echo "             report vcan missing on a grep that did not run" >&2
                    exit 2
                    ;;
            esac
        fi

        have_dev || ip link add dev "$DEV" type vcan
        ip link set up "$DEV"

        echo "[vcan-setup] $DEV ready"
        status

        # candump is how you watch the wire; worth saying once rather than
        # having someone conclude the link is dead when it is just unobserved.
        if ! command -v candump >/dev/null 2>&1; then
            echo "[vcan-setup] note: can-utils is not installed — 'candump $DEV' is"
            echo "             the fastest way to see whether frames are moving."
            echo "             (Debian/Ubuntu: apt install can-utils)"
        fi
        exit 0
        ;;
esac
