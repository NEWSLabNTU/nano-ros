#!/usr/bin/env bash
# The interface packages a FIXTURE build generates messages from — issue 2344.
#
# Sourced, not executed. Every fixture builder calls `nros_fixture_interfaces`
# once, before it configures anything.
#
# Since issue 2278 the CMake road refuses to generate a message package from
# nano-ros's bundled copies unless the build names them: a package on neither
# NROS_INTERFACE_SEARCH_PATH nor AMENT_PREFIX_PATH is a FATAL_ERROR whose
# remedy is "source ROS 2, or put the bundled root on the search path". That is
# the right rule for a user's build (a bundled copy silently picks a distro).
# A fixture build on a host with no ROS discoverable — the self-hosted tier-2
# runner, whose image deliberately leaves AMENT_PREFIX_PATH unset
# (`scripts/ci/runner-container.sh`), or a developer host without ROS — is
# where nano-ros DOES mean the bundled set: the fixtures are its own, and
# their messages have always come from it there.
#
# So the fixture build makes that choice once, out loud, with the refusal's own
# remedy: when no ROS is discoverable (AMENT_PREFIX_PATH unset or empty) and
# the caller set no NROS_INTERFACE_SEARCH_PATH, it exports the bundled root as
# the search path and says so. A host WITH ROS is untouched: its ament packages
# keep winning, exactly as for a user.
#
# Gate: `check-fixture-interfaces-opt-in` (every fixture builder calls this).

nros_fixture_interfaces() {
    local root="${1:-${NROS_REPO_ROOT:-$PWD}}"
    local bundled="$root/packages/cli/interfaces"
    if [ -n "${AMENT_PREFIX_PATH:-}" ] || [ -n "${NROS_INTERFACE_SEARCH_PATH:-}" ]; then
        return 0
    fi
    if [ ! -d "$bundled" ]; then
        echo "fixtures: no ROS 2 discoverable and no bundled interfaces at $bundled" >&2
        return 0
    fi
    export NROS_INTERFACE_SEARCH_PATH="$bundled"
    echo "fixtures: no ROS 2 discoverable (AMENT_PREFIX_PATH unset); messages come from the bundled interfaces, NROS_INTERFACE_SEARCH_PATH=$bundled (issue 2344)" >&2
}
