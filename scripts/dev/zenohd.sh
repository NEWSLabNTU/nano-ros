#!/usr/bin/env bash
# Resolve and run the zenoh router — the one ROS ships (phase-362 / RFC-0075).
#
# The router is `rmw_zenoh_cpp/rmw_zenohd`, not a router of ours. It links the
# same `libzenohc.so` that `rmw_zenoh_cpp` does, so it cannot drift from the RMW
# a ROS 2 node is actually using, and it is what a ROS 2 deployment runs. The
# vendored `zenohd` this file used to resolve could drift, and did: issue 0609
# measured `ros-humble-rmw-zenoh-cpp` 0.1.1 -> 0.1.9 moving its vendored zenoh
# 1.2.0 -> 1.8.0, with our pin taking no part in the failure or the fix.
#
# `nros_zenohd_bin` echoes the resolved binary. `nros_router_exec <locator>`
# runs it on that locator — use the latter unless you have a reason not to,
# because the configuration does not travel on the command line (see below).

# Locate the ROS-shipped router. Mirrors `nros_tests::process::ros_zenohd_path`;
# the two must agree, since a contributor's `just <plat> zenohd` and the test
# harness are expected to start the same thing. `check-zenohd-resolution-parity`
# parses both and enforces it — the sentence above was here on its own for a
# phase, and the two drifted anyway.
#
# Order (issue 0653), most explicit first:
#   1. NROS_RMW_ZENOHD        an explicit path, for a non-standard install
#   2. AMENT_PREFIX_PATH      every prefix the caller has SOURCED
#   3. $ROS_DISTRO            under /opt/ros
#   4. /opt/ros/*             newest distro name last
#
# Step 2 is what makes "source setup.bash, then run the lanes" true, and
# `/opt/ros` alone did not: a ROS install need not be there. This repo documents
# building one on Arch/Fedora/NixOS (docs/development/ros2-on-non-ubuntu.md), and
# a colcon overlay is a prefix wherever the user chose. AMENT_PREFIX_PATH is the
# sourced environment's own answer to which prefixes are active, so neither case
# needs this code to guess a layout.
#
# PATH IS DELIBERATELY NOT SEARCHED. It was, briefly, on the reasoning that a
# caller who put the router there expects it found — and that is the wrong trade
# for THIS binary. RFC-0075 exists so the router is the one paired with the
# `rmw_zenoh_cpp` a ROS node is using, and PATH is exactly where an unrelated
# router accumulates. On the machine this was written:
#
#     $ command -v zenohd
#     ~/.nros/sdk/zenohd/1.7.2-nros2/bin/zenohd    # retired, zenoh 1.7.2
#     $ …/opt/zenoh_cpp_vendor/include/zenoh_configure.h
#     #define ZENOH_C "1.8.0"                            # what ROS actually ships
#
# A user following zenoh's own install instructions gets a third. Letting any of
# them shadow the paired one reintroduces the drift issue 0609 measured, with no
# version to point at. NROS_RMW_ZENOHD covers the deliberate case.
#
# Note the search never looks for the NAME `zenohd` either: that is the retired
# vendored router, and the store above shows one still installed.
nros_zenohd_bin() {
    local relative="lib/rmw_zenoh_cpp/rmw_zenohd"
    # The conventional root, overridable ONLY so the parity gate can drive steps
    # 4 and 5 over a synthetic tree. Untouched, this is `/opt/ros`, and no
    # non-test caller sets it — the alternative is a gate that checks the two
    # cheap steps and leaves the two legacy ones unwatched on both sides.
    local opt_ros="${NROS_ZENOHD_OPT_ROS:-/opt/ros}"

    if [ -n "${NROS_RMW_ZENOHD:-}" ] && [ -x "$NROS_RMW_ZENOHD" ]; then
        printf '%s\n' "$NROS_RMW_ZENOHD"
        return 0
    fi
    # 2 — the sourced prefixes, in ament's own precedence order.
    #
    # Split with the shell's own field splitting rather than `tr`: this function
    # must not depend on anything outside bash. A resolver that shells out is a
    # resolver that fails differently depending on PATH, and PATH is one of the
    # things it is resolving over — the parity gate caught exactly that, both
    # here and in step 5 below.
    if [ -n "${AMENT_PREFIX_PATH:-}" ]; then
        local prefix
        local -a prefixes
        IFS=':' read -r -a prefixes <<< "$AMENT_PREFIX_PATH"
        for prefix in "${prefixes[@]}"; do
            [ -n "$prefix" ] || continue
            if [ -x "$prefix/$relative" ]; then
                printf '%s\n' "$prefix/$relative"
                return 0
            fi
        done
    fi
    if [ -n "${ROS_DISTRO:-}" ] && [ -x "$opt_ros/$ROS_DISTRO/$relative" ]; then
        printf '%s\n' "$opt_ros/$ROS_DISTRO/$relative"
        return 0
    fi
    # Newest distro name last, so a host with several picks one deterministically.
    #
    # A glob rather than `ls | sort | tail`, for the builtins-only reason above,
    # and under `LC_ALL=C` because a glob expands in COLLATION order and the
    # locale decides that — issue 0485 is a counter split across locales by
    # exactly this. Distro names are ASCII, so C order is the intended one.
    local candidate="" d
    local saved_lc="${LC_ALL-}"
    LC_ALL=C
    for d in "$opt_ros"/*/; do
        [ -x "$d$relative" ] && candidate="$d$relative"
    done
    if [ -n "$saved_lc" ]; then LC_ALL="$saved_lc"; else unset LC_ALL; fi
    if [ -n "$candidate" ]; then
        printf '%s\n' "$candidate"
        return 0
    fi

    printf 'ERROR: cannot locate `rmw_zenoh_cpp/rmw_zenohd`.\n' >&2
    printf '       Looked in AMENT_PREFIX_PATH=%s and under %s (ROS_DISTRO=%s).\n' \
        "${AMENT_PREFIX_PATH:-unset}" "$opt_ros" "${ROS_DISTRO:-unset}" >&2
    printf '       PATH is not searched: the router must be the one PAIRED with your\n' >&2
    printf '       rmw_zenoh_cpp, and a `zenohd` on PATH is usually neither (RFC-0075).\n' >&2
    printf '       The zenoh lanes run the router a ROS 2 deployment actually runs.\n' >&2
    printf '       Source your ROS setup (`source /opt/ros/<distro>/setup.bash`), install\n' >&2
    printf '       `ros-<distro>-rmw-zenoh-cpp`, or set NROS_RMW_ZENOHD to its path.\n' >&2
    printf '       No ROS on this host? `--rmw cyclonedds` needs no router at all.\n' >&2
    return 1
}

# Run the router on `$1` (a zenoh locator, e.g. `tcp/127.0.0.1:7447`).
#
# `rmw_zenohd` takes NO command-line configuration: it ignores its argv (a
# `--help` starts a router) and reads `ZENOH_CONFIG_OVERRIDE` /
# `ZENOH_ROUTER_CONFIG_URI` instead. So the `--listen` / `--no-multicast-scouting`
# these recipes used to pass become override entries — `;`-separated, `=` where
# the CLI used `:`.
#
# Multicast scouting is stated explicitly even though the ROS default config
# already disables it: it is the one property these lanes depend on, and a
# default is a thing that can change under us.
# Warn when `$1` is not the router `rmw_zenoh_cpp` ships (issue 0653).
#
# Only NROS_RMW_ZENOHD can reach here un-paired — the search steps look inside
# ament prefixes and cannot produce anything else. But that override is exactly
# where a user points at a `zenohd` built from zenoh's own instructions, whose
# version has no relation to the `rmw_zenoh_cpp` the ROS side runs. A warning,
# not an error: the override is deliberate. What it must not be is silent.
nros_zenohd_warn_if_unpaired() {
    local bin="$1" dir prefix
    dir="$(dirname "$bin")"
    if [ "$(basename "$dir")" = "rmw_zenoh_cpp" ] \
       && [ "$(basename "$(dirname "$dir")")" = "lib" ]; then
        prefix="$(dirname "$(dirname "$dir")")"
        [ -f "$prefix/opt/zenoh_cpp_vendor/include/zenoh_configure.h" ] && return 0
        printf 'WARNING: %s has the ROS layout but its prefix ships no\n' "$bin" >&2
        printf '         zenoh_cpp_vendor header, so the pairing cannot be confirmed.\n' >&2
        return 0
    fi
    printf 'WARNING: %s is NOT a ROS-shipped router — it is not\n' "$bin" >&2
    printf '         <prefix>/lib/rmw_zenoh_cpp/rmw_zenohd, so it is paired with no\n' >&2
    printf '         rmw_zenoh_cpp. Results say nothing about the paired\n' >&2
    printf '         configuration (RFC-0075).\n' >&2
}

nros_router_exec() {
    local locator="${1:?nros_router_exec: a locator is required}"
    local bin
    bin="$(nros_zenohd_bin)" || return 1
    nros_zenohd_warn_if_unpaired "$bin"
    ZENOH_CONFIG_OVERRIDE="listen/endpoints=[\"${locator}\"];scouting/multicast/enabled=false" \
        exec "$bin"
}
