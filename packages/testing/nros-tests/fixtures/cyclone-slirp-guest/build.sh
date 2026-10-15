#!/usr/bin/env bash
# Build-stage fixture for `tests/cyclone_slirp_pair.rs` (issue 1251): the
# Cyclone backend's `ros2_pub` test binary, which that test runs as the "guest"
# behind slirp4netns.
#
#   build.sh <out-dir>
#
# issue 1903 — this is the `fixture-script` builder's script for the
# `cyclone_slirp_guest` `[[compile_check_fixture]]` row. The test used to
# resolve `packages/rmw/cyclonedds/nros-rmw-cyclonedds/build/tests/
# nros_rmw_cyclonedds_ros2_pub` through `require_prebuilt_artifact(..,
# "just cyclonedds build-rmw")`, a recipe no fixture lane runs (only
# `check rmw-cyclonedds` did). The path matched no manifest row, so the test
# could not lane-skip either, and `just test linux` failed it as a broken
# fixture promise. A row puts it in the build every fixture lane runs
# (`build-compile-check-fixtures`), and `.inputsig` covers what it read.
#
# Into <out-dir>:
#   nros_rmw_cyclonedds_ros2_pub   the guest binary (static ddsc, no ROS libs)
#   cmake/                         its own CMake tree — NOT the one
#                                  `just cyclonedds build-rmw` uses, so the two
#                                  never fight over a generator or a cache
#   cmake-deps.d                   ninja's recorded compile inputs, as Make
#                                  dep-info, so the row's signature covers the
#                                  backend and Cyclone sources the build READ
#                                  (ninja keeps them in `.ninja_deps` and deletes
#                                  the compilers' own depfiles)
#
# Configure flags are the ones `just cyclonedds build-rmw` passes; the IDL step
# needs `rosidl_adapter` (a ROS install or `nros setup --source rosidl`), and a
# host without it fails here rather than producing nothing (issue 1758).
set -euo pipefail

out_dir="${1:?usage: build.sh <out-dir>}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../../../.." && pwd)"
proj="$repo_root/packages/rmw/cyclonedds/nros-rmw-cyclonedds"
msg_to_idl="$repo_root/scripts/cyclonedds/msg_to_cyclone_idl.py"
target="nros_rmw_cyclonedds_ros2_pub"
bld="$out_dir/cmake"

echo "== cyclone-slirp-guest fixture: $target =="
mkdir -p "$out_dir"
rm -f "$out_dir/$target" "$out_dir/cmake-deps.d"

# nros-cmake-prefix-exempt: the CycloneDDS RMW's own C/C++ project — it builds
# no Rust and never calls corrosion_import_crate().
cmake -S "$proj" -B "$bld" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON \
    -DCYCLONEDDS_SOURCE_DIR="$repo_root/third-party/dds/cyclonedds" \
    -DENABLE_SECURITY=OFF -DENABLE_SSL=OFF -DENABLE_SHM=OFF \
    -DBUILD_SHARED_LIBS=OFF \
    -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL="$msg_to_idl" \
    -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL_SOURCE="$msg_to_idl" >/dev/null
if [ "${NROS_JOBSERVER:-}" = "1" ]; then
    cmake --build "$bld" --target "$target"
else
    cmake --build "$bld" --target "$target" --parallel
fi

built="$bld/tests/$target"
[ -x "$built" ] || {
    echo "FAIL: cmake reported success but $built is absent" >&2
    exit 1
}
cp -f "$built" "$out_dir/$target"

# Ninja records each object's inputs relative to the build dir; `dep-closure.py`
# resolves a relative entry against the repo root or the `.d` file's own dir, so
# absolutize here. One `<obj>: <dep>` rule per line, spaces escaped.
ninja -C "$bld" -t deps | awk -v b="$bld" '
    /^[^ \t].*: #deps/ { obj = $1; sub(/:$/, "", obj); next }
    /^[ \t]+[^ \t]/ {
        p = $0; sub(/^[ \t]+/, "", p)
        if (substr(p, 1, 1) != "/") p = b "/" p
        gsub(/ /, "\\ ", p)
        print obj ": " p
    }
' > "$out_dir/cmake-deps.d"
[ -s "$out_dir/cmake-deps.d" ] || {
    echo "FAIL: ninja recorded no compile inputs for $target — the row's signature would cover nothing it compiled" >&2
    exit 1
}
echo "   built $out_dir/$target ($(wc -l < "$out_dir/cmake-deps.d") recorded inputs)"
