#!/usr/bin/env bash
# Print a cmake fixture cell's build dir if `cmake --build` had to rebuild it
# (self-heal), reusing cmake/ninja + cargo incrementality. Phase 181.7c.
#
# A detect-only `ninja -n` is not usable here: our C/C++ cells link nros-c /
# nros-cpp via Corrosion, whose cargo step is an always-run custom command, so
# `ninja -n` always reports it pending. Instead we run the incremental build
# (near-no-op when fresh: cargo fingerprint check + ninja skip) and decide from
# the output whether real compile/link work happened.
#
# Input: ONE manifest cmake record line (from `fixtures-manifest.py list
# --lang c|cpp`): <dir>\x1f<build-subdir>\x1f<cmake-defs>\x1f<target>. Only the
# dir + build-subdir (+ optional target) are used — the cell is already
# configured. Must be invoked from the repo root.
set -u

line="$1"
IFS=$'\x1f' read -r dir sub _defs target <<< "$line"
[ -n "${dir:-}" ] && [ -n "${sub:-}" ] || exit 0
bd="$dir/$sub"

# Only probe configured cells; an unbuilt cell is a presence concern (the
# build-fixtures stamp), not staleness.
[ -f "$bd/build.ninja" ] || [ -f "$bd/Makefile" ] || exit 0

args=(--build "$bd")
[ -n "${target:-}" ] && args+=(--target "$target")

# Decide from the ARTIFACT, not from the build's chatter — issue 0835.
#
# The old test grepped the output for "Building <lang> object" / "Linking …" /
# "Compiling <crate> v". "Linking …" is printed on EVERY run of these cells and
# always will be: Corrosion drives nros-c / nros-cpp through a custom target
# whose stamp file is never created, so ninja reports
#
#   ninja explain: output …/CMakeFiles/_cargo-build_nros_cpp doesn't exist
#   ninja explain: …/libnros_cpp.a is dirty
#
# — an always-dirty edge BY DESIGN (the cargo step must always run), whose
# declared output feeds the link. So the link re-runs, the grep matches, and the
# cell is "stale" forever. Measured: the produced executable is byte-identical
# across consecutive builds while the probe reported stale every time. That kept
# 17 cells permanently stale, which is a large share of the ~190 fixture-stale
# failures on every `just ci-matrix`.
#
# Dropping "Linking" from the grep would be the wrong fix: a relink with NO
# compile is exactly how a changed backend archive reaches a binary (issue
# 0475), so a probe blind to it would miss the real case. Hashing the artifact
# separates them — a forced relink of unchanged inputs reproduces the same
# bytes; a real input change does not.
#
# Every one of the 120 cmake cells has exactly one top-level executable in its
# build dir, so "the artifact" is unambiguous. If that ever stops being true the
# fallback below keeps the old behaviour rather than silently passing.
_arts() { find "$bd" -maxdepth 1 -type f -executable -printf '%f ' -exec md5sum {} + 2>/dev/null | awk '{print $1, $2}'; }

before="$(_arts)"
if ! out="$(cmake "${args[@]}" 2>&1)"; then
    # A cell that does not BUILD is not fresh. The old probe ignored the exit
    # status entirely, so a failing cell produced output without the grep
    # patterns and was reported as fresh — a red cell reading as a green one.
    echo "$bd"
    exit 0
fi
after="$(_arts)"

if [ -z "$before" ] && [ -z "$after" ]; then
    # No artifact to compare (shape changed). Fall back to the output grep so
    # this degrades to the previous behaviour instead of passing blindly.
    if printf '%s' "$out" | grep -qE "Building (C|CXX|ASM) object|Linking (C|CXX|CXX shared)|Compiling [a-z0-9_-]+ v"; then
        echo "$bd"
    fi
    exit 0
fi

[ "$before" != "$after" ] && echo "$bd"
exit 0
