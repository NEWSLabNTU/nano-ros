#!/usr/bin/env bash
# Phase 104.A.4 — API decoupling guard.
#
# Asserts that the `nros` umbrella crate and the `nros-node` runtime
# crate consume only the generic RMW + platform ABIs:
#   * `nros-rmw-cffi` (the vtable shim)
#   * `nros-platform-cffi` (the C-header platform ABI)
#
# Either crate carrying a Rust-level dependency on a concrete RMW
# (`nros-rmw-zenoh`, `nros-rmw-xrce-cffi`,
# `nros-rmw-cyclonedds`) or a concrete platform
# (`nros-platform-{posix,freertos,nuttx,threadx,zephyr,esp-idf}`)
# means backend / platform selection has leaked into the umbrella's
# Cargo graph and Thread A of Phase 104 has regressed.
#
# Today this script is EXPECTED TO FAIL — Phase 104.A.1 + A.2 are
# the migrations that bring it to green. The guard is wired in
# advance so the target state is documented and CI prevents
# regression once the migration lands.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# Manifest-level patterns the guard rejects. Any reference whatsoever
# — `[dependencies]` line, `dep:<name>` feature directive,
# `<name>?/feature` forwarding — means the umbrella has Cargo
# knowledge of a concrete backend or platform.
DEP_LINE_RE='^nros-rmw-(zenoh|dds|xrce-cffi|cyclonedds)\s*=|^nros-platform-(posix|freertos|nuttx|threadx|zephyr|esp-idf|posix-c)\s*='
FEATURE_RE='dep:nros-rmw-(zenoh|dds|xrce-cffi|cyclonedds)|dep:nros-platform-(posix|freertos|nuttx|threadx|zephyr|esp-idf|posix-c)|nros-rmw-(zenoh|dds|xrce-cffi|cyclonedds)\?\/|nros-platform-(posix|freertos|nuttx|threadx|zephyr|esp-idf|posix-c)\?\/'

check_manifest() {
    local crate=$1
    local manifest="packages/core/$crate/Cargo.toml"

    if [[ ! -f "$manifest" ]]; then
        echo "FAIL: $manifest not found"
        return 1
    fi

    # Strip comment lines so the regex doesn't catch prose explaining
    # the migration. We grep with `-v` first on `^\s*#` then on the
    # actual pattern.
    local non_comment
    non_comment=$(grep -nv '^[[:space:]]*#' "$manifest")

    local dep_leaks feat_leaks
    dep_leaks=$(echo "$non_comment" | grep -E ":${DEP_LINE_RE#^}" || true)
    feat_leaks=$(echo "$non_comment" | grep -E "$FEATURE_RE" || true)

    if [[ -n "$dep_leaks" || -n "$feat_leaks" ]]; then
        echo "FAIL: $crate Cargo.toml carries concrete backend / platform refs:"
        [[ -n "$dep_leaks" ]] && {
            echo "  [dependencies]:"
            echo "$dep_leaks" | sed 's/^/    /'
        }
        [[ -n "$feat_leaks" ]] && {
            echo "  [features] (dep: / ?/ forwarding):"
            echo "$feat_leaks" | sed 's/^/    /'
        }
        return 1
    fi

    echo "OK:   $crate Cargo.toml clean of concrete RMW / platform refs"
    return 0
}

fail=0

check_manifest "nros"      || fail=1
check_manifest "nros-node" || fail=1

if [[ "$fail" -ne 0 ]]; then
    echo
    echo "Phase 104.A.1 + A.2 not yet complete. Track in"
    echo "docs/roadmap/phase-104-multi-backend-bridges.md."
    exit 1
fi

echo
echo "Phase 104.A decoupling guard PASSED."
