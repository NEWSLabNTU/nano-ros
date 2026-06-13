# cmake/NanoRosWorkspace.cmake — Phase 219.I
#
# Workspace-root + per-pkg cmake-fn pair that gives a pure-C/C++
# multi-pkg workspace the same `[workspace]` discipline cargo gives a
# Rust workspace:
#
# Workspace-root CMakeLists.txt:
#
#     cmake_minimum_required(VERSION 3.22)
#     project(my_ws LANGUAGES C CXX)
#     nano_ros_workspace(
#         NANO_ROS_ROOT /path/to/nano-ros     # or set
#                                             # -DNANO_ROS_ROOT=… or
#                                             # let the auto-walk find
#                                             # `nros-sdk-index.toml`
#         BACKEND       zenoh                 # zenoh | xrce | cyclonedds
#         PLATFORM      posix                 # posix | … (default posix)
#         SUBDIRS       src/talker_pkg
#                       src/listener_pkg
#                       src/cpp_entry
#     )
#
# Per-pkg subdir CMakeLists.txt (Node + Entry pkgs):
#
#     cmake_minimum_required(VERSION 3.22)
#     project(talker_pkg LANGUAGES C CXX)
#     nano_ros_workspace_pkg_guard()
#     nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)
#     nano_ros_node_register(NAME talker
#                            CLASS talker_pkg::Talker
#                            SOURCES src/Talker.cpp
#                            DEPLOY native)
#
# `nano_ros_workspace_pkg_guard()` is the dual:
#
#   * Inside a workspace — returns immediately (the workspace root
#     already imported nano-ros + included the cmake-fn helpers).
#   * Standalone — replicates the workspace-root body so the same
#     subdir CMakeLists builds solo (preserves the single-pkg
#     copy-out path).
#
# Net effect: every Node/Entry pkg CMakeLists is one canonical shape;
# users decide between "workspace" and "standalone" at the root, not
# in every leaf.
#
# Phase 219 workflow review Gaps 1 + 2 closed.

if(DEFINED _NROS_WORKSPACE_INCLUDED)
    return()
endif()
set(_NROS_WORKSPACE_INCLUDED TRUE)

# ---------------------------------------------------------------------------
# Helper — walk up from <start> looking for the `nros-sdk-index.toml`
# sentinel that marks every nano-ros checkout root. Writes the
# discovered path to <out_var> (PARENT_SCOPE), or `_NROS_ROOT-NOTFOUND`
# when nothing matches.
# ---------------------------------------------------------------------------
function(_nros_find_root start out_var)
    set(_dir "${start}")
    set(_max_walk 16)            # bounded — never walk past `/`
    while(_max_walk GREATER 0)
        if(EXISTS "${_dir}/nros-sdk-index.toml")
            set(${out_var} "${_dir}" PARENT_SCOPE)
            return()
        endif()
        get_filename_component(_parent "${_dir}" DIRECTORY)
        if(_parent STREQUAL _dir)
            break()             # reached `/`
        endif()
        set(_dir "${_parent}")
        math(EXPR _max_walk "${_max_walk} - 1")
    endwhile()
    set(${out_var} "_NROS_ROOT-NOTFOUND" PARENT_SCOPE)
endfunction()

