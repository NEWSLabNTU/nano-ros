#!/usr/bin/env bash
#
# ctest launcher: run a test with its Cyclone DDS bus confined to loopback.
#
# Every `add_test` in this directory goes through `nros_rmw_cyclonedds_add_test`
# (tests/CMakeLists.txt), which runs the test as `bash dds_loopback_exec.sh
# <command...>`. Without it the suite ran on Cyclone's own interface pick: on a
# host whose first interface does not carry multicast back to itself,
# `nros_rmw_cyclonedds_session_smoke` never got past discovery and the whole
# suite hit its 2400 s gate timeout, while the same binary with the loopback
# profile passed in under a second. The Rust harness had the pin since issue
# 1009; the C++ suite was the road it did not reach.
#
# Same rules as `nros_tests::dds_isolation::apply_cyclone_config`:
#   * `NROS_DDS_ALLOW_LAN` set: nothing is pinned.
#   * an operator's own `CYCLONEDDS_URI` is kept.
#   * otherwise the committed loopback profile (via `nros_export_cyclone_config`,
#     which also applies the `NROS_E2E_LOOPBACK_*` negative-control overrides).
# Gate: `just check dds-isolation-symmetry` (no bare `add_test` here).
set -euo pipefail

if [ "$#" -eq 0 ]; then
    echo "dds_loopback_exec.sh: no command to run" >&2
    exit 2
fi

if [ -z "${NROS_DDS_ALLOW_LAN:-}" ] && [ -z "${CYCLONEDDS_URI:-}" ]; then
    # shellcheck source=packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/dds_loopback.sh
    . "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/dds_loopback.sh"
    nros_export_cyclone_config "$PWD/nros-cyclone-loopback-$$.xml" >&2
fi

exec "$@"
