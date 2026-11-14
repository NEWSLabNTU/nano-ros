#!/usr/bin/env bash
# Warm the cargo cache with what EVERY offline fixture build can reach, before
# the first `nros build --offline`. Two graphs, fetched once per process
# environment:
#
#   1. the repo-root workspace: nano-ros's own crates. A cmake- or west-driven
#      image builds them (`nros_c_cargo_build`) through a cargo that has no
#      root of its own to warm, and offline it stopped on
#      `failed to download const-default v1.0.0` (live-peer, 2026-11-14);
#   2. each pinned toolchain's std sources
#      (`<sysroot>/lib/rustlib/src/rust/library`), so a `-Zbuild-std` run can
#      resolve std offline.
#
# Why (issue 2560): `nros build --offline` makes every cargo it drives offline,
# the cmake-built Rust FFI glue included, and that glue is a `-Zbuild-std`
# build with no lock of its own. Resolving it takes in std's `sysroot`
# workspace, so offline it needs every std dependency in the cached index. A
# CARGO_HOME that never resolved std has none of them, and the build stops at
# `no matching package named addr2line` (NuttX) or `rustc-literal-escaper`
# (Zephyr). Measured on a scratch CARGO_HOME: cold fails, and after this fetch
# the same offline build-std passes.
#
# The one network step, like the cache warm in `leaf-cache.sh`; the build that
# follows stays offline. Never fatal: an offline-by-design host with a warm
# cache is legitimate, so a failed fetch is reported, not raised. A channel is
# fetched once per CARGO_HOME and std lock (stamp under CARGO_HOME).
#
# Usage: scripts/build/warm-offline-cargo.sh   (reads config/rust-toolchains.txt)
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
list="$here/../../config/rust-toolchains.txt"
[ "${NROS_SKIP_LEAF_FETCH:-0}" = "0" ] || exit 0
[ -r "$list" ] || { echo "warm-offline-cargo: cannot read $list" >&2; exit 0; }
stamps="${CARGO_HOME:-$HOME/.cargo}/.nros-std-sources"

# 1. The repo-root workspace (what leaf-cache.sh's root warm used to do alone).
if ! ( cd "$here/../.." && cargo fetch >/dev/null 2>&1 ); then
    echo "  cargo-warm: could NOT warm the cargo cache for the repo root." >&2
fi

# 2. std's sources.

# Channels whose components include rust-src: only they can build std.
while read -r channel components _; do
    case "$channel" in ''|'#'*) continue ;; esac
    case ",$components," in *,rust-src,*) ;; *) continue ;; esac
    sysroot="$(RUSTUP_AUTO_INSTALL=0 rustc +"$channel" --print sysroot 2>/dev/null)" || continue
    lib="$sysroot/lib/rustlib/src/rust/library"
    [ -f "$lib/Cargo.lock" ] || continue
    sum="$(sha256sum "$lib/Cargo.lock" | cut -c1-16)"
    [ -e "$stamps/$channel-$sum" ] && continue
    # A stable channel builds std only under RUSTC_BOOTSTRAP, and std's manifest
    # names a nightly cargo feature, so the fetch needs it too.
    if ( cd / && RUSTC_BOOTSTRAP=1 cargo +"$channel" fetch --manifest-path "$lib/Cargo.toml" >/dev/null 2>&1 ); then
        mkdir -p "$stamps" && : > "$stamps/$channel-$sum"
    else
        echo "  std-fetch: could NOT fetch std's dependencies for $channel." >&2
        echo "             An offline -Zbuild-std build below may then report" >&2
        echo "             'no matching package named <crate>': the crate is missing" >&2
        echo "             from the cache, not from crates.io." >&2
    fi
done < "$list"
exit 0
