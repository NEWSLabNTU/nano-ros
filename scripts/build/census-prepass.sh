#!/usr/bin/env bash
# Issue 1419 -- take every census a fixture build's cross configures will check,
# BEFORE any of them configures.
#
# A cross image (`demo_bringup:threadx`, `:zephyr`, ...) whose model folds in a
# `*.contract.yaml` runs `nros dev image entity-census check --require-fresh` at
# configure time (phase-463 W4). The census it checks is produced by the NATIVE
# image generated from the same launch file, run in census mode -- and the
# configure never builds that image itself (issue 0641). So an unattended build
# that configures cross rows must take the census first, or every such
# configure reads "census missing" -- a warning while `[census] on_missing`
# defaulted to `warn`, and a refusal now that it defaults to `refuse`.
#
# `nros dev image entity-census take --image <q>` does the work and decides what to
# do: nothing for a host image, a model with no contract or a census that is
# already fresh; otherwise build the native sibling to its fixed point and run
# it. This script only enumerates the images (`fixtures-manifest.py
# census-images`, one row per workspace dir + qualified image) and runs it once
# per pair.
#
# WHERE IT RUNS, and why it is SERIAL. `take` builds a native image into the
# same build tree the native fixture rows build into
# (`<ws>/build/posix-zenoh-linux/cmake`), and `build-test-fixtures` runs its
# platform stages in PARALLEL. A take inside the freertos stage racing the
# native stage's own build of that tree is two cmake builds in one directory.
# So `build-test-fixtures` runs this ONCE, before any stage starts, and exports
# `NROS_CENSUS_PREPASS=done`; the per-platform callers
# (`workspace-fixtures-build.sh`, the Zephyr leaf lane) run it themselves only
# when invoked directly, where nothing else is building concurrently.
#
# Issue 1556 -- and every standalone C/C++ LEAF counted by its census
# (`entities = "census"`, or -- RFC-0120 D4 -- a cross leaf that states no
# `entities`): its configure reads that census through `sizing-descriptor
# --from-leaf`, and refuses without a fresh one. `fixtures-manifest.py
# census-leaves` lists the candidates; `nros dev image entity-census take --leaf <dir>`
# decides (a leaf the rule does not count is "nothing to take") and
# builds the leaf's own `src/` for the host and runs it, into
# `<leaf>/build/census-host` -- a tree no fixture row builds into, so this half
# could run anywhere; it runs here for the same reason as the images: once,
# before the rows fan out.
#
# Usage: census-prepass.sh [--platform P] [--lang L] [--id ID] [--only images|leaves]
#   Honours NROS_FIXTURE_COORDS (the lane's coordinates) like the builders do.
#   `--only` narrows to one half: the workspace builder asks for images, the
#   plain-row builder (`fixtures-build.sh`) for leaves.
set -euo pipefail

if [ "${NROS_CENSUS_PREPASS:-}" = "done" ]; then
    echo "census-prepass: already taken before the stages started (NROS_CENSUS_PREPASS=done)"
    exit 0
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

filter_args=()
only=""
while [ $# -gt 0 ]; do
    case "$1" in
        --only)
            [ $# -ge 2 ] || { echo "census-prepass.sh: --only needs images|leaves" >&2; exit 2; }
            case "$2" in
                images|leaves) only="$2" ;;
                *) echo "census-prepass.sh: --only takes images|leaves, not \`$2\`" >&2; exit 2 ;;
            esac
            shift 2
            ;;
        --platform|--lang|--id)
            [ $# -ge 2 ] || { echo "census-prepass.sh: $1 needs a value" >&2; exit 2; }
            filter_args+=("$1" "$2")
            shift 2
            ;;
        *)
            echo "census-prepass.sh: unknown option: $1" >&2
            echo "usage: census-prepass.sh [--platform P] [--lang L] [--id ID] [--only images|leaves]" >&2
            exit 2
            ;;
    esac
done
if [ -n "${NROS_FIXTURE_COORDS:-}" ]; then
    filter_args+=(--coords-from "$NROS_FIXTURE_COORDS")
fi

source "$repo_root/scripts/build/cargo.sh"
# shellcheck source=scripts/build/leaf-cache.sh
source "$repo_root/scripts/build/leaf-cache.sh"
# Resolved on first use: a builder with nothing in scope (a cargo-only
# platform's rows) must not need a current CLI to learn that.
nros_cli=""
need_cli() {
    if [ -z "$nros_cli" ]; then
        nros_cli="$(nros_cli_bin)" || exit 3
    fi
}

