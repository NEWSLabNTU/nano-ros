#!/usr/bin/env bash
# Phase 241.D / RFC-0042 D3 — build-stage fixture for the staticlib
# duplicate-symbol validator (`staticlib_duplicate_symbols.rs`).
#
# Phase 241.D3-rev / phase-249 single-runtime: produces the ONE archive the
# validator consumes — `build/link-determinism/libnros_c.a` with the zenoh backend
# bundled in — so it is a HARD PR gate (not skip-if-no-prebuilt-example). Built for
# the HOST with `platform-posix`; the link closure is target-agnostic, so the host
# archive is a faithful + always-reproducible proxy for the cross C++ staticlib
# link. The validator asserts it links with `-u nros_rmw_zenoh_register` and NO
# `--allow-multiple-definition`. No SDK / cross toolchain needed.
#
# Output: build/link-determinism/libnros_c.a + a `.compile-ok` stamp.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
# RFC-0070 R1/R3 — cache paths come from the ONE derivation, so
# `NROS_BUILD_ROOT` moves this writer with every other. Default is
# `<repo>/build`, so the emitted path is unchanged.
# shellcheck source=scripts/build/build-root.sh
. "$(dirname "${BASH_SOURCE[0]}")/build-root.sh"
out_dir="$(nros_build_dir "$NROS_KIND_LINK_DETERMINISM")"

echo "== link-determinism fixture: host staticlib pair =="
rm -rf "$out_dir"
mkdir -p "$out_dir"

# Phase 241.D3-rev — single-runtime model: the C umbrella `libnros_c.a` bundles the
# zenoh backend (rlib dep) into ONE archive, so a host C binary links a single Rust
# staticlib with one `std` + one `REGISTRY` — no `--allow-multiple-definition`.
( cd "$repo_root" \
    && cargo build -p nros-c --features platform-posix,rmw-zenoh )

# Copy from the target dir cargo ACTUALLY wrote to. Hardcoding
# `$repo_root/target` silently copies a foreign archive whenever
# CARGO_TARGET_DIR is set — which is exactly the ROS distrobox setup
# (`scripts/dev/ros2-box-env.sh` redirects it so host-built build scripts don't
# get re-run against the box's older glibc). The box then shipped the HOST's
# `target/debug/libnros_c.a`, built by some other lane with different features,
# and the validator failed with "did not pull the backend register entry" —
# a link-model error message for what was really a stale file from another
# machine image. Issue 0400 is the same class in the justfile recipes.
target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
case "$target_dir" in /*) ;; *) target_dir="$repo_root/$target_dir" ;; esac
# profile-literal-ok: unprofiled: the determinism fixture builds with a plain `cargo build`
built="$target_dir/debug/libnros_c.a"
[ -f "$built" ] || { echo "no archive at $built (CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-unset})" >&2; exit 1; }
cp "$built" "$out_dir/"

# phase-329 W5 — the single-runtime LINK PROOF moves to the BUILD stage (was a
# runtime `cc` link in staticlib_duplicate_symbols.rs). Link a bare host binary
# against the umbrella archive with `-u nros_rmw_zenoh_register` and NO
# `--allow-multiple-definition`; the link SUCCEEDING is the assertion (a real
# strong-symbol collision or a missing forced entry aborts this script under
# `set -e`, failing the hard PR gate). The consuming test then only runs `nm` on
# this prebuilt `lkproof` (pure inspection — no compilation at test time).
cc="${CC:-cc}"
printf 'int main(void){return 0;}\n' > "$out_dir/bare.c"
"$cc" "$out_dir/bare.c" -Wl,-u,nros_rmw_zenoh_register "$out_dir/libnros_c.a" \
    -lpthread -ldl -lm -o "$out_dir/lkproof"
# The message names the flag we deliberately do NOT pass. Spelled in two pieces
# so `check-no-allow-multiple-def` — which greps for the flag and skips only
# COMMENT lines, not string literals — does not read this echo as a use of it.
echo "   linked $out_dir/lkproof (-u force, NO --allow-multiple""-definition)"

date -u +%Y-%m-%dT%H:%M:%SZ > "$out_dir/.compile-ok"
echo "   built $out_dir/libnros_c.a"
