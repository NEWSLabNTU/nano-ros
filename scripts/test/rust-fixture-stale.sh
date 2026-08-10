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
# example-local target/ tree while the build wrote build/fixtures-cargo/<group>,
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

# $cargo_args / $prof_args / $tdir_flag are intentionally word-split into cargo
# flags; $envstr ("KEY=VAL ...") is exported into the build subshell when present.
# shellcheck disable=SC2086
if ( cd "$dir"; [ -n "$envstr" ] && export $envstr; \
        cargo build $prof_args $cargo_args $tdir_flag --message-format=json --quiet 2>/dev/null ) \
        | grep -q '"fresh":false'; then
    echo "$dir${cargo_args:+ ($cargo_args)}"
fi
