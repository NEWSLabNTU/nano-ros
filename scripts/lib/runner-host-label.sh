# shellcheck shell=bash
#
# The runner's HOST label, derived from one operator setting.
#
#   nros_runner_with_host_label <labels>
#
# Prints <labels> (comma separated) with `nros-host-$NROS_RUNNER_HOST` added
# when `NROS_RUNNER_HOST` is set and the label is not already there. With
# `NROS_RUNNER_HOST` unset it prints <labels> unchanged. It exits 2 when
# `NROS_RUNNER_HOST` is not a plain lowercase token, so a typo cannot become a
# label.
#
# Why a host label exists at all, when every other label here names a
# CAPABILITY or a ROLE ("Labels, not hostnames" in
# docs/development/multi-agent-ci-workflow.md): the user ruled it for issue
# 2663 on 2026-11-17. It says which machine a runner is on, so a like-for-like
# comparison can dispatch the same job to two named hosts, and so a job's
# runner can be read back as a host. No workflow pins a job to it: the heavy
# scheduled jobs request the ROLE `nros-heavy` instead, so losing or adding a
# host is a label change rather than a workflow edit.
#
# The operator sets ONE thing, `NROS_RUNNER_HOST=<id>` (for example `118` or
# `241`), and both front doors (runner-container.sh and runner-register.sh)
# derive the label through this function, so the label and the variable cannot
# disagree. `runner-doctor.sh` checks the same pairing from the other side.

nros_runner_with_host_label() {
    local labels="${1:-}" host="${NROS_RUNNER_HOST:-}" want
    labels="${labels// /,}"
    if [ -z "$host" ]; then
        printf '%s\n' "$labels"
        return 0
    fi
    case "$host" in
        *[!a-z0-9-]* | -* | *-)
            echo "runner: NROS_RUNNER_HOST='$host' is not a plain lowercase token ([a-z0-9-], e.g. 118)" >&2
            return 2
            ;;
    esac
    want="nros-host-$host"
    case ",$labels," in
        *",$want,"*) printf '%s\n' "$labels" ;;
        *) if [ -n "$labels" ]; then printf '%s,%s\n' "$labels" "$want"; else printf '%s\n' "$want"; fi ;;
    esac
}
