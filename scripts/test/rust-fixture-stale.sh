#!/usr/bin/env bash
# Print a rust fixture's id if cargo considers it stale — reusing cargo's own
# fingerprint instead of a custom input hash (Phase 177.9 / 181).
#
# Input: ONE manifest record line (from `fixtures-manifest.py list
# --with-platform`), 0x1F-separated: <platform>\x1f<dir>\x1f<env>\x1f<cargo-args>.
# Building with the fixture's EXACT features/target-dir/env (not default
# features) is required — mismatched features make cargo rebuild on every probe
# (feature thrash) and report false staleness.
#
# Phase 226.D — the leading <platform> field drives the shared
# fixture-target-dir resolver (scripts/build/fixtures-target-dir.sh), the
# SAME helper fixtures-build.sh uses. Without it the probe would inspect the
# example-local target/ tree while the build wrote build/cargo-fixtures/<group>,
# producing permanent false-stale reports.
#
# `cargo build --message-format=json` is a no-op when fresh and rebuilds only
# stale units; a `"fresh":false` artifact means the fixture was stale (and is
# now rebuilt). Must be invoked from the repo root.
set -u

line="$1"
IFS=$'\x1f' read -r platform dir envstr cargo_args <<< "$line"
[ -n "${dir:-}" ] || exit 0

# shellcheck source=/dev/null
source scripts/build/cargo.sh 2>/dev/null || exit 0
# shellcheck source=scripts/lib/grep-q.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/grep-q.sh"
# phase-340 P2 — the PLATFORM's profile, not the ambient one. A platform with a
# carve-out (`freertos` -> freertos-qemu, `nuttx` -> nuttx-rust) builds its whole
# rust fixture lane at that profile, and cargo writes each profile into its own
# `<root>/[<triple>/]<profile>/` subtree. Probing the ambient profile would
# therefore compile a SECOND copy from scratch on every run and report
# `"fresh":false` for all of it — a permanent false-STALE against a tree the
# builder never wrote. Same class as the `--target-dir` mismatch this file's
# header describes; one spelling, `nros_cargo_platform_profile`, shared with the
# recipes.
#
# One flag per LINE from this accessor; `$prof_args` is word-split below and
# the default IFS splits on newline too, so it reaches cargo the same way the
# single-string form did.
prof_args="$(nros_cargo_profile_args_for "$(nros_cargo_platform_profile "$platform")")"

# Phase 226.D — append the shared fixture-only --target-dir for eligible
# rows so the probe stats the same artifact tree fixtures-build.sh wrote.
NROS_REPO_ROOT="${NROS_REPO_ROOT:-$PWD}"
# shellcheck source=scripts/build/fixtures-target-dir.sh
source scripts/build/fixtures-target-dir.sh 2>/dev/null || exit 0
tdir_flag="$(nros_fixture_target_dir_flag "$platform" "$cargo_args" "$envstr")"
# phase-340 W2 — an authored `--target-dir` now names a GROUP rather than
# opting the row out, so when the group governs the row's own flag is stripped.
# This MUST mirror the same two lines in fixtures-build.sh: a probe that passed
# both flags would build into the leaf while the build wrote the group dir, and
# report permanent false-stale — the exact failure the header above describes.
if [ -n "$tdir_flag" ]; then
    cargo_args="$(nros_fixture_strip_authored_target_dir "$cargo_args")"
fi

# Decide from the ARTIFACT, not from cargo's `"fresh":false` — issue 0835.
#
# `"fresh":false` means cargo re-ran a UNIT, which is not the same as "the
# artifact was out of date". These fixtures share one `--target-dir` per platform
# (the phase-340 group), and rows in one group EVICT each other: build row A,
# probe A again → fresh; build sibling row B; probe A again → stale. Measured,
# and the produced binaries are byte-identical the whole way through:
#
#   A fresh, artifacts: 78ecc91a b11f4d7c
#   after B:            78ecc91a b11f4d7c
#   A stale again? [examples/qemu-arm-baremetal/rust/talker]
#   after A rebuild:    78ecc91a b11f4d7c
#
# So ~22 rows reported stale on EVERY run, forever, with nothing to fix — a
# large share of the ~190 fixture-stale test failures on every `just ci-matrix`.
# The mutual eviction is worth its own look (a shared `--target-dir` across
# separate workspace roots is issue 0616's territory), but it is a waste of
# CPU, not a correctness problem: the bytes never change. The probe should not
# have been reporting it as staleness.
#
# Scoped to THIS ROW's binaries rather than the whole profile directory: probes
# run under `parallel` against a shared group dir, so a sibling's concurrent
# build would otherwise register as this row's change.
_row_bins() {
    # `[[bin]] name = "..."` lines, else the package name. A leaf with neither
    # yields nothing and the caller falls back.
    local m="$dir/Cargo.toml"
    [ -f "$m" ] || return 0
    local bins
    bins="$(sed -n 's/^[[:space:]]*name[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$m")"
    printf '%s\n' "$bins" | sort -u
}

