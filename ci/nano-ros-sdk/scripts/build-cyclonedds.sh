#!/usr/bin/env bash
# Build the Cyclone DDS host tools (`idlc` + `libddsc`) for the rmw-cyclonedds
# path and package them for one host. Mirrors [tool.cyclonedds.source] in
# nros-sdk-index.toml. Phase 191.6.
#
#   build-cyclonedds.sh <version> <host-key> <upstream>  ->  dist/cyclonedds-<host>.tar.zst
#
# `upstream` is a sha/ref on the NEWSLabNTU/cyclonedds fork (= the index
# [tool.cyclonedds].upstream, the submodule pointer).
set -euo pipefail

version="${1:?usage: build-cyclonedds.sh <version> <host-key> <upstream>}"
host="${2:?usage: build-cyclonedds.sh <version> <host-key> <upstream>}"
upstream="${3:?usage: build-cyclonedds.sh <version> <host-key> <upstream>}"

root="$(pwd)"
src="$root/cyclonedds-src"
build="$src/build"
prefix="$root/out/cyclonedds"
rm -rf "$src" "$prefix"
mkdir -p "$prefix" "$root/dist"

# Full clone + checkout: `upstream` is a sha, not a branch/tag.
git clone https://github.com/NEWSLabNTU/cyclonedds "$src"
git -C "$src" checkout --detach "$upstream"

cmake -S "$src" -B "$build" \
    -DCMAKE_INSTALL_PREFIX="$prefix" \
    -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_IDLC=ON \
    -DBUILD_TESTING=OFF \
    -DBUILD_DDSPERF=OFF
cmake --build "$build" --parallel "$(nproc 2>/dev/null || echo 4)" --target install

test -x "$prefix/bin/idlc" \
    || { echo "error: idlc not installed to $prefix/bin"; exit 1; }

tar --use-compress-program "zstd -19 -T0" \
    -cf "dist/cyclonedds-${host}.tar.zst" -C "$prefix" .
echo "built dist/cyclonedds-${host}.tar.zst"
