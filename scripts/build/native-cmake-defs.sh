#!/usr/bin/env bash
# Sourced, never run. The ONE spelling of the cmake definitions every native
# (linux) C/C++ fixture leaf is configured with.
#
# Issue 1846. `just native build-fixture-extras` wrote these twice by hand and
# exported them as `NROS_CMAKE_EXTRA_DEFS` before calling `fixtures-build.sh`.
# The live-peer host job called `fixtures-build.sh linux <lang> <rmw>`
# DIRECTLY and passed none. So its 42 C/C++ leaf configures ran with no
# `NROS_SHARED_CARGO_ROOT`, and each leaf compiled the whole Rust stack into
# its own cargo directory (about 1-3 GB each), which filled a 110 GB runner.
# They also ran without `CMAKE_BUILD_TYPE=Release` or the codegen tool, so
# that lane built these fixtures in a different shape from the one `just
# native` builds. `fixtures-build.sh` now applies this itself when the caller
# states nothing, and the recipe calls the same function, so the two cannot
# drift.
#
# Needs `nros_build_dir` + `NROS_KIND_CORROSION_CARGO` (scripts/build/build-root.sh).

# nros_native_cmake_extra_defs <codegen tool> <repo root>
nros_native_cmake_extra_defs() {
    local codegen_tool="${1:?nros_native_cmake_extra_defs: codegen tool}"
    local repo_root="${2:?nros_native_cmake_extra_defs: repo root}"
    local shared
    shared="$(nros_build_dir "$NROS_KIND_CORROSION_CARGO" native)" || return 1
    [ -n "$shared" ] || {
        echo "nros_native_cmake_extra_defs: empty shared cargo root -- is build-root.sh sourced?" >&2
        return 1
    }
    printf '%s' "-DNROS_SHARED_CARGO_ROOT=$shared -DCMAKE_BUILD_TYPE=Release" \
        " -DNROS_BUILD_CODEGEN=OFF -D_NANO_ROS_CODEGEN_TOOL=$codegen_tool" \
        " -DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON" \
        " -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=$repo_root/scripts/cyclonedds/msg_to_cyclone_idl.py"
}
