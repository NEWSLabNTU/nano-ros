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
# Only this fixture's OUTPUTS are cleared: the image crate and its target dir
# below persist, so a re-run is incremental rather than a zenoh-pico rebuild.
rm -f "$out_dir/libnros_c.a" "$out_dir/bare.c" "$out_dir/lkproof" "$out_dir/.compile-ok"
mkdir -p "$out_dir"

# Phase 241.D3-rev — single-runtime model: the C umbrella `libnros_c.a` bundles the
# zenoh backend (rlib dep) into ONE archive, so a host C binary links a single Rust
# staticlib with one runtime + one `REGISTRY` — no `--allow-multiple-definition`.
# (phase-359 W10 stage C: that runtime is `#![no_std]` — `panic-platform` and the
# port's allocator — so the hosted set is named, not `std`.)
#
# RFC-0071 D9 (phase-506 W4d) — `nros-c` names no backend, so the archive is
# built from the IMAGE CRATE `cmake/NanoRosImageCrate.cmake` generates (its
# `-P` script mode): `nros-c` plus the backend the zenoh descriptor names, with
# the platform forwarded through that descriptor. Its lib is named `nros_c`, so
# the archive keeps its name. It is its own cargo root, so it gets its own
# target dir (issue 0616) — which also retires the `CARGO_TARGET_DIR` hazard
# this script used to guard (a foreign `target/debug/libnros_c.a` built by
# another lane, issue 0400's class): nothing else writes there.
image_dir="$out_dir/image"
if ! image="$(cmake --log-level=WARNING -DNROS_IMAGE_FLAVOR=c -DNROS_IMAGE_RMW=zenoh \
        -DNROS_IMAGE_EDITION=humble -DNROS_IMAGE_OUT_DIR="$image_dir" \
        "-DNROS_IMAGE_API_FEATURES=alloc;env;log;panic-platform;platform-posix;rmw-cffi;ros-humble" \
        -P "$repo_root/cmake/NanoRosImageCrate.cmake")"; then
    echo "link-determinism: generating the zenoh image crate failed" >&2
    exit 1
fi
target_dir="$image_dir/target"
cargo build --manifest-path "${image%% *}" --no-default-features \
    --features "${image#* }" --target-dir "$target_dir"

# profile-literal-ok: unprofiled: the determinism fixture builds with a plain `cargo build`
built="$target_dir/debug/libnros_c.a"
[ -f "$built" ] || { echo "no archive at $built" >&2; exit 1; }
cp "$built" "$out_dir/"

# phase-329 W5 — the single-runtime LINK PROOF moves to the BUILD stage (was a
# runtime `cc` link in staticlib_duplicate_symbols.rs). Link a bare host binary
# against the umbrella archive with `-u nros_rmw_zenoh_register` and NO
# `--allow-multiple-definition`; the link SUCCEEDING is the assertion (a real
# strong-symbol collision or a missing forced entry aborts this script under
# `set -e`, failing the hard PR gate). The consuming test then only runs `nm` on
# this prebuilt `lkproof` (pure inspection — no compilation at test time).
cc="${CC:-cc}"
# Every C image owes `nros_app_register_backends` (no weak default on the C
# path); a real image gets it from its generated entry. Since phase-359 W10
# stage C the archive's own panic runtime is always pulled into a link and can
# share an object with `rclc_support_init_rmw`, so a bare binary states it too.
printf 'void nros_app_register_backends(void);\nvoid nros_app_register_backends(void){}\nint main(void){return 0;}\n' > "$out_dir/bare.c"
"$cc" "$out_dir/bare.c" -Wl,-u,nros_rmw_zenoh_register "$out_dir/libnros_c.a" \
    -lpthread -ldl -lm -o "$out_dir/lkproof"
# The message names the flag we deliberately do NOT pass. Spelled in two pieces
# so `check-no-allow-multiple-def` — which greps for the flag and skips only
# COMMENT lines, not string literals — does not read this echo as a use of it.
echo "   linked $out_dir/lkproof (-u force, NO --allow-multiple""-definition)"

date -u +%Y-%m-%dT%H:%M:%SZ > "$out_dir/.compile-ok"
echo "   built $out_dir/libnros_c.a"
