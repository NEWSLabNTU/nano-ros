# nros-rmw-provision.cmake — what this backend needs resolved BEFORE its own
# `add_subdirectory()` (phase-439 W4, RFC-0094 D5).
#
# Included by the selecting build, in ITS scope, when this backend is chosen.
# The hook exists so the root `CMakeLists.txt` does not need an
# `if(NANO_ROS_RMW STREQUAL "cyclonedds")` pre-step — which is exactly the
# closed-list shape W4 deleted one block down. What a backend needs provisioned
# is the backend's business.
#
# Phase 186 — `nros_provide_cyclonedds()` resolves Cyclone: a prebuilt install
# on `CMAKE_PREFIX_PATH` wins; otherwise it self-provisions from
# `CYCLONEDDS_SOURCE_DIR`. As the project root we own `third-party/`, so default
# that to the pinned submodule — a bare cmake build then needs no
# `just cyclonedds` pre-step. A user overrides with `-DCMAKE_PREFIX_PATH` (their
# install) or `-DCYCLONEDDS_SOURCE_DIR` (their checkout).
#
# `NANO_ROS_ROOT_DIR` rather than `CMAKE_CURRENT_SOURCE_DIR`: an `include()`
# evaluates in the INCLUDING file's directory scope, so the latter would name
# the consumer's project, not this checkout. Falls back to walking up from this
# file (packages/rmw/cyclonedds/nros-rmw-cyclonedds -> the tree root) for a
# consumer that reaches the descriptor without the root having run.

if(NOT DEFINED CYCLONEDDS_SOURCE_DIR)
    if(NANO_ROS_ROOT_DIR)
        set(_nros_cyclone_tree "${NANO_ROS_ROOT_DIR}")
    else()
        get_filename_component(_nros_cyclone_tree
            "${CMAKE_CURRENT_LIST_DIR}/../../../.." ABSOLUTE)
    endif()
    if(EXISTS "${_nros_cyclone_tree}/third-party/dds/cyclonedds/CMakeLists.txt")
        set(CYCLONEDDS_SOURCE_DIR
            "${_nros_cyclone_tree}/third-party/dds/cyclonedds"
            CACHE PATH "CycloneDDS source tree for self-provisioning (Phase 186)")
    endif()
    unset(_nros_cyclone_tree)
endif()
