#!/usr/bin/env bash
# Issue 1419 -- take every census a fixture build's cross configures will check,
# BEFORE any of them configures.
#
# A cross image (`demo_bringup:threadx`, `:zephyr`, ...) whose model folds in a
# `*.contract.yaml` runs `nros ws entity-census check --require-fresh` at
# configure time (phase-463 W4). The census it checks is produced by the NATIVE
# image generated from the same launch file, run in census mode -- and the
# configure never builds that image itself (issue 0641). So an unattended build
# that configures cross rows must take the census first, or every such
# configure reads "census missing" -- a warning while `[census] on_missing`
# defaulted to `warn`, and a refusal now that it defaults to `refuse`.
#
# `nros ws entity-census take --image <q>` does the work and decides what to
# do: nothing for a host image, a model with no contract or a census that is
# already fresh; otherwise build the native sibling to its fixed point and run
# it. This script only enumerates the images (`fixtures-manifest.py
# census-images`, one row per workspace dir + qualified image) and runs it once
# per pair.
#
# WHERE IT RUNS, and why it is SERIAL. `take` builds a native image into the
# same build tree the native fixture rows build into
# (`<ws>/build/posix-zenoh-native/cmake`), and `build-test-fixtures` runs its
# platform stages in PARALLEL. A take inside the freertos stage racing the
# native stage's own build of that tree is two cmake builds in one directory.
# So `build-test-fixtures` runs this ONCE, before any stage starts, and exports
# `NROS_CENSUS_PREPASS=done`; the per-platform callers
# (`workspace-fixtures-build.sh`, the Zephyr leaf lane) run it themselves only
# when invoked directly, where nothing else is building concurrently.
#
# Usage: census-prepass.sh [--platform P] [--lang L] [--id ID]
#   Honours NROS_FIXTURE_COORDS (the lane's coordinates) like the builders do.
set -euo pipefail

if [ "${NROS_CENSUS_PREPASS:-}" = "done" ]; then
    echo "census-prepass: already taken before the stages started (NROS_CENSUS_PREPASS=done)"
    exit 0
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

filter_args=()
while [ $# -gt 0 ]; do
    case "$1" in
        --platform|--lang|--id)
            [ $# -ge 2 ] || { echo "census-prepass.sh: $1 needs a value" >&2; exit 2; }
            filter_args+=("$1" "$2")
            shift 2
            ;;
        *)
            echo "census-prepass.sh: unknown option: $1" >&2
            echo "usage: census-prepass.sh [--platform P] [--lang L] [--id ID]" >&2
            exit 2
            ;;
    esac
done
if [ -n "${NROS_FIXTURE_COORDS:-}" ]; then
    filter_args+=(--coords-from "$NROS_FIXTURE_COORDS")
fi

source "$repo_root/scripts/build/cargo.sh"
nros_cli="$(nros_cli_bin)" || exit 3

rows_rc=0
rows="$(python3 "$repo_root/scripts/build/fixtures-manifest.py" census-images \
    "${filter_args[@]}")" || rows_rc=$?
if [ "$rows_rc" -ne 0 ]; then
    echo "census-prepass: fixtures-manifest.py census-images failed (exit $rows_rc)" >&2
    exit 1
fi
if [ -z "$rows" ]; then
    echo "census-prepass: no cross workspace image in scope"
    exit 0
fi

synced=""
while IFS=$'\t' read -r dir image; do
    [ -n "$dir" ] || continue
    case " $synced " in
        *" $dir "*) ;;
        *)
            # `take` resolves the model the configure will read, and that model
            # is what `nros sync` writes; the builders sync before they build,
            # and this runs before them.
            ( cd "$repo_root/$dir" && "$nros_cli" sync --no-provider-index >/dev/null )
            synced="$synced $dir"
            ;;
    esac
    echo "census-prepass: $dir $image"
    ( cd "$repo_root/$dir" && "$nros_cli" ws entity-census take --image "$image" \
        --workspace . --offline )
done <<< "$rows"