# ---------------------------------------------------------------------------
# Resolve the nano-ros root from (in priority order):
#   1. explicit `<NANO_ROS_ROOT>` arg (workspace-root call),
#   2. `-DNANO_ROS_ROOT=…` cache var,
#   3. `NANO_ROS_ROOT` env var,
#   4. auto-walk from <start_dir> for `nros-sdk-index.toml`.
# Writes resolved path to <out_var> (PARENT_SCOPE) or errors via
# FATAL_ERROR with a hint when nothing resolves.
# ---------------------------------------------------------------------------
function(_nros_resolve_root explicit start_dir out_var)
    if(explicit AND NOT explicit STREQUAL "")
        set(${out_var} "${explicit}" PARENT_SCOPE)
        return()
    endif()
    if(DEFINED NANO_ROS_ROOT AND NOT NANO_ROS_ROOT STREQUAL "")
        set(${out_var} "${NANO_ROS_ROOT}" PARENT_SCOPE)
        return()
    endif()
    if(DEFINED ENV{NANO_ROS_ROOT} AND NOT "$ENV{NANO_ROS_ROOT}" STREQUAL "")
        set(${out_var} "$ENV{NANO_ROS_ROOT}" PARENT_SCOPE)
        return()
    endif()
    _nros_find_root("${start_dir}" _walked)
    if(NOT _walked STREQUAL "_NROS_ROOT-NOTFOUND")
        set(${out_var} "${_walked}" PARENT_SCOPE)
        return()
    endif()
    message(FATAL_ERROR
        "nano-ros: cannot locate nano-ros root from '${start_dir}'.\n"
        "  Pass NANO_ROS_ROOT to `nano_ros_workspace()` or set the\n"
        "  -DNANO_ROS_ROOT=<path> cache var, or run from inside a tree\n"
        "  that contains `nros-sdk-index.toml`.")
endfunction()

# ---------------------------------------------------------------------------
# Internal one-shot import: `add_subdirectory(<nano-ros>)` + include the
# cmake-fn helpers. Idempotent — second call is a no-op.
# ---------------------------------------------------------------------------
function(_nros_import_once nano_ros_root)
    if(TARGET NanoRos::NanoRosCpp OR TARGET NanoRos::NanoRos)
        return()
    endif()
    # The cmake-fn modules need NANO_ROS_PLATFORM / NANO_ROS_RMW visible
    # to the `add_subdirectory()` body; callers must have set them on
    # PARENT_SCOPE before calling _nros_import_once (workspace-root does
    # this in nano_ros_workspace(); standalone does this in the guard).
    # We rely on directory-scope visibility — both call sites set them
    # at top of their function bodies, so child directories inherit.
    add_subdirectory("${nano_ros_root}" "${CMAKE_BINARY_DIR}/nano_ros")
    include("${nano_ros_root}/cmake/NanoRosNodeRegister.cmake")
    include("${nano_ros_root}/cmake/NanoRosEntry.cmake")
endfunction()

# ---------------------------------------------------------------------------
# Public — `nano_ros_workspace(SYSTEM … BACKEND … PLATFORM … SUBDIRS …)`
# ---------------------------------------------------------------------------
function(nano_ros_workspace)
    cmake_parse_arguments(_NRW
        ""
        "SYSTEM;BACKEND;PLATFORM;NANO_ROS_ROOT"
        "SUBDIRS"
        ${ARGN})

    # Defaults: backend = zenoh, platform = posix.
    if(NOT _NRW_BACKEND)
        set(_NRW_BACKEND zenoh)
    endif()
    if(NOT _NRW_PLATFORM)
        set(_NRW_PLATFORM posix)
    endif()

    # Resolve the nano-ros root (priority chain in _nros_resolve_root).
    _nros_resolve_root("${_NRW_NANO_ROS_ROOT}"
                       "${CMAKE_CURRENT_SOURCE_DIR}"
                       _nros_root)

    # Stamp the resolution so subdirs + the per-pkg guard reuse it
    # without re-walking. PARENT_SCOPE here = the workspace-root scope.
    set(NANO_ROS_ROOT     "${_nros_root}"      PARENT_SCOPE)
    set(NANO_ROS_PLATFORM "${_NRW_PLATFORM}"   PARENT_SCOPE)
    set(NANO_ROS_RMW      "${_NRW_BACKEND}"    PARENT_SCOPE)
    set(NROS_RMW          "${_NRW_BACKEND}"    PARENT_SCOPE)

    # Also set them in the local fn scope so _nros_import_once's
    # add_subdirectory body sees them directly (PARENT_SCOPE writes
    # don't reach the current fn frame).
    set(NANO_ROS_ROOT     "${_nros_root}")
    set(NANO_ROS_PLATFORM "${_NRW_PLATFORM}")
    set(NANO_ROS_RMW      "${_NRW_BACKEND}")
    set(NROS_RMW          "${_NRW_BACKEND}")

    _nros_import_once("${_nros_root}")

    # Optional: workspace metadata for `nros plan` consumption. SYSTEM
    # arg threads through; if absent we skip — workspaces without a
    # Bringup pkg are valid (single-Entry self-bringup mode).
    if(_NRW_SYSTEM)
        include("${_nros_root}/cmake/nano_ros_workspace_metadata.cmake")
        nano_ros_workspace_metadata(SYSTEM "${_NRW_SYSTEM}"
                                    WORKSPACE_ROOT "${CMAKE_SOURCE_DIR}")
    endif()

    foreach(_sub IN LISTS _NRW_SUBDIRS)
        if(IS_ABSOLUTE "${_sub}")
            add_subdirectory("${_sub}")
        else()
            add_subdirectory("${CMAKE_SOURCE_DIR}/${_sub}")
        endif()
    endforeach()

    # Phase 241 W11 (Option D) — if this configure contains a Rust Node pkg, synthesise the
    # per-configure runtime umbrella (nros-cpp + all workspace Rust nodes, one staticlib)
    # and re-point NanoRos::NanoRosCpp at it. No-op for pure-C / pure-C++ workspaces. Runs
    # AFTER the SUBDIRS loop so nros-metadata.json lists every registered node; the umbrella
    # archive swap is an INTERFACE property edit, evaluated at generate time.
    include("${_nros_root}/cmake/NanoRosRuntimeCrate.cmake")
    nros_synth_runtime_umbrella(BACKEND "${_NRW_BACKEND}" PLATFORM "${_NRW_PLATFORM}")
