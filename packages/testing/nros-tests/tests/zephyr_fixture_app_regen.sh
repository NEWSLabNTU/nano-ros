#!/usr/bin/env bash
# Issue 1707 — a declaration change reaches an already-configured Zephyr image
# in ONE build, and an unchanged one does not reconfigure it.
#
# The generated west application (`build/<coord>/<image>_entry/CMakeLists.txt`)
# carries the image's capability Kconfig and `NANO_ROS_FEATURES`, and only stage
# 4 of a `plan_builds` writes it. Two edges were missing:
#
#   1. `zephyr-fixture-run-one.sh`'s NINJA path never regenerated it, so the
#      file moved only when something else planned the workspace — and that
#      something else was the `nros image-facts` query a configure runs.
#   2. That query runs INSIDE the configure, after cmake has read the file. The
#      build system is written after the rewrite, newer than its input, so no
#      later ninja reconfigured. Measured on `features`' `zephyr_rust_qos`
#      (ninja 1.10): `.config` kept `CONFIG_NROS_CAPABILITY_PARAM_SERVICES=y`
#      through `cmake` + `ninja` while the application said `n`.
#
# A real Zephyr configure costs minutes and is forbidden in a test, so this
# asserts each edge at its seam, with stubs where a toolchain would be:
#
#   R1  ninja path, retargeted row: the runner runs `nros build <image> ...
#       --dry-run` BEFORE `ninja` (edge 1).
#   R2  negative control: a row with no image runs ninja alone, and the west
#       path never runs the extra regeneration (it runs `nros build` already).
#   C1  the configure-side check refuses when the digest the configure EXECUTED
#       differs from the one on disk (edge 2) ...
#   C2  ... and is silent when they agree — the unchanged-rebuild control, so
#       C1's red is evidence the check discriminates rather than that it is
#       stuck red.
#   C3  a plain Zephyr application (no digest variable) is never touched.
#
# The digest line's own format is pinned on the renderer side by
# `west_app::tests::the_application_states_its_own_digest`, which carries this
# file's regex by hand.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
runner="$repo/scripts/build/zephyr-fixture-run-one.sh"
agreement="$repo/cmake/NanoRosImageAgreement.cmake"
# shellcheck source=scripts/lib/grep-q.sh
. "$repo/scripts/lib/grep-q.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/nros-1707-XXXXXX")"
trap 'rm -rf "$work"' EXIT

fail=0
pass() { echo "  ok    $1"; }
bad() { echo "  FAIL  $1" >&2; fail=1; }

# ---- the runner (edge 1) ---------------------------------------------------
mkdir -p "$work/bin" "$work/zws" "$work/ws" "$work/build"
calls="$work/calls.log"
: > "$calls"
# Stubs append their argv; order in the file is the order they ran.
printf '#!/bin/sh\necho "nros $*" >> "%s"\n' "$calls" > "$work/bin/nros"
printf '#!/bin/sh\necho "ninja $*" >> "%s"\n' "$calls" > "$work/bin/ninja"
printf '#!/bin/sh\necho "west $*" >> "%s"\n' "$calls" > "$work/bin/west"
chmod +x "$work/bin/nros" "$work/bin/ninja" "$work/bin/west"

# One record, 24 tab-separated fields (the runner's own `usage`).
# $1 = build dir, $2 = sig, $3 = ws_dir, $4 = nros_image
record() {
    local f=(
        fixture "t/$(basename "$1")" "fixture/t" native_sim/native/64 rust rust entry zenoh
        examples/x "$work/ws" "$(basename "$1")" "$1"
        "$work/$(basename "$1").log" "" "" "" ""
        "-D_NANO_ROS_CODEGEN_TOOL=$work/bin/nros"
        "$2" "$1/.sig" 0 auto "$3" "$4"
    )
    local IFS=$'\t'
    printf '%s\n' "${f[*]}"
}

# A build dir the runner reads as CURRENT: build.ninja present, signature equal.
current_dir() {
    mkdir -p "$1"
    : > "$1/build.ninja"
    printf '%s\n' sig-1 > "$1/.sig"
}

