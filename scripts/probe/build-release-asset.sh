#!/usr/bin/env bash
# Build a release asset the way `release-nros.yml` does — phase-447 A3.
#
# Runs INSIDE the builder container that run-bootstrap-probe.sh starts for the
# `installed` track. Never on the host: the asset links the builder's glibc and
# libpython, and the release pins those by pinning its runner
# (`runs-on: ubuntu-22.04`), so the probe pins the same image for the same
# reason. An asset built on whatever host ran the probe would carry that host's
# ABI and prove nothing about the one users download.
#
# What it runs is NOT written here. `/probe/release-steps.sh` is extracted from
# the workflow by extract-workflow-steps.py — see that file for why the probe
# must not carry its own idea of what an asset contains.
#
# This file owns only what a GitHub runner provides and a bare `ubuntu:22.04`
# does not — the RUNNER IMAGE, not the release:
#   * the packages the hosted image ships (git, curl, a C toolchain,
#     python3-dev for the resolver's pyo3 link, zstd for the tarball);
#   * a Rust toolchain (the hosted image carries rustup + stable);
#   * `actions/checkout@v4`, which is a depth-1 checkout of the commit.
# The workflow's own `Install zstd` step is runner provisioning too, and is
# replaced by the first bullet rather than extracted.
#
# Inputs (env): PROBE_BRANCH, HOST_UID, HOST_GID.
# Mounts: /nano-ros-git (the repo's git dir, RO), /probe/release-steps.sh,
#         /out (receives the asset + its .sha256), and optionally the cache
#         volumes /cache/target-cli and /cache/target-resolve.

set -euo pipefail

: "${PROBE_BRANCH:?PROBE_BRANCH names the commit to build}"

echo '=== release builder: runner-image shim ==='
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq git curl ca-certificates build-essential pkg-config \
    python3 python3-dev zstd >/dev/null
if [ ! -x "$HOME/.cargo/bin/cargo" ]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain stable --no-modify-path
fi
# shellcheck disable=SC1091
. "$HOME/.cargo/env"
cargo --version
# The mounted git dir belongs to the host uid; git refuses to read a repo owned
# by someone else unless told otherwise. An artifact of the bind mount, not of
# anything a runner does.
git config --global --add safe.directory '*'

echo "=== release builder: checkout $PROBE_BRANCH (actions/checkout@v4 shape) ==="
# Depth 1, as actions/checkout@v4 fetches by default — which matters, because
# the workflow's `Record` step has a fallback for exactly that shallowness.
# `file://` because git ignores `--depth` for a plain-path local clone.
src=/src
git clone -q --depth 1 --branch "$PROBE_BRANCH" file:///nano-ros-git "$src"
echo "building $(git -C "$src" rev-parse --short HEAD): $(git -C "$src" log -1 --format=%s)"
# The cache volumes (run-bootstrap-probe.sh, PROBE_RELEASE_CACHE=1) mount under
# /cache rather than inside the clone, because a clone refuses a destination
# that already holds mount points. Link them where the workflow's steps expect
# cargo's output; both paths are gitignored, so the checkout is unchanged.
link_cache() {  # link_cache <volume> <path in the clone>
    if [ -d "$1" ]; then
        ln -s "$1" "$src/$2"
    fi
}
link_cache /cache/target-cli packages/cli/target
link_cache /cache/target-resolve packages/cli/nros-launch-resolve/target

export GITHUB_WORKSPACE="$src"
export RUNNER_TEMP=/runner-temp
mkdir -p "$RUNNER_TEMP"
bash /probe/release-steps.sh

asset="$RUNNER_TEMP/nros-linux-x86_64.tar.zst"
for f in "$asset" "$asset.sha256"; do
    if [ ! -s "$f" ]; then
        echo "PROBE FAIL: the release steps finished without producing $f" >&2
        echo "  (the asset install.sh downloads — the workflow's 'Stage the prefix' step writes it)" >&2
        exit 1
    fi
done
cp "$asset" "$asset.sha256" /out/
chown "${HOST_UID:-0}:${HOST_GID:-0}" /out/nros-linux-x86_64.tar.zst /out/nros-linux-x86_64.tar.zst.sha256
echo "release builder: asset -> /out ($(du -h /out/nros-linux-x86_64.tar.zst | cut -f1))"
