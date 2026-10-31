#!/usr/bin/env bash
# Resolve and run the Micro-XRCE-DDS Agent — phase-519 W4.
#
# The zenoh router has one door: `nros_router_exec <locator>` in
# scripts/dev/zenohd.sh, run by every `just <platform>::zenohd`. The agent had
# none. `just setup xrce` provisions it and nothing started it, so every README
# that needed one typed `MicroXRCEAgent udp4 -p 8888` by hand. That restates the
# transport and port the image's domain already owns (RFC-0109 U8: the domain's
# `locator` is the owner), and a README copying a neighbour's port is how a
# device ends up dialling an agent on another one, which reads as a silent
# session-open timeout.
#
# This file is that door. Source it, then:
#
#   nros_xrce_agent_bin            echo the provisioned agent's path
#   nros_xrce_agent_args <loc>     echo the agent's argv for a nano-ros locator
#   nros_xrce_agent_exec <loc>     exec the agent on that locator
#   nros_xrce_agent_hint <loc>     echo the line that TELLS a human how
#
# `just xrce agent` is the verb over it.
#
# WHICH AGENT. The one `just setup xrce` published, at
# `$(nros_build_dir "$NROS_KIND_XRCE_AGENT")/MicroXRCEAgent`. `scripts/xrce-agent/build.sh`
# publishes there either a build against the SOURCED ROS's Fast-DDS (no version
# skew, issue 0741) or a forwarding wrapper around the `nros setup` store agent,
# so that one path already carries the preference order the test harness uses
# (`nros_tests::fixtures::xrce_agent`). PATH is NOT searched, for the reason the
# router resolver gives: PATH is where an agent nobody paired with this ROS
# accumulates, and issue 0741 measured what a Fast-CDR major skew does to a
# service reply.
#
# DDS ISOLATION. The test harness pins the agent's DDS side to loopback with a
# Fast-DDS profile (`apply_fastdds_profile`, issue 1009), and it pins the ROS
# peer the same way. This door does NOT, on purpose: a profile on one side and
# not the other is NO discovery (issue 1009 measured 0/15), and the ROS peer a
# reader starts by hand (`ros2 topic echo`) carries no profile. So the agent
# here joins DDS the way any ROS 2 node on this host does. A caller that wants
# the harness's isolation sets FASTRTPS_DEFAULT_PROFILES_FILE on both sides
# itself; it is passed through untouched.

# shellcheck source=scripts/build/build-root.sh
source "$(dirname "${BASH_SOURCE[0]}")/../build/build-root.sh"

nros_xrce_agent_bin() {
    local bin
    bin="$(nros_build_dir "$NROS_KIND_XRCE_AGENT")/MicroXRCEAgent"
    if [ -x "$bin" ]; then
        printf '%s\n' "$bin"
        return 0
    fi
    printf 'ERROR: no provisioned Micro-XRCE-DDS Agent at %s.\n' "$bin" >&2
    printf '       Run `just setup xrce`: it provisions the agent and publishes it there,\n' >&2
    printf '       built against your sourced ROS when one is sourced (issue 0741).\n' >&2
    printf '       PATH is not searched: an agent nobody paired with this ROS can\n' >&2
    printf '       mis-frame a service reply.\n' >&2
    return 1
}

