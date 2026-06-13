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

# `crate-type = ["staticlib"]`; `platform-posix` is the host port. The RFC-0042
# D3 slice-4 provider archive (defines the cffi C ABI once) builds too. The
# `external-registry` feature mirrors the non-NuttX cmake C/C++ link: `nros-c` +
# the RMW staticlib reference `REGISTRY` as an undefined external so the provider
# archive is its lone definer (the provider pins the feature via its cffi dep).
( cd "$repo_root" \
    && cargo build -p nros-c --features platform-posix,external-registry \
    && cargo build -p nros-rmw-zenoh-staticlib --features platform-posix,external-registry \
    && cargo build -p nros-rmw-cffi-provider --features platform-posix )

cp "$repo_root/target/debug/libnros_c.a" "$out_dir/"
cp "$repo_root/target/debug/libnros_rmw_zenoh_staticlib.a" "$out_dir/"
cp "$repo_root/target/debug/libnros_rmw_cffi_provider.a" "$out_dir/"

date -u +%Y-%m-%dT%H:%M:%SZ > "$out_dir/.compile-ok"
echo "   built $out_dir/{libnros_c.a,libnros_rmw_zenoh_staticlib.a,libnros_rmw_cffi_provider.a}"