# Issue 1556 -- the leaves first: each is independent of every workspace.
if [ "$only" != "images" ]; then
    leaves_rc=0
    leaves="$(python3 "$repo_root/scripts/build/fixtures-manifest.py" census-leaves \
        "${filter_args[@]}")" || leaves_rc=$?
    if [ "$leaves_rc" -ne 0 ]; then
        echo "census-prepass: fixtures-manifest.py census-leaves failed (exit $leaves_rc)" >&2
        exit 1
    fi
    # phase-530 W2 (Ruling R30) -- the leaves are independent (each builds
    # into its own `<leaf>/build/census-host`), and since RFC-0120 D4 made the
    # census the default every C/C++ cross leaf has one, at 70-165 s of host
    # build each. So they run side by side, as many at once as the fixture
    # build runs cmake frontends (`nros_cmake_frontend_jobs`, the budget
    # `NROS_CMAKE_FRONTENDS` / `NROS_BUILD_JOBS` already set), each told its
    # share of the cores (`nros_cargo_inner_jobs`) so N builds do not each
    # claim the whole machine. A failure is still fatal, and EVERY failing
    # leaf is named, with its log, rather than the first.
    if [ -n "$leaves" ]; then
        need_cli
        width="$(nros_cmake_frontend_jobs)"
        inner="$(nros_cargo_inner_jobs "$width")"
        log_dir="$repo_root/${NROS_BUILD_LOG_DIR:-build}/census-prepass"
        mkdir -p "$log_dir"
        echo "census-prepass: $(grep -c . <<< "$leaves") leaf candidate(s), $width at a time"
        pids=()
        names=()
        failed=()
        reap() {
            # Wait for whichever take finishes first (bash 5.1 `wait -n -p`);
            # record it if it failed, and drop it from the running set.
            local rc=0 done_pid="" i
            wait -n -p done_pid "${pids[@]}" || rc=$?
            for i in "${!pids[@]}"; do
                if [ "${pids[$i]}" = "$done_pid" ]; then
                    if [ "$rc" -ne 0 ]; then
                        failed+=("${names[$i]} (exit $rc; log: $log_dir/$(echo "${names[$i]}" | tr / _).log)")
                    fi
                    unset 'pids[i]' 'names[i]'
                    pids=("${pids[@]}")
                    names=("${names[@]}")
                    return
                fi
            done
        }
        while IFS= read -r leaf; do
            [ -n "$leaf" ] || continue
            while [ "${#pids[@]}" -ge "$width" ]; do
                reap
            done
            echo "census-prepass: leaf $leaf"
            log="$log_dir/$(echo "$leaf" | tr / _).log"
            CMAKE_BUILD_PARALLEL_LEVEL="$inner" \
                "$nros_cli" dev image entity-census take --leaf "$repo_root/$leaf" \
                > "$log" 2>&1 &
            pids+=("$!")
            names+=("$leaf")
        done <<< "$leaves"
        while [ "${#pids[@]}" -gt 0 ]; do
            reap
        done
        if [ "${#failed[@]}" -gt 0 ]; then
            echo "census-prepass: ${#failed[@]} leaf census(es) FAILED:" >&2
            for f in "${failed[@]}"; do
                echo "  - $f" >&2
            done
            exit 1
        fi
    fi
fi
if [ "$only" = "leaves" ]; then
    exit 0
fi

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

need_cli
synced=""
while IFS=$'\t' read -r dir image; do
    [ -n "$dir" ] || continue
    case " $synced " in
        *" $dir "*) ;;
        *)
            # `take` resolves the model the configure will read, and that model
            # is what `nros configure` writes; the builders sync before they build,
            # and this runs before them.
            ( cd "$repo_root/$dir" && "$nros_cli" configure --no-provider-index >/dev/null )
            synced="$synced $dir"
            ;;
    esac
    echo "census-prepass: $dir $image"
    # Issue 1848 -- `take --offline` BUILDS the host sibling image, and an
    # offline build can only use crates already in the cache. Ask `take` which
    # image that is (it owns the choice) and warm that image's cache first, as
    # `workspace-fixtures-build.sh` does before its own offline builds. On a
    # runner whose only rows are cross images, nothing else has fetched the
    # host image's graph, and the take died on `failed to download` with
    # `--frozen was specified`.
    plan_rc=0
    plan="$(cd "$repo_root/$dir" && "$nros_cli" dev image entity-census take --image "$image" \
        --workspace . --plan)" || plan_rc=$?
    if [ "$plan_rc" -ne 0 ]; then
        echo "census-prepass: \`take --plan\` failed for $dir $image (exit $plan_rc)" >&2
        exit 1
    fi
    producer="$(printf '%s\n' "$plan" | sed -n 's/^producer //p' | head -n 1)"
    if [ -n "$producer" ]; then
        nros_warm_leaf_cache "$repo_root/$dir" "$producer"
    fi
    ( cd "$repo_root/$dir" && "$nros_cli" dev image entity-census take --image "$image" \
        --workspace . --offline )
done <<< "$rows"
