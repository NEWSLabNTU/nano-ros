#!/usr/bin/env bash
# phase-475 W1/W5 — the census itself, in WHATEVER environment runs it.
#
# `lane-census.sh <image>` runs this inside a container; a CI job whose step
# already IS the lane's image (`nano-ros-ci`) runs it directly, through
# `lane-census.sh --here`. One body either way, so the scheduled census and a
# developer's local one cannot measure different things.
#
#   lane-census-run.sh <out-dir>
#
# NROS_CENSUS_RUNS (default 1) repeats the RUN, not the build: a target is only
# admissible if it reaches a verdict every time. Measured reason — two targets
# that compile at test time (`cmake_platform_matrix`,
# `native_main_macro_misuse`) failed in one full parallel census and passed in
# isolation in both image variants with identical source (issue 1620). One run
# cannot tell that from a real red; two disagreeing runs can.
#
# Exit 2 = the census could not measure (git unusable, CLI or crate build
# failed). A census of a broken build is a census of nothing, so it refuses
# rather than reporting every target as failing.
set -uo pipefail
out="${1:?usage: lane-census-run.sh <out-dir>}"
runs="${NROS_CENSUS_RUNS:-1}"
mkdir -p "$out"
git ls-files >/dev/null 2>&1 || { echo "census: git is not usable here" >&2; exit 2; }
# shellcheck disable=SC1091
source ./activate.sh >/dev/null 2>&1 || true
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros >/dev/null 2>&1 \
    || { echo "census: the gate job's CLI build failed" >&2; exit 2; }
export PATH="$PWD/packages/cli/target/release:$PATH"
NROS_CARGO_FLAGS= cargo nextest run -p nros-tests --no-run >/dev/null 2>&1 \
    || { echo "census: nros-tests does not build here" >&2; exit 2; }
for i in $(seq 1 "$runs"); do
    start=$(date +%s)
    NROS_CARGO_FLAGS= cargo nextest run -p nros-tests --no-fail-fast >/dev/null 2>&1
    echo "census: run $i/$runs took $(( $(date +%s) - start ))s"
    cp target/nextest/default/junit.xml "$out/junit-$i.xml"
done
