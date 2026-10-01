#!/usr/bin/env bash
# phase-475 W1 — run every `nros-tests` target in a LANE'S OWN IMAGE and record,
# per target, whether it reaches a verdict there.
#
# Why a census and not an inference: a target's preconditions are written four
# ways (a library probe, a helper module that probes, a test-local probe, an
# inline `if … { skip! }`), and only the environment can say which of them hold.
# Measured on ci-base (phase-475): 878 cases over 167 targets run in ~24 s with
# no fixtures staged, because an unmet precondition ends a test in milliseconds.
#
# Usage:  scripts/test/lane-census.sh <image> [out-dir]
#
# The checkout is mounted READ-ONLY and copied inside the container, so the run
# never writes root-owned files into the tree. In a linked worktree `.git` is a
# FILE naming the main checkout's gitdir; that gitdir is mounted at the same
# absolute path, read-only, or every `git ls-files`-based test fails for a
# reason that is about the census and not about the code (measured: 6 of the
# first run's 11 FAILs).
#
# Provisions only the gate job's CLI build. The real lane provisions MORE
# (compile-tier sources, compile-check fixtures, bindings, the launch
# resolver), so a VERDICT here is a verdict there — the admissible set this
# reports is a LOWER BOUND, and a target that needs one of those shows up as a
# precondition rather than as a pass.
set -uo pipefail
image="${1:?usage: lane-census.sh <image> [out-dir]}"
out="${2:-$PWD/tmp/lane-census}"
root="$(git rev-parse --show-toplevel)"
common="$(git rev-parse --path-format=absolute --git-common-dir)"
name="nros-lane-census-$$"
mkdir -p "$out"
trap 'docker rm -f "$name" >/dev/null 2>&1 || true' EXIT

docker run --name "$name" \
    -v "$root":/src:ro \
    -v "$common":"$common":ro \
    --entrypoint bash "$image" -lc '
set -uo pipefail
cp -a /src /work && cd /work
rm -rf target packages/cli/target
git config --global --add safe.directory "*"
git ls-files >/dev/null 2>&1 || { echo "census: git is not usable in /work" >&2; exit 2; }
source ./activate.sh >/dev/null 2>&1 || true
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros >/dev/null 2>&1 \
    || { echo "census: the gate job'"'"'s CLI build failed" >&2; exit 2; }
export PATH="/work/packages/cli/target/release:$PATH"
NROS_CARGO_FLAGS= cargo nextest run -p nros-tests --no-run >/dev/null 2>&1 \
    || { echo "census: nros-tests does not build in this image" >&2; exit 2; }
start=$(date +%s)
NROS_CARGO_FLAGS= cargo nextest run -p nros-tests --no-fail-fast >/dev/null 2>&1
echo "census: run took $(( $(date +%s) - start ))s"
mkdir -p /out && cp target/nextest/default/junit.xml /out/junit.xml
'
rc=$?
[ "$rc" -eq 2 ] && exit 2
docker cp "$name":/out/junit.xml "$out/junit.xml" >/dev/null
python3 "$root/scripts/test/lane-census-classify.py" "$out/junit.xml" "$out/per-target.json"
