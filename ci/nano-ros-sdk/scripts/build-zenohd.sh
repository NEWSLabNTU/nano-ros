#!/usr/bin/env bash
# Build zenohd (pinned 1.7.2 for rmw_zenoh_cpp compat) and package it for one
# host. Mirrors [tool.zenohd.source] in nros-sdk-index.toml. Phase 187.5.
#
#   build-zenohd.sh <version> <host-key>   ->   dist/zenohd-<host-key>.tar.zst
set -euo pipefail

version="${1:?usage: build-zenohd.sh <version> <host-key> <upstream>}"
host="${2:?usage: build-zenohd.sh <version> <host-key> <upstream>}"
# Upstream tag (e.g. 1.7.2) — SSOT is the index [tool.*].upstream, passed by
# build-tool.yml. No longer derived from the version label.
upstream="${3:?usage: build-zenohd.sh <version> <host-key> <upstream>}"

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
