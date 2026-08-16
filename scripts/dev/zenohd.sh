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
# harness are expected to start the same thing.
nros_zenohd_bin() {
    local relative="lib/rmw_zenoh_cpp/rmw_zenohd"

    if [ -n "${NROS_RMW_ZENOHD:-}" ] && [ -x "$NROS_RMW_ZENOHD" ]; then
        printf '%s\n' "$NROS_RMW_ZENOHD"
        return 0
    fi
    if [ -n "${ROS_DISTRO:-}" ] && [ -x "/opt/ros/$ROS_DISTRO/$relative" ]; then
        printf '%s\n' "/opt/ros/$ROS_DISTRO/$relative"
        return 0
    fi
    # Newest distro name last, so a host with several picks one deterministically.
    local candidate
    candidate="$(ls -d /opt/ros/*/ 2>/dev/null | sort | while read -r d; do
        [ -x "$d$relative" ] && printf '%s\n' "$d$relative"
    done | tail -1)"
    if [ -n "$candidate" ]; then
        printf '%s\n' "$candidate"
        return 0
    fi

    printf 'ERROR: no `rmw_zenoh_cpp/rmw_zenohd` under /opt/ros (ROS_DISTRO=%s).\n' \
        "${ROS_DISTRO:-unset}" >&2
    printf '       The zenoh lanes run the router a ROS 2 deployment actually runs.\n' >&2
    printf '       Install `ros-<distro>-rmw-zenoh-cpp`, or set NROS_RMW_ZENOHD to its path.\n' >&2
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
nros_router_exec() {
    local locator="${1:?nros_router_exec: a locator is required}"
    local bin
    bin="$(nros_zenohd_bin)" || return 1
    ZENOH_CONFIG_OVERRIDE="listen/endpoints=[\"${locator}\"];scouting/multicast/enabled=false" \
        exec "$bin"
}