_row_artifacts() {
    local td="${tdir_flag#*--target-dir }"
    td="${td%% *}"
    [ -n "$td" ] || td="$dir/target"
    local b
    for b in $(_row_bins); do
        # `<td>/<triple>/<profile>/<bin>` and `<td>/<profile>/<bin>` — the
        # triple comes from the leaf's own .cargo/config.toml for most of these
        # rows, so glob rather than trying to re-derive it.
        md5sum "$td"/*/*/"$b" "$td"/*/"$b" 2>/dev/null
    done | sort -u
}

# issue 0466 — a probe build that FAILS must not read as fresh.
#
# This used to be `( cargo build … 2>/dev/null ) | grep -q '"fresh":false'`,
# which keeps only grep's status: cargo's exit code was discarded and its errors
# went to /dev/null. So a fixture that could not compile at all printed no
# `"fresh":false` line and was reported FRESH — the gate cleared having verified
# nothing, the artifact was never produced, and the failure resurfaced later as
# ~100 tests panicking with "Test fixture binary not prebuilt". A gate that
# passes because its probe broke is worse than no gate, because it launders the
# lane green (the same laundering the SCOPE handling in check-fixtures-stale.sh
# exists to prevent, one layer down).
#
# So capture the status, and report the three outcomes distinctly: FRESH (say
# nothing), stale-and-now-rebuilt (the dir, as before — cargo self-healed it),
# and could-not-build (a `FAILED\t` record the caller escalates to an ERROR).
# Keep cargo's own first error line: it is the difference between "you forgot
# `nros sync`" and a real compile break, and re-running the probe by hand to
# find out costs minutes.
#
# $cargo_args / $prof_args / $tdir_flag are intentionally word-split into cargo
# flags; $envstr ("KEY=VAL ...") is exported into the build subshell when present.
art_before="$(_row_artifacts)"

# shellcheck disable=SC2086
build_out="$( cd "$dir"; [ -n "$envstr" ] && export $envstr; \
        cargo build $prof_args $cargo_args $tdir_flag --message-format=json --quiet 2>&1 )"
build_rc=$?

if [ -z "$art_before" ] && [ -z "$(_row_artifacts)" ]; then
    # Nothing to compare — an unbuilt row, or a leaf whose bins this cannot
    # name. Fall back to cargo's own signal rather than passing blindly.
    #
    # issue 0945 item 6 — SAY SO. This branch is the one a cargo output-layout
    # change produces, and it is silent: `_row_artifacts` globs
    # `<target-dir>/[<triple>/]<profile>/<bin>`, cargo's convention rather than
    # anything cargo promises. If that layout moves, every row lands here at
    # once, the probe reverts to `"fresh":false`, and for rows sharing a
    # phase-340 group that signal is PERMANENTLY true because they evict each
    # other — which is issue 0835 exactly: ~22 rows reporting stale on every run
    # with byte-identical binaries, and ~190 fixture-stale failures per
    # `just ci-matrix`. Reported as a self-healing WARNING, which reads as the
    # gate working.
    #
    # Measured 2026-09-05 across the shared checkout: 116 of 117 rust rows find
    # an artifact, so this branch is nearly unreachable today and a jump in the
    # count is the signal. The one row already on it is
    # `packages/testing/qemu-smoltcp-bridge`, whose Cargo.toml names no binary
    # the glob can find.
    #
    # `DEGRADED` widens no watch set, so it cannot make 0835 worse — phase-424's
    # constraint. It is a fact ABOUT the verdict, not a new input to it.
    printf 'DEGRADED\t%s\t%s\n' "$dir${cargo_args:+ ($cargo_args)}" \
        "no artifact matched $(_row_bins | tr '\n' ' ' | sed 's/ $//' | sed 's/^$/(no [[bin]] name in Cargo.toml)/'); verdict falls back to cargo's \"fresh\" flag"
    if [ "$build_rc" -eq 0 ]; then
        # `nros_grep_q`, not a bare `grep -q`: this branch decides a VERDICT, and
        # a grep that fails to start (issue 0726 — measured under a 32-way gate
        # fan-out) would otherwise read as "not stale" and launder the lane
        # green. That is the same laundering the exit-status handling above
        # exists to prevent, one layer down.
        nros_grep_q '"fresh":false' <<<"$build_out"
        case $? in
            0)
                echo "$dir${cargo_args:+ ($cargo_args)}"
                exit 0
                ;;
            1) : ;;
        esac
    fi
fi

if [ "$build_rc" -ne 0 ]; then
    # `--message-format=json` puts diagnostics on stdout as JSON too, so prefer a
    # bare `error[E...]`/`error:` line and fall back to naming the exit code.
    reason="$(printf '%s\n' "$build_out" | grep -m1 -E '^error' || true)"
    printf 'FAILED\t%s\t%s\n' "$dir${cargo_args:+ ($cargo_args)}" \
        "${reason:-cargo exited $build_rc}"
elif [ "$(_row_artifacts)" != "$art_before" ]; then
    echo "$dir${cargo_args:+ ($cargo_args)}"
fi
