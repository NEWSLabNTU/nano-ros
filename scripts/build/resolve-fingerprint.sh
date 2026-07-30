#!/usr/bin/env bash
# Print the RESOLVER fingerprint — RFC-0061 / phase-318 W1.b.
#
# A hash of the SystemModel `nros-launch-resolve` emits for a committed probe
# launch tree, so it moves iff resolved models would.
#
# Why the resolver is in the fixture signature at all: `nros sync` shells out to
# it (RFC-0060), and the SystemModel that comes back IS a fixture input — it is
# committed, consumed by `nros::main!(model = …)`, and its contents change what
# gets built. A signature blind to the resolver repeats issue #182 one layer
# down, where a fixture built by a museum resolver verifies as fresh. Both
# 2026-07-28 skews were invisible to a nros-only signature:
#
#   * the rebuilt `nros` passed `--bringup-root` and the installed resolver
#     rejected it ("unexpected argument");
#   * issue 0320's fix changed emitted models from absolute to repo-relative
#     paths — an output change, in fixture inputs, from an unhashed tool.
#
# Cached per binary hash, so this costs one resolve per rebuild and a file read
# thereafter.
#
# FALLBACK, deliberately: the resolver embeds CPython, so on a host without a
# usable Python it cannot run. Then emit `binary:<sha256>` — today's
# over-approximation (a rebuild invalidates fixtures) rather than "assume fresh".
# Failing safe beats the optimisation.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
repo_root="$PWD"

bin="$repo_root/packages/cli/nros-launch-resolve/target/release/nros-launch-resolve"
probe="$repo_root/packages/cli/nros-launch-resolve/tests/fixtures/fingerprint-launch"

if [ ! -x "$bin" ]; then
    printf 'resolver-absent'
    exit 0
fi

# PRECONDITION, and it is load-bearing: this assumes `$bin` is not itself
# stale. The cache is keyed on the BINARY's hash, so a museum binary has a
# stable hash, emits a stable fingerprint, and every fixture is reported FRESH
# forever. This mechanism cannot see that and must not be asked to — keeping the
# binary current is `just setup-launch-resolve`'s staleness probe.
#
# That probe has to walk the resolver's NESTED submodule (ros-launch-manifest
# lives inside ros-launch-resolve), which `git ls-files` does not do without
# `--recurse-submodules`. It briefly did not, and the result was exactly the
# laundering described above: a binary a day older than its sources, reported
# fresh. This comment exists so the two halves are read together — W1.b's own
# note says a signature blind to the resolver repeats #182 one layer down, and
# a probe blind to the resolver's SOURCES repeats it one layer further.
bin_hash="$(sha256sum "$bin" | awk '{print $1}')"
cache="$repo_root/.nros-cache/resolve-fingerprint/$bin_hash"

if [ -s "$cache" ]; then
    cat "$cache"
    exit 0
fi

fp=""
if [ -f "$probe/launch/system.launch.xml" ]; then
    # `-o -` writes the model to stdout. The model is deterministic for a fixed
    # tree: input paths are recorded RELATIVE to --bringup-root (issue 0320) and
    # the recorded sha256 is of the launch file, not of anything host-specific.
    if out="$("$bin" "$probe/launch/system.launch.xml" --bringup-root "$probe" -o - 2>/dev/null)" \
        && [ -n "$out" ]; then
        fp="$(printf '%s' "$out" | sha256sum | awk '{print $1}')"
    fi
fi

if [ -z "$fp" ]; then
    # Could not resolve (no CPython, probe missing, resolver too old) — degrade
    # to the binary hash, never to "assume fresh".
    fp="binary:$bin_hash"
fi

mkdir -p "$(dirname "$cache")" 2>/dev/null && printf '%s' "$fp" > "$cache" || true
printf '%s' "$fp"
