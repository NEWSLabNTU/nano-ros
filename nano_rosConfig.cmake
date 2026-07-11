# nano_rosConfig.cmake — RFC-0048 (phase-287 W3): the ament-shape entry point.
#
# A nano-ros C/C++ package opens with `find_package(nano_ros REQUIRED)`, exactly
# like an ament_cmake package opens with `find_package(rclcpp REQUIRED)`. Because
# nano-ros is a SOURCE distribution (no install prefix, no crates.io — #171 D2),
# resolution is source-backed: this config lives at the checkout root and is
# located via `nano_ros_ROOT` (which `activate.sh` exports; a copy-out passes
# `-Dnano_ros_ROOT=<checkout>` or lets a `nros setup` CMakePreset carry it — W5).
#
# What `find_package(nano_ros)` wires up:
#   1. imports nano-ros (add_subdirectory of the checkout — the machinery
#      phase-287 W1 shipped as `nano_ros_bootstrap()`, now a config internal),
#      publishing NanoRos::NanoRos / NanoRos::NanoRosCpp and enabling CXX iff the
#      resolved RMW needs it;
#   2. prepends the find-package stubs so `find_package(<msg_pkg>)` resolves to
#      nano-ros codegen (RFC-0048 §2) and defines the ament shims
#      (`ament_target_dependencies`, `ament_package`, …);
#   3. defines the two role verbs `nano_ros_add_executable` / `nano_ros_add_node`
#      (RFC-0048 §3) and `nano_ros_generate_interfaces` (§5).
#
# The consuming `CMakeLists.txt` is byte-identical across every platform; the
# per-package platform/board/RMW delta lives in `package.xml` `<export>` (§4,
# wired in W4). Until W4 lands, deploy defaults to `native` and platform/RMW come
# from the `NANO_ROS_PLATFORM` / `NANO_ROS_RMW` cache vars as before.

# This config file sits at the checkout root, so its own directory is the root.
set(NANO_ROS_ROOT "${CMAKE_CURRENT_LIST_DIR}")

# `find_package(<msg_pkg>)` here VALIDATES the dependency and satisfies the ament
# `REQUIRED` line; it does NOT itself codegen. The authoritative generation is
# driven by the `nano_ros_add_*` verb, which knows the leaf's language (inferred
# from its sources) and reads the `package.xml` `<depend>` closure via `nros
# codegen resolve-deps`. Two reasons this is the right split: (a) the CLI
# resolves well-known ROS packages with no in-tree bundle or sourced ROS install,
# whereas the find-stub's cmake-glob resolution cannot; (b) generating a C++
# interface lib from a `find_package` line — before any source names a language —
# would drag CXX target-features into a C leaf's scope and force a CPP FFI build
# for a C example. Setting this flag keeps `find_package(<msg>)` a pure validate.
set(NROS_FIND_PACKAGE_VALIDATE_ONLY TRUE)

# --- 1. import nano-ros ------------------------------------------------------
include("${NANO_ROS_ROOT}/cmake/NanoRosBootstrap.cmake")
nano_ros_bootstrap(ROOT "${NANO_ROS_ROOT}")

# --- 2. find-package stubs + ament shims -------------------------------------
# NrosRclcppCompat prepends `cmake/compat/stubs/` to CMAKE_MODULE_PATH so a stock
# `find_package(std_msgs REQUIRED)` line routes into nano-ros codegen, and defines
# the `ament_target_dependencies` / `ament_package` shims the ament shape uses.
include("${NANO_ROS_ROOT}/cmake/compat/NrosRclcppCompat.cmake")

# --- 3. role verbs + interface generation ------------------------------------
include("${NANO_ROS_ROOT}/cmake/NanoRosVerbs.cmake")

set(nano_ros_FOUND TRUE)
