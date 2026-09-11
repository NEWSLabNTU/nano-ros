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

if(NANO_ROS_ROOT_DIR)
    set(_nros_cyclone_tree "${NANO_ROS_ROOT_DIR}")
else()
    get_filename_component(_nros_cyclone_tree
        "${CMAKE_CURRENT_LIST_DIR}/../../../.." ABSOLUTE)
endif()

# Issue 1304 — an INSTALLED SDK root builds against the Cyclone `nros setup`
# already provisioned (RFC-0099 D4: prefer the prebuilt).
#
# `nros setup <board> --rmw cyclonedds` unpacks the `[tool.cyclonedds]` dist
# into the store, and nothing told configure to look there: find_package missed
# it, the fallback below is a checkout's submodule, and an installed root has
# no checkout — "CycloneDDS not found and no source to build it from".
#
# ASK the toolchain for the pinned prefix (`nros sdk-path`, the same bridge
# NanoRosCorrosion.cmake uses — never a glob of the store, issue 0625), from
# the root so it reads the index this root ships.
#
# Only for an installed root, and only for a host build. The root carries
# `nros-submodule-pins.toml` exactly when a release staged it
# (scripts/stage-sdk-root.sh); a CHECKOUT never does, so a contributor keeps
# building the fork submodule they may be editing — RFC-0099 D3's ownership
# argument one layer down: a store copy must not silently replace a tree
# someone owns. A cross build never takes find_package's answer anyway (see
# ProvideCycloneDDS.cmake). The name is held against the reader and the stager
# by `check-release-manifest` R6.
if(NOT CMAKE_CROSSCOMPILING
   AND NOT DEFINED CycloneDDS_DIR
   AND EXISTS "${_nros_cyclone_tree}/nros-submodule-pins.toml"
   AND COMMAND nros_resolve_cli)
    nros_resolve_cli(_nros_cyclone_cli OPTIONAL CONTEXT "nros-rmw-cyclonedds")
    if(_nros_cyclone_cli AND NOT _nros_cyclone_cli STREQUAL "NOTFOUND")
        execute_process(
            COMMAND "${_nros_cyclone_cli}" sdk-path cyclonedds --require
            WORKING_DIRECTORY "${_nros_cyclone_tree}"
            OUTPUT_VARIABLE _nros_cyclone_prefix
            OUTPUT_STRIP_TRAILING_WHITESPACE
            ERROR_VARIABLE _nros_cyclone_err
            ERROR_STRIP_TRAILING_WHITESPACE
            RESULT_VARIABLE _nros_cyclone_rc)
        if(_nros_cyclone_rc EQUAL 0 AND IS_DIRECTORY "${_nros_cyclone_prefix}")
            list(PREPEND CMAKE_PREFIX_PATH "${_nros_cyclone_prefix}")
            message(STATUS
                "nano-ros: installed SDK root — CycloneDDS from the provisioned "
                "prebuilt at ${_nros_cyclone_prefix}")
        else()
            message(STATUS
                "nano-ros: installed SDK root, but no provisioned CycloneDDS "
                "(${_nros_cyclone_err}) — `nros setup <board> --rmw cyclonedds` "
                "provisions it")
        endif()
    endif()
    unset(_nros_cyclone_cli)
    unset(_nros_cyclone_prefix)
    unset(_nros_cyclone_err)
    unset(_nros_cyclone_rc)
endif()

if(NOT DEFINED CYCLONEDDS_SOURCE_DIR)
    if(EXISTS "${_nros_cyclone_tree}/third-party/dds/cyclonedds/CMakeLists.txt")
        set(CYCLONEDDS_SOURCE_DIR
            "${_nros_cyclone_tree}/third-party/dds/cyclonedds"
            CACHE PATH "CycloneDDS source tree for self-provisioning (Phase 186)")
    endif()
endif()
unset(_nros_cyclone_tree)
