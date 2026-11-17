#!/usr/bin/env bash
# Issue 2663: a runner's host label comes from ONE operator setting,
# `NROS_RUNNER_HOST`, and the doctor refuses a label that disagrees with it.
# The heavy jobs' role, `nros-heavy`, is refused without the labels those jobs
# also request, and beside `nros-queue`.
#
# Both halves, both directions:
#   - `nros_runner_with_host_label` adds `nros-host-<id>` once, leaves the set
#     alone when the variable is unset, and refuses an id that is not a plain
#     token;
#   - `_nros_runner_check_host` accepts the derived label and refuses a label
#     with no setting, a label that names another host, and two host labels.
# Pure shell, no network, no docker, ~0.1 s.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/runner-host-label.sh
. "$repo_root/scripts/lib/runner-host-label.sh"
# Sourced, not run: the file's main is guarded by BASH_SOURCE, so this defines
# the check functions without probing the host.
# shellcheck source=scripts/ci/runner-doctor.sh
. "$repo_root/scripts/ci/runner-doctor.sh"

fail=0
expect() {  # expect <what> <want> <got>
    if [ "$2" = "$3" ]; then
        echo "  ok    $1"
    else
        echo "  FAIL  $1: want [$2], got [$3]" >&2
        fail=1
    fi
}

# --- the helper ---------------------------------------------------------------
expect "unset leaves the set alone" "nros-qemu,nros-big" \
    "$(unset NROS_RUNNER_HOST; nros_runner_with_host_label "nros-qemu,nros-big")"
expect "set adds the host label" "nros-qemu,nros-big,nros-host-118" \
    "$(NROS_RUNNER_HOST=118 nros_runner_with_host_label "nros-qemu,nros-big")"
expect "space-separated input is normalised" "nros-qemu,nros-big,nros-host-118" \
    "$(NROS_RUNNER_HOST=118 nros_runner_with_host_label "nros-qemu nros-big")"
expect "an existing host label is not doubled" "nros-host-241,nros-queue" \
    "$(NROS_RUNNER_HOST=241 nros_runner_with_host_label "nros-host-241,nros-queue")"
for bad in "Host241" "241 " "-241" "241-" "a/b"; do
    rc=0
    NROS_RUNNER_HOST="$bad" nros_runner_with_host_label "nros-qemu" >/dev/null 2>&1 || rc=$?
    expect "refuses NROS_RUNNER_HOST='$bad'" 2 "$rc"
done

# --- the doctor ---------------------------------------------------------------
check() {  # check <labels> <label to verify>; prints the status
    local rc=0
    _NROS_RUNNER_DOCTOR_LABELS="$(nros_runner_labels_split "$1" | tr '\n' ' ')" \
        NROS_RUNNER_QUIET=1 nros_runner_check_label "$2" >/dev/null 2>&1 || rc=$?
    echo "$rc"
}
expect "the derived label holds" 0 \
    "$(NROS_RUNNER_HOST=118 check "nros-qemu,nros-host-118" nros-host-118)"
expect "a host label with no NROS_RUNNER_HOST is refused" 1 \
    "$(unset NROS_RUNNER_HOST; check "nros-qemu,nros-host-118" nros-host-118)"
expect "a host label naming another host is refused" 1 \
    "$(NROS_RUNNER_HOST=241 check "nros-qemu,nros-host-118" nros-host-118)"
expect "two host labels on one runner are refused" 1 \
    "$(NROS_RUNNER_HOST=118 check "nros-host-118,nros-host-241" nros-host-118)"
expect "the bare prefix is not a host label" 1 \
    "$(NROS_RUNNER_HOST=118 check "nros-host-" nros-host-)"

# --- the heavy role -------------------------------------------------------------
heavy="nros-qemu,nros-sdk-zephyr,nros-big,nros-heavy"
expect "a full heavy set holds" 0 "$(check "$heavy" nros-heavy)"
expect "nros-heavy without nros-big is refused" 1 \
    "$(check "nros-qemu,nros-sdk-zephyr,nros-heavy" nros-heavy)"
expect "nros-heavy without nros-qemu is refused" 1 \
    "$(check "nros-sdk-zephyr,nros-big,nros-heavy" nros-heavy)"
expect "nros-heavy beside nros-queue is refused" 1 \
    "$(check "$heavy,nros-queue" nros-heavy)"

if [ "$fail" -ne 0 ]; then
    echo "runner-host-label: FAILED" >&2
    exit 1
fi
echo "runner-host-label: OK"