run_runner() {
    : > "$calls"
    NROS_ZEPHYR_WORKSPACE="$work/zws" NROS_ZEPHYR_TOOL_PATH="$work/bin:/usr/bin:/bin" \
        NROS_PYTHON=/bin/true \
        bash "$runner" "$1" > "$work/runner.out" 2>&1
}

# R1
current_dir "$work/build/retargeted"
record "$work/build/retargeted" sig-1 "$work/ws" demo_bringup:zephyr_x > "$work/r1.tsv"
if ! run_runner "$work/r1.tsv"; then
    bad "R1 runner failed: $(tail -5 "$work/runner.out")"
else
    mapfile -t got < "$calls"
    want_regen="nros build demo_bringup:zephyr_x --workspace $work/ws --offline --dry-run"
    if [ "${#got[@]}" -eq 2 ] && [ "${got[0]}" = "$want_regen" ] \
        && [ "${got[1]%% *}" = ninja ]; then
        pass "R1 the ninja path regenerates the application before ninja"
    else
        bad "R1 expected [$want_regen] then ninja, got: $(printf '[%s] ' "${got[@]}")"
    fi
fi

# R2 — no image: ninja alone.
current_dir "$work/build/plain"
record "$work/build/plain" sig-1 "" "" > "$work/r2.tsv"
if ! run_runner "$work/r2.tsv"; then
    bad "R2 runner failed: $(tail -5 "$work/runner.out")"
else
    mapfile -t got < "$calls"
    if [ "${#got[@]}" -eq 1 ] && [ "${got[0]%% *}" = ninja ]; then
        pass "R2 a row with no image runs ninja alone"
    else
        bad "R2 expected ninja alone, got: $(printf '[%s] ' "${got[@]}")"
    fi
fi

# R2b — the west path (signature moved) runs `nros build` ONCE, not twice.
current_dir "$work/build/moved"
record "$work/build/moved" sig-2 "$work/ws" demo_bringup:zephyr_x > "$work/r2b.tsv"
if ! run_runner "$work/r2b.tsv"; then
    bad "R2b runner failed: $(tail -5 "$work/runner.out")"
else
    mapfile -t got < "$calls"
    if [ "${#got[@]}" -eq 1 ] && [ "${got[0]#nros build demo_bringup:zephyr_x}" != "${got[0]}" ] \
        && [ "${got[0]#*--dry-run}" = "${got[0]}" ]; then
        pass "R2b the west path runs nros build once, not a dry run"
    else
        bad "R2b expected one full nros build, got: $(printf '[%s] ' "${got[@]}")"
    fi
fi

# ---- the configure-side check (edge 2) -------------------------------------
app="$work/app"
mkdir -p "$app"
digest_a=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
digest_b=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
# The file ON DISK says B.
printf 'cmake_minimum_required(VERSION 3.20.0)\nset(NROS_GENERATED_APP_DIGEST %s)\n' \
    "$digest_b" > "$app/CMakeLists.txt"

# $1 = the digest the configure executed ("" = not a generated application)
check() {
    local def=""
    [ -z "$1" ] || def="set(NROS_GENERATED_APP_DIGEST $1)"
    printf 'include("%s")\nset(APPLICATION_SOURCE_DIR "%s")\n%s\nnros_check_generated_app_current()\n' \
        "$agreement" "$app" "$def" > "$work/check.cmake"
    cmake -P "$work/check.cmake" > "$work/check.out" 2>&1
}

if check "$digest_a"; then
    bad "C1 a configure that executed digest A against a file stating B was NOT refused"
elif nros_grep_q 'issue 1707' "$work/check.out"; then
    pass "C1 a configure that read a since-rewritten application is refused"
else
    bad "C1 refused, but not by this check: $(head -5 "$work/check.out")"
fi

if check "$digest_b"; then
    pass "C2 an application unchanged since the configure read it passes"
else
    bad "C2 the unchanged case was refused: $(head -5 "$work/check.out")"
fi

if check ""; then
    pass "C3 a plain Zephyr application (no digest) is not checked"
else
    bad "C3 a plain application was refused: $(head -5 "$work/check.out")"
fi

if [ "$fail" -ne 0 ]; then
    echo "zephyr_fixture_app_regen: FAILED (issue 1707)" >&2
    exit 1
fi
echo "zephyr_fixture_app_regen: ok"
