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

# --- Zephyr arm (287-W6) ------------------------------------------------------
# On Zephyr, nano-ros is already IN the build as a west/Zephyr module (the
# `zephyr/` dir, Kconfig-selected — it owns the runtime import, the RMW
# feature choice, and `nros_find_interfaces`). `find_package(nano_ros)` here
# must NOT `add_subdirectory` the checkout again; it only supplies the ament
# surface: the package.xml tuple, the find-package stubs, and the verbs.
# The leaf keeps `find_package(Zephyr)` first — Zephyr owns the build, so a
# Zephyr leaf is deliberately NOT byte-identical to native (RFC-0048 §3).
if(DEFINED ZEPHYR_BASE AND TARGET zephyr_interface)
    include("${NANO_ROS_ROOT}/cmake/NanoRosPackageXml.cmake")
    nano_ros_read_package_export()
    # deploy="zephyr" (board/RMW stay with Zephyr's own BOARD/Kconfig axes).
    set(NANO_ROS_PLATFORM zephyr)
    set(NROS_DEPLOY "${NANO_ROS_EXPORT_DEPLOY}")
    set(NROS_BOARD  "${NANO_ROS_EXPORT_BOARD}")
    set(NROS_FIND_PACKAGE_VALIDATE_ONLY TRUE)
    # find_package(<msg_pkg>) validate-stubs, WITHOUT the full compat module
    # (NrosRclcppCompat asserts NanoRos::NanoRosCpp, which a C-only Zephyr
    # image doesn't define).
    if(NOT "${NANO_ROS_ROOT}/cmake/compat/stubs" IN_LIST CMAKE_MODULE_PATH)
        list(PREPEND CMAKE_MODULE_PATH "${NANO_ROS_ROOT}/cmake/compat/stubs")
    endif()
    include("${NANO_ROS_ROOT}/cmake/compat/stubs/_NrosFindRosMsgPackage.cmake")
    include("${NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")
    include("${NANO_ROS_ROOT}/cmake/NanoRosVerbs.cmake")
    set(nano_ros_FOUND TRUE)
    return()
endif()

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

# --- package.xml is the SSoT (RFC-0048 §4) -----------------------------------
# Read the consumer's `<export><nano_ros deploy= board= rmw=/></export>` tuple
# NOW, before importing nano-ros, so deploy→NANO_ROS_PLATFORM and rmw→
# NANO_ROS_RMW reach the `add_subdirectory` body. This is what lets the leaf's
# CMakeLists stay byte-identical across platforms — the delta is one line of
# package.xml. Explicit `-DNANO_ROS_PLATFORM` / `-DNANO_ROS_RMW` still win (they
# are only set below when the caller left them at the default).
include("${NANO_ROS_ROOT}/cmake/NanoRosPackageXml.cmake")
nano_ros_read_package_export()
if(NANO_ROS_EXPORT_FOUND)
    # Tuple values go into the CACHE, not directory scope. The imported root
    # CMakeLists declares `NANO_ROS_PLATFORM`/`NANO_ROS_RMW` with cached posix/
    # zenoh defaults; a directory-scope set here would shadow them only on the
    # FIRST configure — on any reconfigure the stale cached default is visible
    # to the `NOT NANO_ROS_*` guards, the tuple is skipped, and a freertos leaf
    # silently reconfigures as posix (Threads_FOUND death on a cross
    # toolchain). Writing the tuple into the cache on first parse makes
    # reconfigures stable; an explicit `-D` still wins (the guard sees it).
    if(NANO_ROS_EXPORT_DEPLOY AND NOT NANO_ROS_PLATFORM)
        _nros_deploy_to_platform("${NANO_ROS_EXPORT_DEPLOY}" _nros_tuple_platform)
        set(NANO_ROS_PLATFORM "${_nros_tuple_platform}" CACHE STRING
            "nano-ros platform (from the package.xml <nano_ros deploy=…> tuple)")
    endif()
    if(NANO_ROS_EXPORT_RMW AND NOT NANO_ROS_RMW)
        set(NANO_ROS_RMW "${NANO_ROS_EXPORT_RMW}" CACHE STRING
            "RMW backend (from the package.xml <nano_ros rmw=…> tuple)")
    endif()
    if(NANO_ROS_EXPORT_BOARD AND NOT NANO_ROS_BOARD)
        set(NANO_ROS_BOARD "${NANO_ROS_EXPORT_BOARD}" CACHE STRING
            "nano-ros board (from the package.xml <nano_ros board=…> tuple)")
    endif()
    # The verbs pick DEPLOY/BOARD up from these directory-scope vars.
    set(NROS_DEPLOY "${NANO_ROS_EXPORT_DEPLOY}")
    set(NROS_BOARD  "${NANO_ROS_EXPORT_BOARD}")
endif()

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
