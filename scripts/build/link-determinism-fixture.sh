#!/usr/bin/env bash
# Phase 241.D / RFC-0042 D3 — build-stage fixture for the staticlib
# duplicate-symbol validator (`staticlib_duplicate_symbols.rs`).
#
# Produces the `(libnros_c.a, libnros_rmw_zenoh_staticlib.a)` pair the validator
# consumes, so it is a HARD PR gate (not skip-if-no-prebuilt-example). Built for
# the HOST with `platform-posix` — the duplicate set the validator checks is the
# shared Rust dependency closure, which is target-agnostic, so the host pair is a
# faithful + always-reproducible proxy for the cross C++ staticlib link that
# carries `--allow-multiple-definition`. No SDK / cross toolchain needed.
#
# Output: build/link-determinism/{libnros_c.a,libnros_rmw_zenoh_staticlib.a}
# + a `.compile-ok` stamp the test gates on.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
out_dir="$repo_root/build/link-determinism"

echo "== link-determinism fixture: host staticlib pair =="
rm -rf "$out_dir"
mkdir -p "$out_dir"

# Phase 241.D3-rev — single-runtime model: the C umbrella `libnros_c.a` bundles the
# zenoh backend (rlib dep) into ONE archive, so a host C binary links a single Rust
# staticlib with one `std` + one `REGISTRY` — no `--allow-multiple-definition`.
( cd "$repo_root" \
    && cargo build -p nros-c --features platform-posix,rmw-zenoh )

cp "$repo_root/target/debug/libnros_c.a" "$out_dir/"

date -u +%Y-%m-%dT%H:%M:%SZ > "$out_dir/.compile-ok"
echo "   built $out_dir/libnros_c.a"
