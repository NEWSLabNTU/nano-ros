# NanoRosBootstrap.cmake — the one entry point a nano-ros example / user package
# includes. Folds everything an app-level CMakeLists used to open with — root
# resolution, the workspace helper include + per-pkg guard, and the
# RMW-conditional CXX enable — so a leaf reads the same for every RMW and
# platform (issue #171 D5, phase-287 W1).
#
# Usage in a leaf CMakeLists.txt (uniform, do not hand-edit the prelude):
#
#     cmake_minimum_required(VERSION 3.22)
#     project(c_talker LANGUAGES C CXX)
#     include("${NANO_ROS_ROOT}/cmake/NanoRosBootstrap.cmake")
#     nano_ros_bootstrap()
#
#     nros_find_interfaces(LANGUAGE C)
#     nano_ros_entry(NAME c_talker SOURCES src/main.c DEPLOY native)
#     nano_ros_link(c_talker)
#
# `NANO_ROS_ROOT` is resolved by the leaf's prelude (the copy-out contract:
# `-DNANO_ROS_ROOT` cache var → `$NROS_REPO_DIR` env → in-tree walk-up). This
# file only needs it to have located itself; `nano_ros_bootstrap()` re-resolves
# through the canonical `nano_ros_workspace_pkg_guard()` chain so the value is
# authoritative regardless of how the prelude found it.

include_guard(GLOBAL)

# --------------------------------------------------------------------------
# nano_ros_bootstrap([ROOT <path>])
#
# Resolve + import nano-ros for a standalone (or copied-out) leaf. Idempotent;
# a no-op inside a workspace that already imported nano-ros. After this call the
# app helpers (`nano_ros_entry`, `nros_find_interfaces`, `nano_ros_link`, …) and
# `NROS_RMW` are available, and CXX is enabled iff the selected RMW needs it.
# --------------------------------------------------------------------------
macro(nano_ros_bootstrap)
    cmake_parse_arguments(_NRB "" "ROOT" "" ${ARGN})

    if(_NRB_ROOT AND NOT _NRB_ROOT STREQUAL "")
        set(NANO_ROS_ROOT "${_NRB_ROOT}")
    endif()
    if(NOT DEFINED NANO_ROS_ROOT OR NANO_ROS_ROOT STREQUAL "")
        message(FATAL_ERROR
            "nano_ros_bootstrap: NANO_ROS_ROOT is not set. Pass "
            "-DNANO_ROS_ROOT=<nano-ros checkout>, export NROS_REPO_DIR "
            "(source activate.sh), or call nano_ros_bootstrap(ROOT <path>).")
    endif()

    # The workspace helper carries the canonical root/RMW resolution + the
    # add_subdirectory import. Pull it in once and run the per-package guard —
    # a no-op if a parent workspace already imported nano-ros.
    if(NOT COMMAND nano_ros_workspace_pkg_guard)
        include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
    endif()
    nano_ros_workspace_pkg_guard(NANO_ROS_ROOT "${NANO_ROS_ROOT}")

    # RMW/CXX micro-option, hidden: the CycloneDDS backend is C++ (operator
    # new/delete, std::nothrow), so a C app linking it needs CXX enabled in this
    # directory scope. Derive it from the resolved RMW instead of making every
    # leaf hand-write the branch.
    if(NROS_RMW STREQUAL "cyclonedds")
        enable_language(CXX)
    endif()
endmacro()

# --------------------------------------------------------------------------
# nano_ros_link(<target>)
#
# Link the app: every generated interface library (`<pkg>__nano_ros_<lang>`,
# accumulated by `nros_find_interfaces` in the `NROS_GENERATED_INTERFACE_LIBS`
# directory property) plus the platform/board link. Replaces the per-leaf
# `target_link_libraries(<t> PRIVATE <pkg>__nano_ros_c)` + `nros_platform_link_app(<t>)`
# pair — the user no longer names the generated msg libs by hand.
# --------------------------------------------------------------------------
function(nano_ros_link target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR "nano_ros_link: '${target}' is not a target.")
    endif()

    get_directory_property(_nros_iface_libs NROS_GENERATED_INTERFACE_LIBS)
    foreach(_lib IN LISTS _nros_iface_libs)
        if(TARGET ${_lib})
            target_link_libraries(${target} PRIVATE ${_lib})
        endif()
    endforeach()

    # Platform/board link (kernel/netstack/startup for embedded; libc glue for
    # native). Only when the platform overlay defined it AND nano_ros_entry did
    # not already perform it for this target.
    if(COMMAND nros_platform_link_app)
        nros_platform_link_app_deferred(${target})
    endif()
endfunction()
