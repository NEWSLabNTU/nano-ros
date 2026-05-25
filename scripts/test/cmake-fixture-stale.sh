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

out="$(cmake "${args[@]}" 2>&1)"
# cmake/ninja emit "Building <lang> object" / "Linking <lang>" only on a real
# rebuild; Corrosion's cargo step prints "Compiling <crate>" only when it
# actually recompiles (a fresh cargo prints just "Finished"). Any of these ⇒
# the cell was stale and has now been rebuilt.
if printf '%s' "$out" | grep -qE "Building (C|CXX|ASM) object|Linking (C|CXX|CXX shared)|Compiling [a-z0-9_-]+ v"; then
    echo "$bd"
fi
