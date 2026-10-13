# nano_rosConfig.cmake — RFC-0048 (phase-287 W3): the ament-shape entry point.
#
# A nano-ros C/C++ package opens with `find_package(nano_ros REQUIRED)`, exactly
# like an ament_cmake package opens with `find_package(rclcpp REQUIRED)`. Because
# nano-ros is a SOURCE distribution (no install prefix, no crates.io — #171 D2),
# resolution is source-backed: this config lives at the checkout root and is
# located via `-Dnano_ros_ROOT=<root>`, which `nros build` and the CMakePresets
# `nros init` writes pass; a raw `cmake` names it (no shell exports it —
# RFC-0103 D6, amended 2026-10-12).
#
# What `find_package(nano_ros)` wires up:
#   1. imports nano-ros (add_subdirectory of the checkout — the machinery
#      phase-287 W1 shipped as `nano_ros_bootstrap()`, now a config internal),
#      publishing NanoRos::NanoRos / NanoRos::NanoRosCpp and enabling CXX iff the
#      resolved RMW needs it;
#   2. prepends the find modules (`cmake/find/`) so `find_package(<msg_pkg>)` resolves to
#      nano-ros codegen (RFC-0048 §2) and defines the ament verbs
#      (`ament_target_dependencies`, `ament_package`, …);
#   3. defines the two role verbs `nano_ros_add_executable` / `nano_ros_add_node`
#      (RFC-0048 §3) and `nano_ros_generate_interfaces` (§5).
#
# The consuming `CMakeLists.txt` is byte-identical across every platform; the
# per-package platform/board/RMW delta lives in `package.xml` `<export>` (§4,
# wired in W4). Until W4 lands, deploy defaults to `native` and platform/RMW come
# from the `NROS_PLATFORM` / `NROS_RMW` cache vars as before.

# This config file sits at the checkout root, so its own directory is the root.
# phase-484 W7b — adopt any deprecated `NANO_ROS_*` name the project set,
# BEFORE the root is stated: this config file's own directory IS the root, so a
# deprecated root alias the project passed (also CMake's find hint) is adopted and then
# superseded here rather than compared with it.
include("${CMAKE_CURRENT_LIST_DIR}/cmake/NanoRosDeprecatedNames.cmake")
nros_adopt_deprecated_names()
set(NROS_REPO_DIR "${CMAKE_CURRENT_LIST_DIR}")

# --- Zephyr arm (287-W6) ------------------------------------------------------
# On Zephyr, nano-ros is already IN the build as a west/Zephyr module (the
# `zephyr/` dir, Kconfig-selected — it owns the runtime import, the RMW
# feature choice, and `nros_find_interfaces`). `find_package(nano_ros)` here
# must NOT `add_subdirectory` the checkout again; it only supplies the ament
# surface: the leaf's system.toml deployment, the find-package stubs, and the
# verbs. The leaf keeps `find_package(Zephyr)` first — Zephyr owns the build,
# so a Zephyr leaf is deliberately NOT byte-identical to native (RFC-0048 §3).
if(DEFINED ZEPHYR_BASE AND TARGET zephyr_interface)
    include("${NROS_REPO_DIR}/cmake/NanoRosPackageXml.cmake")
    nano_ros_read_package_export()
    # phase-445 W3 (RFC-0098 D3) — the leaf's system.toml names its board.
    if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/system.toml")
        include("${NROS_REPO_DIR}/cmake/NanoRosCodegenCore.cmake")
    endif()
    nano_ros_read_leaf_system()
    # deploy="zephyr" (board/RMW stay with Zephyr's own BOARD/Kconfig axes).
    set(NROS_PLATFORM zephyr)
    set(NROS_DEPLOY "${NROS_EXPORT_DEPLOY}")
    set(NROS_LEAF_IMAGE_ID "${NROS_EXPORT_IMAGE}")
    if(NOT DEFINED NROS_BOARD)  # a project's own -DNROS_BOARD wins (W7b-2)
        set(NROS_BOARD  "${NROS_EXPORT_BOARD}")
    endif()
    set(NROS_FIND_PACKAGE_VALIDATE_ONLY TRUE)
    # find_package(<msg_pkg>) validate modules, WITHOUT the full ament surface
    # (NanoRosAmentSurface asserts NanoRos::NanoRosCpp, which a C-only Zephyr
    # image doesn't define).
    if(NOT "${NROS_REPO_DIR}/cmake/find" IN_LIST CMAKE_MODULE_PATH)
        list(PREPEND CMAKE_MODULE_PATH "${NROS_REPO_DIR}/cmake/find")
    endif()
    include("${NROS_REPO_DIR}/cmake/find/_NrosFindRosMsgPackage.cmake")
    # phase-482 W3 — the one ament verb a ported package's glue needs here
    # (`ament_target_dependencies(<name> diagnostic_updater)`); it routes to
    # `app`, where `nano_ros_add_executable` compiles the sources.
    include("${NROS_REPO_DIR}/cmake/NanoRosAmentTargetDeps.cmake")
    include("${NROS_REPO_DIR}/cmake/NanoRosNodeRegister.cmake")
    # NanoRosEntry too: a Zephyr workspace-entry app uses
    # `nano_ros_add_executable(... LAUNCH ...)`, whose body calls
    # `nano_ros_entry` (both modules are re-include guarded).
    include("${NROS_REPO_DIR}/cmake/NanoRosEntry.cmake")
    include("${NROS_REPO_DIR}/cmake/NanoRosVerbs.cmake")
    set(nano_ros_FOUND TRUE)
    return()
