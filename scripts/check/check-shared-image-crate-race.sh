#!/usr/bin/env bash
# check-shared-image-crate-race: same-content image roots generated in parallel
# land as ONE complete root (issue 2676).
#
# The metadata probe runs several workspaces at once, and with
# NROS_SHARED_IMAGE_CRATE_ROOT every configure whose image root has the same
# content writes the same directory. There is no lock: each configure builds
# the root complete in a staging dir of its own and renames it into place, and
# a rename that finds the root already there drops its stage. Every generation
# after the first takes that branch, so this exercises it N-1 times, not only
# in a race nobody can time. It runs the generator N times in parallel (script
# mode, no build) and checks:
#   - exactly one root, holding the same bytes as a solo run;
#   - no staging dir left;
#   - every run printed that root's manifest.
# Negative control: a different feature set is a different root, so the
# identity is the content and not, say, the first caller's arguments.
# Mutation, measured 2026-11-18: making the "already there" branch an error
# turns this red.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
n="${1:-8}"
mkdir -p "$repo/tmp"
work="$(mktemp -d "$repo/tmp/shared-image-crate-race.XXXXXX")"
trap 'rm -rf "$work"' EXIT

gen() { # <shared-root> <features> <out-file>
    if ! cmake -DNROS_IMAGE_FLAVOR=c -DNROS_IMAGE_RMW=zenoh -DNROS_IMAGE_EDITION=humble \
        -DNROS_IMAGE_OUT_DIR="$work/unused" "-DNROS_IMAGE_API_FEATURES=$2" \
        -DNROS_SHARED_IMAGE_CRATE_ROOT="$1" \
        -P "$repo/cmake/NanoRosImageCrate.cmake" > "$3" 2>&1; then
        echo "check-shared-image-crate-race: the generator failed:" >&2
        sed 's/^/    /' "$3" | head -20 >&2
        return 1
    fi
}

# The negative control, run on every invocation: other content must get its own
# root, so the identity is the content and not, say, the first caller's
# arguments. Without it, a generator that wrote every root to one fixed path
# would pass the checks below.
selftest() {
    local d="$work/selftest"
    gen "$d" "platform-posix" "$work/selftest-a.out" || return 1
    gen "$d" "platform-posix;alloc" "$work/selftest-b.out" || return 1
    local n
    n=$(find "$d" -mindepth 1 -maxdepth 1 -type d | wc -l)
    if [ "$n" -ne 2 ]; then
        echo "check-shared-image-crate-race: selftest: two contents made $n root(s), want 2"
        return 1
    fi
}

fail=0
selftest || fail=1
gen "$work/solo" "platform-posix" "$work/solo.out"
solo="$(ls -d "$work"/solo/nros-c-image-* | grep -v -E '\.lock$')"

pids=()
for i in $(seq 1 "$n"); do
    gen "$work/shared" "platform-posix" "$work/run$i.out" &
    pids+=($!)
done
for p in "${pids[@]}"; do
    wait "$p" || { echo "check-shared-image-crate-race: a parallel run failed"; fail=1; }
done

roots=$(find "$work/shared" -mindepth 1 -maxdepth 1 -type d | wc -l)
stages=$(find "$work/shared" -mindepth 1 -maxdepth 1 -name '*.stage-*' | wc -l)
[ "$roots" -eq 1 ] || { echo "check-shared-image-crate-race: $roots roots for one content, want 1"; fail=1; }
[ "$stages" -eq 0 ] || { echo "check-shared-image-crate-race: $stages staging dir(s) left behind"; fail=1; }
shared="$(find "$work/shared" -mindepth 1 -maxdepth 1 -type d | head -1)"
if [ "$(basename "$shared")" != "$(basename "$solo")" ]; then
    echo "check-shared-image-crate-race: parallel root $(basename "$shared") != solo $(basename "$solo")"
    fail=1
fi
for f in Cargo.toml src/lib.rs; do
    cmp -s "$solo/$f" "$shared/$f" || { echo "check-shared-image-crate-race: $f differs from a solo run"; fail=1; }
done
for i in $(seq 1 "$n"); do
    grep -q "$shared/Cargo.toml" "$work/run$i.out" \
        || { echo "check-shared-image-crate-race: run $i did not name the shared root"; fail=1; }
done

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "check-shared-image-crate-race: OK — $n parallel generations of one content made one complete root, byte-equal to a solo run, no stage left; selftest: other content gets its own root"
