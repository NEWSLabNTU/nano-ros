# shellcheck shell=bash
#
# The self-hosted runner's NAME is required, never defaulted.
#
# GitHub keys a runner registration by name, and a broker session by
# registration. Every runner script used to fall back to the same literal,
# `nano-ros-runner`, so two machines started without thinking about it claimed
# ONE identity: on 2026-10-09 the hand-over from one workstation to newslab-118
# opened with "A session for this runner already exists", and the new runner then
# sat online, idle and label-matched while no job reached it for 17 hours. A
# default that is right on the first machine is wrong on the second, and nothing
# says so — so the operator names each runner, once, on purpose.
#
#   nros_require_runner_name <script-name> [<value>]
#
# Prints the name: <value> when given (a `--name` flag), else
# `$NROS_RUNNER_NAME`. Exits 2 with the remedy when neither is set.

nros_require_runner_name() {
    local script="$1" value="${2:-${NROS_RUNNER_NAME:-}}"
    if [ -z "$value" ]; then
        {
            echo "$script: the runner needs an explicit name — set NROS_RUNNER_NAME."
            echo "  A runner's name is its registration on GitHub, so it must be unique"
            echo "  per machine (and per label set on one machine). There is no default:"
            echo "  two machines sharing one silently fight over a single registration."
            echo "  e.g.  NROS_RUNNER_NAME=nano-ros-runner-\$(hostname -s) $script …"
        } >&2
        exit 2
    fi
    printf '%s\n' "$value"
}