endif()

# --- Workspace-root arm (287-W6 slice 3) -------------------------------------
# A multi-pkg workspace ROOT opens with
#     find_package(nano_ros REQUIRED COMPONENTS workspace)
#     nano_ros_workspace(BACKEND … PLATFORM … SYSTEM … SUBDIRS …)
# The root must NOT import nano-ros here: `nano_ros_workspace()` owns the
# import (it maps BACKEND→NROS_RMW *before* the add_subdirectory body
# runs — an eager import here would bake the cache-default RMW and an
# xrce/cyclonedds workspace would silently configure as zenoh). The arm only
# supplies the workspace orchestrator + the verbs; member/Entry pkgs each call
# plain `find_package(nano_ros REQUIRED)`, which no-ops onto the already
# imported targets (the bootstrap is target-guarded).
if("workspace" IN_LIST nano_ros_FIND_COMPONENTS)
    include("${NROS_REPO_DIR}/cmake/NanoRosWorkspace.cmake")
    include("${NROS_REPO_DIR}/cmake/NanoRosPackageXml.cmake")
    set(nano_ros_workspace_FOUND TRUE)
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

# --- the leaf's system.toml is the SSoT (RFC-0098 D3/D5) ---------------------
# Read the consumer's deployment NOW, before importing nano-ros, so the board's
# deploy token → NROS_PLATFORM and rmw → NROS_RMW reach the
# `add_subdirectory` body. This is what lets the leaf's CMakeLists stay
# byte-identical across platforms — the delta is the `system.toml` beside it.
# Explicit `-DNROS_PLATFORM` / `-DNROS_RMW` still win (they are only set
# below when the caller left them at the default).
#
# `nano_ros_read_package_export()` reads the package.xml's build type and
# `<nano_ros_uses>` selections, and REFUSES the retired `<nano_ros deploy=
# board= rmw=/>` tuple (RFC-0048 §4's spelling, retired by phase-445 W3b);
# `nano_ros_read_leaf_system()` then fills the NROS_EXPORT_* variables from
# `system.toml` through the `nros` CLI, the one reader.
include("${NROS_REPO_DIR}/cmake/NanoRosPackageXml.cmake")
nano_ros_read_package_export()
if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/system.toml")
    include("${NROS_REPO_DIR}/cmake/NanoRosCodegenCore.cmake")
