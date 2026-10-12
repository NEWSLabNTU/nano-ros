#!/usr/bin/env bash
# The CI cache for `just generate-bindings`' outputs (phase-489 W4).
#
#   scripts/ci/bindings-cache.sh key     # key= / restore-prefix= / paths<<EOF
#   scripts/ci/bindings-cache.sh check   # refuse a cached path git tracks
#
# WHAT IS CACHED is what the verb writes and later reads back to decide that
# it has nothing to do — and nothing else:
#
#   * every gitignored `generated/` tree (+ its `.nros-generate-rust.sig`
#     stamp) under `examples/` and the standalone test-bin roots, and
#   * every workspace member's metadata-probe sidecar
#     `src/<pkg>/metadata/*.json` and its negative marker `*.unprobeable`.
#
# NOT the probe build dirs (`<ws>/build/nros-metadata`, ~1.6 GB a workspace)
# and no cargo target: measured, a tree holding fresh sidecars runs the whole
# verb in 9 s with every probe build dir moved away, and recreates none of
# them. The 19-22 min the scheduled lanes paid was the metadata probes (~60-120
# s per C/C++ workspace), not codegen (~18 s for the whole tree).
#
# The cache ONLY PRE-POPULATES. The job still runs `just generate-bindings`,
# and the verb's own content-addressed freshness decides every package:
# `generate-rust-incremental.sh`'s signature (CLI binary, ament set, the
# package's msg/srv/action/package.xml) for a standalone `generated/`, and
# `metadata_refresh::probe_inputs_key` (the package's sources + the CLI source
# stamp + every crate the probe compiles) for a sidecar. A stale restored file
# is regenerated, never trusted — so the key decides only how OFTEN the cache
# helps, never whether the result is right.
#
# THE KEY, from real inputs, never time:
#
#   restore-prefix  nros-bindings-v1-<os>-<arch>-<path>-<ament>-<codegen>-
#     <path>     the checkout path: both the standalone signature and the
#                negative markers record absolute paths, so a cache from
#                another checkout path could only ever miss.
#     <ament>    `nros_generate_rust_ament_signature` — the SAME function the
#                verb's own signature calls (distro, prefix paths, the sha1
#                of every interface file under each prefix's share/).
#     <codegen>  the three `codegen_version.rs` (emitter, runtime, build
#                helper). In the PREFIX, so a codegen bump falls back to
#                nothing at all: every `generated/` tree and every fixture
#                built from one is rebuilt from cold (issue 1360's pair).
#   key             <restore-prefix><inputs>
#     <inputs>   the CLI binary key (`cli-bin-cache-key.sh`, a superset of
#                the CLI source stamp every sidecar carries) + the git index
#                entries of every package the verb visits, every crate the
#                probe compiles (`packages/cli/probe-source-dirs.txt`), the
#                cmake the C/C++ probe configures through, and the verb's own
#                scripts.
#
# An exact hit means the inputs are byte-identical to the run that saved it.
# A prefix hit (`restore-keys`) restores the newest entry for the same ament
# set and codegen version, and the verb re-validates it per package — the
# sidecars of a workspace nobody touched stay fresh, the rest re-probe.
#
# Index entries, not file contents, like `cli-bin-cache-key.sh`: a CI checkout
# is clean, so the index sha IS the content. Nothing outside CI should use it.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
cd "$root"

# The verb's output set — the ONE list. The workflow reads it back from the
# `paths` output, so the cache step and this script cannot name different
# files.
#
# FIXED-DEPTH globs, never `**`: by the time the save step runs the probe build
# dirs exist (`<ws>/build/nros-metadata/...`, with `cargo` symlinks into the
# shared target), and a `**` both walks them and matches their own
# `metadata/*.json` (measured: 60 such files under `build/` on a built tree).
# The depths are the verb's: a standalone package is `examples/<a>/<b>[/<c>]`
# or a test-bin `<root>/<x>[/<y>]`, a workspace root is `examples/<a>[/<b>]`,
# and a sidecar sits at `<ws root>/src/<member>/metadata/`. The `!build`
# exclusions keep a dir NAMED like an output inside a build tree out.
GENERATED_DIRS=(
    'examples/*/*/generated'
    'examples/*/*/*/generated'
    'examples/*/*/*/*/generated'
    'packages/testing/nros-bench/*/generated'
    'packages/testing/nros-bench/*/*/generated'
    'packages/testing/nros-tests/bins/*/generated'
    'packages/testing/nros-tests/bins/*/*/generated'
    'packages/testing/nros-smoke/*/generated'
    'packages/testing/nros-smoke/*/*/generated'
)
SIDECAR_FILES=(
    'examples/*/src/*/metadata/*.json'
    'examples/*/src/*/metadata/*.unprobeable'
    'examples/*/*/src/*/metadata/*.json'
    'examples/*/*/src/*/metadata/*.unprobeable'
)
EXCLUDES=(
    '!examples/**/build/**'
    '!packages/testing/**/build/**'
)

