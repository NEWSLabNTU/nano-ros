# NanoRosBootstrap.cmake — INTERNAL to `find_package(nano_ros)` (RFC-0048).
#
# phase-287 W8: the phase-287 W1 public spelling (`include(NanoRosBootstrap)`
# + `nano_ros_bootstrap()` + `nano_ros_link()`) is RETIRED — every consumer
# opens with `find_package(nano_ros REQUIRED)` and the role verbs
# (`nano_ros_add_executable` / `nano_ros_add_node`). The machinery survives
# here as the config's internals: `_nros_bootstrap` (root resolve + workspace
# import + hidden RMW/CXX enable) and `_nros_link` (auto-link the generated
# interface libs + platform), called from `nano_rosConfig.cmake` and
# `cmake/NanoRosVerbs.cmake` only.

include_guard(GLOBAL)

# --------------------------------------------------------------------------
# _nros_bootstrap([ROOT <path>])
#
# Resolve + import nano-ros for a standalone (or copied-out) leaf. Idempotent;
# a no-op inside a workspace that already imported nano-ros. After this call the
# app helpers (`nano_ros_entry`, `nros_find_interfaces`, `_nros_link`, …) and
# `NROS_RMW` are available, and CXX is enabled iff the selected RMW needs it.
# --------------------------------------------------------------------------
macro(_nros_bootstrap)
    cmake_parse_arguments(_NRB "" "ROOT" "" ${ARGN})

    if(_NRB_ROOT AND NOT _NRB_ROOT STREQUAL "")
        set(NANO_ROS_ROOT "${_NRB_ROOT}")
    endif()
    if(NOT DEFINED NANO_ROS_ROOT OR NANO_ROS_ROOT STREQUAL "")
        message(FATAL_ERROR
            "_nros_bootstrap: NANO_ROS_ROOT is not set. Pass "
            "-DNANO_ROS_ROOT=<nano-ros checkout>, export NROS_REPO_DIR "
            "(source activate.sh), or call _nros_bootstrap(ROOT <path>).")
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
# _nros_link(<target>)
#
# Link the app: every generated interface library (`<pkg>__nano_ros_<lang>`,
# accumulated by `nros_find_interfaces` in the `NROS_GENERATED_INTERFACE_LIBS`
# directory property) plus the platform/board link. Replaces the per-leaf
# `target_link_libraries(<t> PRIVATE <pkg>__nano_ros_c)` + `nros_platform_link_app(<t>)`
# pair — the user no longer names the generated msg libs by hand.
# --------------------------------------------------------------------------
function(_nros_link target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR "_nros_link: '${target}' is not a target.")
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