endfunction()

# ---------------------------------------------------------------------------
# Public — `nano_ros_workspace_pkg_guard([NANO_ROS_ROOT <path>])`
#
# Top-of-CMakeLists call in every Node + Entry pkg subdir. Inside a
# workspace it is a no-op; standalone it bootstraps the same way the
# workspace root would.
# ---------------------------------------------------------------------------
function(nano_ros_workspace_pkg_guard)
    if(TARGET NanoRos::NanoRosCpp OR TARGET NanoRos::NanoRos)
        return()
    endif()

    cmake_parse_arguments(_NRG
        ""
        "NANO_ROS_ROOT;BACKEND;PLATFORM"
        ""
        ${ARGN})

    if(NOT _NRG_BACKEND)
        if(NROS_RMW)
            set(_NRG_BACKEND "${NROS_RMW}")
        else()
            set(_NRG_BACKEND zenoh)
        endif()
    endif()
    if(NOT _NRG_PLATFORM)
        if(NANO_ROS_PLATFORM)
            set(_NRG_PLATFORM "${NANO_ROS_PLATFORM}")
        else()
            set(_NRG_PLATFORM posix)
        endif()
    endif()

    _nros_resolve_root("${_NRG_NANO_ROS_ROOT}"
                       "${CMAKE_CURRENT_SOURCE_DIR}"
                       _nros_root)

    # Direct-scope sets so the cmake-fn helpers + add_subdirectory body
    # see them. PARENT_SCOPE keeps them visible to the rest of the
    # pkg's CMakeLists too.
    set(NANO_ROS_ROOT     "${_nros_root}"     PARENT_SCOPE)
    set(NANO_ROS_PLATFORM "${_NRG_PLATFORM}"  PARENT_SCOPE)
    set(NANO_ROS_RMW      "${_NRG_BACKEND}"   PARENT_SCOPE)
    set(NROS_RMW          "${_NRG_BACKEND}"   PARENT_SCOPE)
    set(NANO_ROS_ROOT     "${_nros_root}")
    set(NANO_ROS_PLATFORM "${_NRG_PLATFORM}")
    set(NANO_ROS_RMW      "${_NRG_BACKEND}")
    set(NROS_RMW          "${_NRG_BACKEND}")

    _nros_import_once("${_nros_root}")
endfunction()
