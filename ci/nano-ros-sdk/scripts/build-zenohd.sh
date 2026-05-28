#!/usr/bin/env bash
# Build zenohd (pinned 1.7.2 for rmw_zenoh_cpp compat) and package it for one
# host. Mirrors [tool.zenohd.source] in nros-sdk-index.toml. Phase 187.5.
#
#   build-zenohd.sh <version> <host-key>   ->   dist/zenohd-<host-key>.tar.zst
set -euo pipefail

version="${1:?usage: build-zenohd.sh <version> <host-key>}"
host="${2:?usage: build-zenohd.sh <version> <host-key>}"

# Index version is <upstream>-nros<n>; build from the upstream tag.
upstream="${version%-nros*}"

root="$(pwd)"
prefix="$root/out/zenohd"
rm -rf "$root/zenoh-src" "$prefix"
mkdir -p "$prefix" "$root/dist"

git clone --depth 1 --branch "$upstream" https://github.com/eclipse-zenoh/zenoh zenoh-src
# `cargo install --root` lays down <prefix>/bin/zenohd — matches the contract.
cargo install --path zenoh-src/zenohd --root "$prefix" --locked

tar --use-compress-program "zstd -19 -T0" \
    -cf "dist/zenohd-${host}.tar.zst" -C "$prefix" .
echo "built dist/zenohd-${host}.tar.zst"
