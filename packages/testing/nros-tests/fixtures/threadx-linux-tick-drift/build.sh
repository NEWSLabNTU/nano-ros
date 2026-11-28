#!/usr/bin/env bash
# Build-stage fixture for `tests/threadx_linux_tick.rs` (issue 3017): the
# tick-drift probe, a ThreadX Linux-port process the test runs and reads one
# line from.
#
#   build.sh <out-dir>
#
# This is the `fixture-script` builder's script for the
# `threadx_linux_tick_drift` `[[compile_check_fixture]]` row. Into <out-dir>:
#   tick-drift      the probe binary
#   cmake/          its own CMake tree
#   cmake-deps.d    ninja's recorded compile inputs, as Make dep-info, so the
#                   row's signature covers the kernel sources the build READ —
#                   the port's `tx_initialize_low_level.c` above all, which
#                   holds the simulated timer interrupt under test
#
# The kernel is located like every other ThreadX build here (`THREADX_DIR`,
# else the store or the in-tree submodule through `nros locate`); a host
# without it fails here rather than producing nothing (issue 1758).
set -euo pipefail

out_dir="${1:?usage: build.sh <out-dir>}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../../../.." && pwd)"
target="tick-drift"
bld="$out_dir/cmake"

# shellcheck source=scripts/build/cargo.sh
source "$repo_root/scripts/build/cargo.sh"
# shellcheck source=scripts/lib/grep-q.sh
source "$repo_root/scripts/lib/grep-q.sh"
if ! threadx_dir="$(nros_locate_source threadx)"; then
    echo "FAIL: the ThreadX kernel is not provisioned (nros locate threadx) — nros setup --source threadx" >&2
    exit 1
fi

echo "== threadx-linux-tick-drift fixture: $target =="
mkdir -p "$out_dir"
rm -f "$out_dir/$target" "$out_dir/cmake-deps.d"

# nros-cmake-prefix-exempt: a bare ThreadX kernel probe — it builds no Rust and
# never calls corrosion_import_crate().
cmake -S "$script_dir" -B "$bld" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DNROS_REPO_DIR="$repo_root" \
    -DTHREADX_DIR="$threadx_dir" >/dev/null
if [ "${NROS_JOBSERVER:-}" = "1" ]; then
    cmake --build "$bld" --target "$target"
else
    cmake --build "$bld" --target "$target" --parallel
fi

built="$bld/$target"
[ -x "$built" ] || {
    echo "FAIL: cmake reported success but $built is absent" >&2
    exit 1
}
cp -f "$built" "$out_dir/$target"

# Same dep-info conversion as `cyclone-slirp-guest/build.sh`: ninja records each
# object's inputs relative to the build dir, so absolutize them.
ninja -C "$bld" -t deps | awk -v b="$bld" '
    /^[^ \t].*: #deps/ { obj = $1; sub(/:$/, "", obj); next }
    /^[ \t]+[^ \t]/ {
        p = $0; sub(/^[ \t]+/, "", p)
        if (substr(p, 1, 1) != "/") p = b "/" p
        gsub(/ /, "\\ ", p)
        print obj ": " p
    }
' > "$out_dir/cmake-deps.d"
rc=0
nros_grep_q 'ports/linux/gnu/src/tx_initialize_low_level\.c' "$out_dir/cmake-deps.d" || rc=$?
case $rc in
    0) ;;
    1)
        echo "FAIL: ninja recorded no compile of the Linux port's tx_initialize_low_level.c — the row's signature would not cover the code under test" >&2
        exit 1
        ;;
    *)
        echo "FAIL: could not read $out_dir/cmake-deps.d (grep exit $rc)" >&2
        exit 1
        ;;
esac
echo "   built $out_dir/$target ($(wc -l < "$out_dir/cmake-deps.d") recorded inputs)"