endif()
nano_ros_read_leaf_system()
if(NROS_LEAF_SYSTEM)
    # Deployment identity (RFC-0098 D5). Directory scope, never the cache, and
    # only where no `-D` spoke: an explicit override (every RTOS fixture row
    # passes `-DNROS_ENTRY_LOCATOR`) still wins, exactly as it wins over the
    # per-platform defaults in NanoRosEntryLocator.cmake.
    if(NOT NROS_LEAF_LOCATOR STREQUAL "" AND NOT DEFINED NROS_ENTRY_LOCATOR)
        set(NROS_ENTRY_LOCATOR "${NROS_LEAF_LOCATOR}")
    endif()
    if(NOT NROS_LEAF_DOMAIN_ID STREQUAL "" AND NOT DEFINED NROS_DOMAIN_ID)
        set(NROS_DOMAIN_ID "${NROS_LEAF_DOMAIN_ID}")
    endif()
    # Issue 1142 — and what the leaf DECLARES it creates (RFC-0098 D8). This is
    # the third thing its `system.toml` answers, after the board and the
    # deployment identity: a standalone leaf has no bringup and no SystemModel,
    # so the model roads never reach it and the RMW would size its
    # queryable table from a literal. Recorded HERE, before nano-ros is
    # imported, because the delivery is deferred to the end of the top-level
    # scope (`nros_entity_facts_env_deferred`, already armed on
    # `nros_cpp-static` / `nros_c-static`) and only needs the facts to exist by
    # then. The Zephyr arm above does NOT call this: it has the Kconfig derive
    # sentinel, which is its own front-end (RFC-0049).
    include("${NROS_REPO_DIR}/cmake/NanoRosLeafEntityFacts.cmake")
    nros_record_leaf_entity_facts("${CMAKE_CURRENT_SOURCE_DIR}")
endif()
if(NROS_EXPORT_FOUND)
    # The leaf's values go into the CACHE, not directory scope. The imported
    # root CMakeLists declares `NROS_PLATFORM`/`NROS_RMW` with cached
    # posix/zenoh defaults; a directory-scope set here would shadow them only
    # on the FIRST configure — on any reconfigure the stale cached default is
    # visible to the `NOT NANO_ROS_*` guards, the leaf's value is skipped, and a
    # freertos leaf silently reconfigures as posix (Threads_FOUND death on a
    # cross toolchain). Writing them into the cache on first parse makes
    # reconfigures stable; an explicit `-D` still wins (the guard sees it).
    if(NROS_EXPORT_DEPLOY AND NOT NROS_PLATFORM)
        _nros_deploy_to_platform("${NROS_EXPORT_DEPLOY}" _nros_leaf_platform)
        set(NROS_PLATFORM "${_nros_leaf_platform}" CACHE STRING
            "nano-ros platform (derived from the leaf system.toml board)")
    endif()
    if(NROS_EXPORT_RMW AND NOT NROS_RMW)
        set(NROS_RMW "${NROS_EXPORT_RMW}" CACHE STRING
            "RMW backend (from the leaf system.toml rmw)")
    endif()
    # phase-484 W4f — the board MODULE the leaf's board names, not the alias
    # it was authored as (a package.xml-only leaf has no CMAKE_BOARD).
    if(NROS_EXPORT_CMAKE_BOARD)
        set(_nros_leaf_cmake_board "${NROS_EXPORT_CMAKE_BOARD}")
    else()
        set(_nros_leaf_cmake_board "${NROS_EXPORT_BOARD}")
    endif()
    if(_nros_leaf_cmake_board AND NOT NROS_BOARD)
        set(NROS_BOARD "${_nros_leaf_cmake_board}" CACHE STRING
            "nano-ros board module (from the leaf system.toml [image] board)")
    endif()
    # The verbs pick DEPLOY/BOARD up from these directory-scope vars.
    set(NROS_DEPLOY "${NROS_EXPORT_DEPLOY}")
    set(NROS_LEAF_IMAGE_ID "${NROS_EXPORT_IMAGE}")
    if(NOT DEFINED NROS_BOARD)  # a project's own -DNROS_BOARD wins (W7b-2)
        set(NROS_BOARD  "${NROS_EXPORT_BOARD}")
    endif()
endif()

# --- 1. import nano-ros ------------------------------------------------------
include("${NROS_REPO_DIR}/cmake/NanoRosBootstrap.cmake")
_nros_bootstrap(ROOT "${NROS_REPO_DIR}")

# --- 2. find modules + ament verbs -------------------------------------------
# NanoRosAmentSurface prepends `cmake/find/` to CMAKE_MODULE_PATH so a stock
# `find_package(std_msgs REQUIRED)` line routes into nano-ros codegen, and defines
# the `ament_target_dependencies` / `ament_package` verbs the ament shape uses.
include("${NROS_REPO_DIR}/cmake/NanoRosAmentSurface.cmake")

# --- 3. role verbs + interface generation ------------------------------------
include("${NROS_REPO_DIR}/cmake/NanoRosVerbs.cmake")

set(nano_ros_FOUND TRUE)
