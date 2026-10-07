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
# The gate job builds the launch resolver before `test-lane-contracts`
# ("Build nros-launch-resolve", gate.yml), so admitted tests may use it. The
# census must provision the same, in THIS body, or the two census paths disagree:
# the first scheduled CI census (2026-10-06) reported three admitted tests as
# NO LONGER PASSING on `nros-launch-resolve not built`, while a local image run
# passed them only because it copied the developer's host-built resolver in.
# The compile-check rows staged below that resolve a bringup run `nros sync`,
# which needs it too (issue 1656).
just setup-launch-resolve >/dev/null 2>&1 \
    || { echo "census: the gate job's launch-resolver build failed" >&2; exit 2; }
NROS_CARGO_FLAGS= cargo nextest run -p nros-tests --no-run >/dev/null 2>&1 \
    || { echo "census: nros-tests does not build here" >&2; exit 2; }
# issue 1656 — stage the compile-check stamps the gate lane builds. A target
# that reads a build-stage stamp or verdict classifies FIXTURE when nothing
# staged it, and was never admitted — four verdict targets left the gate lane
# that way. `--census` is every compile-resolver target's stamp rows;
# `test-lane-contracts` builds the admitted subset with `--admission`, through
# the same derivation. Serial and keep-going: a row that cannot build here
# costs its own targets their admission, and the census still measures.
census_ids="$(python3 scripts/test/lane-compile-stamps.py --census)" \
    || { echo "census: cannot derive the compile-check stamp rows" >&2; exit 2; }
start=$(date +%s)
if NROS_FIXTURE_IDS="$census_ids" NROS_COMPILE_CHECK_POOL=0 \
    bash scripts/build/compile-check-fixtures.sh >"$out/compile-stamps.log" 2>&1; then
    echo "census: staged $(tr ',' '\n' <<<"$census_ids" | wc -l) compile-check row(s) in $(( $(date +%s) - start ))s"
else
    echo "census: some compile-check rows did not build here (compile-stamps.log) — their targets are not admissible" >&2
fi
for i in $(seq 1 "$runs"); do
    start=$(date +%s)
    NROS_CARGO_FLAGS= cargo nextest run -p nros-tests --no-fail-fast >/dev/null 2>&1
    echo "census: run $i/$runs took $(( $(date +%s) - start ))s"
    cp target/nextest/default/junit.xml "$out/junit-$i.xml"
done
