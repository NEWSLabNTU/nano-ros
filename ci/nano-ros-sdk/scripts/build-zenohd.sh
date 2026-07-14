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
# #189 — `zenoh/transport_serial` is NOT a zenohd default; without it the
# router refuses `--listen serial/...` ("Unicast not supported for serial
# protocol") and exits, killing every serial e2e lane. The legacy in-tree
# build (scripts/zenohd/build.sh) always enabled it; the phase-187 SDK
# migration dropped it. Keep this in lockstep with [tool.zenohd.source]
# `install` in nros-sdk-index.toml.
cargo install --path zenoh-src/zenohd --root "$prefix" --locked \
    --features zenoh/transport_serial

tar --use-compress-program "zstd -19 -T0" \
    -cf "dist/zenohd-${host}.tar.zst" -C "$prefix" .
echo "built dist/zenohd-${host}.tar.zst"
