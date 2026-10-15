# shellcheck shell=bash
# Configure a CMake project that sits OUTSIDE this checkout — issue 1925.
#
# A gate that copies a project out (`check-template-copy-out`: a template;
# `check-scaffold-builds`: an `nros new` scaffold) does so precisely so the
# project cannot walk up into the checkout. That leaves `find_package(nano_ros)`
# with exactly ONE input, and a bare `cmake -S <copy>` takes it from the
# CALLER's environment:
#
#   * no `nano_ros_ROOT` (a clean shell — RFC-0103 D6 retired the export, and
#     issue 1840 unsets an inherited foreign one) -> "Could not find a package
#     configuration file provided by nano_ros";
#   * `nano_ros_ROOT` inherited from the shell a linked worktree was spawned in
#     -> the gate configures the PARENT checkout's CMake tree and reports on it
#     (issue 1280's class). Measured: `nano_ros_DIR:PATH=<main checkout>`.
#
# `just` hid both, because `just/sdk-env.just` exports this checkout's root to
# every recipe; run directly, the scripts depended on an export they never
# named.
#
# So the root is named EXPLICITLY, the way the book tells a user to configure a
# copied-out leaf (`-Dnano_ros_ROOT=…`, RFC-0103 D6 rung 1), and it is THIS
# checkout — the one this file lives in, located by the one checkout-marker walk
# (`checkout-paths.sh`), never by an environment variable. The inherited
# `nano_ros_ROOT` / `NROS_REPO_DIR` are also dropped from the configure's
# environment, so neither CMake's `<Pkg>_ROOT` env fallback nor anything the
# configure spawns can reach the other tree.
#
# `check-copy-out-cmake-root` refuses a bare `cmake` configure in a gate script
# and runs this helper against a stub `cmake` under an inherited foreign root.

_nros_copy_out_cmake_lib="$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd -P)"
# shellcheck source=scripts/lib/checkout-paths.sh
. "$_nros_copy_out_cmake_lib/checkout-paths.sh"

# nros_copy_out_cmake_root
#
# Print the checkout this helper belongs to — the one a copy-out gate builds.
nros_copy_out_cmake_root() {
    nros_checkout_root "$_nros_copy_out_cmake_lib"
}

# nros_copy_out_cmake_configure <cmake configure args…>
#
# `cmake <args…> -Dnano_ros_ROOT=<this checkout>`, with no inherited
# `nano_ros_ROOT` / `NROS_REPO_DIR` in its environment. Returns cmake's status;
# fails (2) without running cmake when this file is not inside a checkout.
nros_copy_out_cmake_configure() {
    local root
    root="$(nros_copy_out_cmake_root)"
    if [ -z "$root" ]; then
        echo "copy-out-cmake: $_nros_copy_out_cmake_lib is in no nano-ros checkout" >&2
        return 2
    fi
    env -u nano_ros_ROOT -u NROS_REPO_DIR cmake "$@" "-Dnano_ros_ROOT=$root"
}