# Translate a nano-ros locator into the agent's own argv.
#
#   udp/<host>:<port>             -> udp4 -p <port>   (udp6 for a [v6] host)
#   tcp/<host>:<port>             -> tcp4 -p <port>   (tcp6 for a [v6] host)
#   <host>:<port>                 -> udp4 -p <port>   (the XRCE client's own
#                                    NROS_LOCATOR form, which is UDP)
#   serial/<dev>[#baudrate=<n>]   -> serial -D <dev> -b <n>   (default 115200)
#
# The HOST is where the device dials, not a bind address: the agent has no bind
# option and listens on every interface of the family, so the host is checked
# for shape and otherwise dropped.
#
# A serial locator in a `system.toml` names the DEVICE's UART (`UART_0`), which
# is not a file on this host. Such a locator is refused unless
# NROS_XRCE_AGENT_DEV names the host end (a tty, or a PTY a QEMU/socat pair
# made). An absolute device path in the locator is used as written.
nros_xrce_agent_args() {
    local locator="${1:?nros_xrce_agent_args: a locator is required}"
    local scheme rest
    case "$locator" in
        udp/*) scheme=udp; rest="${locator#udp/}" ;;
        tcp/*) scheme=tcp; rest="${locator#tcp/}" ;;
        serial/*) scheme=serial; rest="${locator#serial/}" ;;
        */*)
            printf 'ERROR: the agent cannot serve locator `%s`.\n' "$locator" >&2
            printf '       It speaks udp/, tcp/ and serial/ (`custom://` is the\n' >&2
            printf '       image'"'"'s own transport, which no host agent reaches).\n' >&2
            return 2 ;;
        *) scheme=udp; rest="$locator" ;;
    esac

    if [ "$scheme" = serial ]; then
        local dev="${rest%%#*}" baud=115200
        case "$rest" in
            *'#baudrate='*) baud="${rest##*#baudrate=}"; baud="${baud%%[#&]*}" ;;
        esac
        case "$baud" in
            ''|*[!0-9]*)
                printf 'ERROR: locator `%s`: baudrate `%s` is not a number.\n' "$locator" "$baud" >&2
                return 2 ;;
        esac
        if [ -n "${NROS_XRCE_AGENT_DEV:-}" ]; then
            dev="$NROS_XRCE_AGENT_DEV"
        elif [ "${dev#/}" = "$dev" ]; then
            printf 'ERROR: locator `%s` names the DEVICE'"'"'s UART (`%s`), not a host file.\n' "$locator" "$dev" >&2
            printf '       Set NROS_XRCE_AGENT_DEV to the host end of that link\n' >&2
            printf '       (e.g. /dev/ttyACM0, or the PTY a QEMU or socat pair printed).\n' >&2
            return 2
        fi
        printf '%s\n' serial -D "$dev" -b "$baud"
        return 0
    fi

    local host port family=4
    case "$rest" in
        \[*\]:*) host="${rest%%]:*}"; host="${host#[}"; port="${rest##*]:}"; family=6 ;;
        *:*:*)
            printf 'ERROR: locator `%s`: write an IPv6 host in brackets ([::1]:8888).\n' "$locator" >&2
            return 2 ;;
        *:*) host="${rest%:*}"; port="${rest##*:}" ;;
        *)
            printf 'ERROR: locator `%s` has no port; write <host>:<port>.\n' "$locator" >&2
            return 2 ;;
    esac
    case "$port" in
        ''|*[!0-9]*)
            printf 'ERROR: locator `%s`: port `%s` is not a number.\n' "$locator" "$port" >&2
            return 2 ;;
    esac
    if [ -z "$host" ]; then
        printf 'ERROR: locator `%s` has an empty host.\n' "$locator" >&2
        return 2
    fi
    printf '%s\n' "${scheme}${family}" -p "$port"
}

# Run the provisioned agent on `$1`. Extra arguments go to the agent after the
# transport's own (e.g. `-v 6` for its verbose log).
nros_xrce_agent_exec() {
    local locator="${1:?nros_xrce_agent_exec: a locator is required}"
    shift
    local -a args
    local out
    out="$(nros_xrce_agent_args "$locator")" || return 2
    mapfile -t args <<< "$out"
    local bin
    bin="$(nros_xrce_agent_bin)" || return 1
    echo "xrce-agent: $bin ${args[*]} $* (locator $locator)" >&2
    exec "$bin" "${args[@]}" "$@"
}

# The canonical "how to start the agent" line, as TEXT, for a caller that can
# only tell a human (the router's `nros_router_hint`, issue 0654).
nros_xrce_agent_hint() {
    local locator="${1:-udp/127.0.0.1:8888}"
    printf 'just xrce agent %s\n' "$locator"
}

# Run directly: the same as `just xrce agent <locator>`.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    set -euo pipefail
    nros_xrce_agent_exec "$@"
fi