cache_paths() {
    printf '%s\n' "${GENERATED_DIRS[@]}" "${SIDECAR_FILES[@]}" "${EXCLUDES[@]}"
}

# The same globs as git pathspecs, for `check`: a restore must never write over
# a tracked file, and `actions/cache` would do exactly that without a word.
tracked_in_cache_paths() {
    local specs=() g
    for g in "${GENERATED_DIRS[@]}"; do specs+=(":(glob)$g/**"); done
    for g in "${SIDECAR_FILES[@]}"; do specs+=(":(glob)$g"); done
    git ls-files -- "${specs[@]}"
}

check() {
    local tracked
    tracked="$(tracked_in_cache_paths)"
    if [ -n "$tracked" ]; then
        echo "bindings-cache: these TRACKED files sit under a cached path, so a" >&2
        echo "  restore would overwrite them with another commit's bytes:" >&2
        printf '%s\n' "$tracked" | sed 's/^/    /' >&2
        echo "  Narrow the cached globs in scripts/ci/bindings-cache.sh, or move" >&2
        echo "  the file out of a generated/ or metadata/ dir." >&2
        return 1
    fi
}

sha() { sha256sum | cut -c1-16; }

key() {
    check

    # shellcheck source=scripts/build/generate-rust-incremental.sh
    source scripts/build/generate-rust-incremental.sh
    local ament codegen path os
    ament="$(nros_generate_rust_ament_signature | sha)"
    local codegen_entries
    codegen_entries="$(git ls-files -s -- \
        packages/cli/rosidl-codegen/src/codegen_version.rs \
        packages/core/nros-core/src/codegen_version.rs \
        packages/tooling/nros-build-helpers/src/codegen_version.rs)"
    if [ "$(printf '%s\n' "$codegen_entries" | grep -c .)" -ne 3 ]; then
        echo "bindings-cache: expected 3 codegen_version.rs in the index, found:" >&2
        printf '%s\n' "$codegen_entries" >&2
        return 1
    fi
    codegen="$(printf '%s\n' "$codegen_entries" | sha)"
    path="$(printf '%s' "$root" | sha)"
    os="$(uname -s)-$(uname -m)"

    local probe_list="packages/cli/probe-source-dirs.txt"
    if [ ! -f "$probe_list" ]; then
        echo "bindings-cache: $probe_list is missing — probe_inputs_key would" >&2
        echo "  never call a sidecar fresh without it, so there is nothing to key" >&2
        return 1
    fi
    local dirs=(
        examples
        packages/testing/nros-bench
        packages/testing/nros-tests/bins
        packages/testing/nros-smoke
        cmake
        zephyr/cmake
        nano_rosConfig.cmake
        "$probe_list"
        scripts/regenerate-bindings.sh
        scripts/build/generate-rust-incremental.sh
        scripts/build/cargo.sh
        scripts/lib/checkout-paths.sh
        scripts/ci/bindings-cache.sh
    )
    local line
    while IFS= read -r line; do
        line="${line%%#*}"
        line="${line//[[:space:]]/}"
        [ -n "$line" ] && dirs+=("$line")
    done < "$probe_list"

    local inputs
    inputs="$({
        printf 'cli=%s\n' "$(scripts/ci/cli-bin-cache-key.sh "$root")"
        git ls-files -s -- "${dirs[@]}"
    } | sha)"

    local prefix="nros-bindings-v1-$os-$path-$ament-$codegen-"
    echo "key=$prefix$inputs"
    echo "restore-prefix=$prefix"
    echo "paths<<NROS_BINDINGS_PATHS"
    cache_paths
    echo "NROS_BINDINGS_PATHS"
}

case "${1:-}" in
    key) key ;;
    check) check ;;
    *)
        echo "usage: $0 key|check" >&2
        exit 2
        ;;
esac
