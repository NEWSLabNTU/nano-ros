#!/usr/bin/env bash
# The umbrella crates select no PLATFORM. (Phase 104.A.4, rewritten phase-444
# W4.b — issue 1219 asked for "retire or rewrite", and the measurement said
# rewrite.)
#
# `nros` (the umbrella) and `nros-node` (the runtime) must reach the platform
# only through the generic ABI — `nros-platform-api` / `nros-platform-cffi` —
# never through a Cargo dependency, a `dep:` directive or `?/` forwarding on a
# concrete `nros-platform-{posix,freertos,nuttx,threadx,zephyr,esp-idf,posix-c}`.
# Choosing the platform is the outer build system's job (the board crate and the
# generated selection facade); a concrete platform in these two graphs means a
# consumer picking a board also picks these crates' idea of one.
#
# WHY THIS FILE STOPPED SAYING "EXPECTED TO FAIL"
#
# Its header claimed, from 2026-06-09, that RFC-0031 had reversed the goal, that
# the guard was EXPECTED TO FAIL, and that it was un-wired from `just check` for
# that reason. Measured on 2026-09-11, on the tree it describes: it PASSES, both
# manifests clean. So what was actually shipping was an un-wired gate that
# nobody could read a verdict from — the "red means nothing" state issue 0952
# and the 1219 write-up both name.
#
# RFC-0031 restored the `?/` forwarding and the optional backend deps for the
# RMW axis, and that half of the old rule is genuinely gone. It is also covered
# now, from the other direction: `check-rmw-agnostic` (RFC-0071, issue 1219)
# reads `packages/api/nros/Cargo.toml` and `packages/core/nros-node/Cargo.toml`
# among 13 files, refuses any NEW backend name in either, and holds the existing
# ones to a per-file count. The PLATFORM half has no such cover and never had
# another gate — which is why this one is kept, narrowed to the half that is
# still a live rule, and wired into `just check` rather than left advisory.
#
# Buildless: two `grep`s over two manifests. Self-test on the normal path.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

PLATFORMS='posix|freertos|nuttx|threadx|zephyr|esp-idf|posix-c'
# A dependency line, a `dep:` feature directive, or `?/` forwarding — any Cargo
# knowledge of a concrete platform crate.
DEP_LINE_RE="^nros-platform-($PLATFORMS)[[:space:]]*="
FEATURE_RE="dep:nros-platform-($PLATFORMS)|nros-platform-($PLATFORMS)\?/"

# `$1` = manifest text on stdin's behalf. Echoes the offending lines; returns 1
# when there are any. Split out from the file reader so the self-test can feed
# it planted manifests — a gate whose matcher can only be exercised by editing
# the tree is a gate nobody exercises.
scan_text() {
    local text=$1 non_comment leaks
    non_comment=$(printf '%s\n' "$text" | grep -nv '^[[:space:]]*#' || true)
    leaks=$(printf '%s\n' "$non_comment" | grep -E "(:${DEP_LINE_RE#^})|(${FEATURE_RE})" || true)
    if [[ -n "$leaks" ]]; then
        printf '%s\n' "$leaks"
        return 1
    fi
    return 0
}

self_test() {
    local bad=0 out rc
    # Clean: the generic ABI, and an RMW name (that axis belongs to
    # `check-rmw-agnostic`, so this must NOT fire on it).
    rc=0
    out=$(scan_text 'nros-platform-api = { path = "../x" }
nros-platform-cffi = { path = "../y" }
rmw-zenoh = ["dep:nros-rmw-zenoh"]
# nros-platform-posix = { path = "../z" }   a comment, not a dep') || rc=$?
    if [[ $rc -ne 0 ]]; then
        echo "SELFTEST: a clean manifest was reported as leaking:" >&2
        printf '%s\n' "$out" >&2
        bad=1
    fi
    # Each of the three shapes must fire.
    local probe
    for probe in 'nros-platform-posix = { path = "../p" }' \
                 'platform-zephyr = ["dep:nros-platform-zephyr"]' \
                 'std = ["nros-platform-freertos?/std"]'; do
        rc=0
        out=$(scan_text "$probe") || rc=$?
        if [[ $rc -eq 0 ]]; then
            echo "SELFTEST: a concrete platform ref was NOT caught: $probe" >&2
            bad=1
        fi
    done
    [[ $bad -eq 0 ]] || exit 2
}

self_test

check_manifest() {
    local crate=$1
    # Resolve the LAYER rather than hardcoding it. `nros` moved from
    # `packages/core/` to `packages/api/`, so this printed
    #
    #     FAIL: packages/core/nros/Cargo.toml not found
    #
    # on every run: a FAILING verdict whose stated reason was false, about a
    # crate that was never checked at all.
    #
    # A glob keeps the answer right across the next move too. `head -1` because
    # two crates of one name in different layers is itself a defect, and this
    # guard is not the place to discover it.
    local manifest
    # `|| true` — an unmatched glob is `ls` exiting 2, which is the very case
    # the `[[ -z ]]` below reports; without it the function dies here silently
    # (issue 1249).
    manifest=$(ls -d packages/*/"$crate"/Cargo.toml 2>/dev/null | head -1 || true)

    if [[ -z "$manifest" || ! -f "$manifest" ]]; then
        echo "FAIL: no packages/*/$crate/Cargo.toml found"
        return 1
    fi

    local leaks rc=0
    leaks=$(scan_text "$(cat "$manifest")") || rc=$?
    if [[ $rc -ne 0 ]]; then
        echo "FAIL: $manifest names a concrete platform crate:"
        printf '%s\n' "$leaks" | sed 's/^/    /'
        return 1
    fi

    echo "OK:   $crate Cargo.toml selects no concrete platform"
    return 0
}

fail=0

check_manifest "nros"      || fail=1
check_manifest "nros-node" || fail=1

if [[ "$fail" -ne 0 ]]; then
    echo
    echo "These two crates consume the platform through nros-platform-api /"
    echo "nros-platform-cffi only; the board crate and the generated selection"
    echo "facade choose the port. (The RMW axis is check-rmw-agnostic's.)"
    exit 1
fi

echo
echo "decoupling guard PASSED (platform axis; RMW axis -> check-rmw-agnostic)."
