# NanoRosConfigureHook.cmake -- phase-530 W4/W5 (RFC-0120 D5): every HOOKED
# host configure calls `nros configure` before its first reader.
#
# =============================================================================
# One call, three hosts
# =============================================================================
#
#   * Zephyr west: `nros_image_kconfig_hook` (zephyr/cmake/nros_image_kconfig.cmake),
#     inside `find_package(Zephyr)`, before Kconfig;
#   * CMake, `find_package(nano_ros)`: the top of the package config's leaf
#     arm, before `nros_record_leaf_entity_facts` and the import, and
#     `nano_ros_workspace()` before its import;
#   * CMake, `add_subdirectory(<nano-ros>)` with no `find_package`: the top of
#     the checkout's `CMakeLists.txt`.
#
# All of them call `nros_configure_hook` below, and nothing else spells the
# configure command: a second spelling is how one host would drift from the
# others (CLAUDE.md, "fix the class").
#
# =============================================================================
# What it does
# =============================================================================
#
#   nros_configure_hook(DIR <dir-holding-system.toml>
#                       [IMAGE <id> | IMAGES <id>... | ALL_IMAGES]
#                                             # else the leaf's default image
#                       [PROJECT <project>]   # else DIR itself
#                       [CLI <nros>]
#                       [ROAD <label>])
#
# * The PROJECT is the consumer's own source dir (DIR), unless the caller
#   KNOWS it is building a workspace's image and says which workspace:
#   `nano_ros_workspace()` passes its WORKSPACE_ROOT, and the Zephyr hook
#   walks up (RFC-0085 D14) only for a generated application, i.e. only when
#   `NROS_IMAGE_SYSTEM_DIR` was given (W4 review ruling R31). A parent dir
#   that happens to be named `src` decides nothing.
# * The IMAGE is the one this configure builds: IMAGE when the caller knows it
#   (`-DNROS_IMAGE`, `-DNROS_CONFIGURED_IMAGE`), else the leaf's own
#   `default_images`, asked of `nros dev image leaf-system` (the image
#   `nros build` builds). IMAGES -- a generated workspace root whose coordinate
#   several images share -- configures each and locates none, and ALL_IMAGES
#   -- a root that names none -- configures every image the project declares
#   and locates none; either way its readers stay unseeded (phase-530 W3's
#   per-coordinate union is what gives such a configure one answer).
# * A QUERY caller (RFC-0120 D1): `nros configure` runs no target build tool
#   and never `nros build`; with a current stamp it is a digest comparison and
#   returns at once -- which is what it costs under `nros build`, whose
#   implicit configure already ran and released the project lock before it
#   handed off to this host (`cmd::build::configure_implicitly`).
# * A no-op inside the census's own host build (`NROS_LEAF_CENSUS_HOST`),
#   which is configure's producer, not its consumer; and for a GUEST (no
#   `system.toml` in DIR), which is unchanged.
# * ONCE per host configure: the CMake roads reach `find_package(nano_ros)`
#   from every member package and the root `CMakeLists.txt` after it, and only
#   the first call configures.
#
# It then asks where configure put the image's answer
# (`nros_configured_locate`), which publishes it only from a CURRENT stamp
# (Ruling R33), and marks this configure CURRENT (`NROS_CONFIGURED_CURRENT`)
# exactly then, so every reader after it reads configure's seed and the late
# producers are held to it (cmake/NanoRosPromise.cmake).

include_guard(GLOBAL)

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCli.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosResolved.cmake")

# The checkout this module lives in, captured at FILE scope (inside a function
# `CMAKE_CURRENT_LIST_DIR` names the caller's file) and CACHE INTERNAL, so a
# function-scoped `include()` of this file does not drop it when its frame pops.
get_filename_component(_NROS_CONFIGURE_HOOK_ROOT "${CMAKE_CURRENT_LIST_DIR}/.." ABSOLUTE)
set(_NROS_CONFIGURE_HOOK_ROOT "${_NROS_CONFIGURE_HOOK_ROOT}" CACHE INTERNAL
    "phase-530 W5: the nano-ros checkout NanoRosConfigureHook.cmake belongs to")

# nros_configure_hook_ran(<out_var>) -- TRUE once a hook configured in THIS
# host configure (GLOBAL properties live for one configure only).
function(nros_configure_hook_ran _out)
    get_property(_ran GLOBAL PROPERTY NROS_CONFIGURE_HOOK_RAN)
    if(_ran)
        set(${_out} TRUE PARENT_SCOPE)
    else()
        set(${_out} FALSE PARENT_SCOPE)
    endif()
endfunction()

function(nros_configure_hook)
    cmake_parse_arguments(_H "ALL_IMAGES" "DIR;IMAGE;PROJECT;CLI;ROAD" "IMAGES" ${ARGN})
    if(NOT "$ENV{NROS_LEAF_CENSUS_HOST}" STREQUAL "")
        return()
    endif()
    if("${_H_DIR}" STREQUAL "" OR NOT EXISTS "${_H_DIR}/system.toml")
        return()
    endif()
    nros_configure_hook_ran(_ran)
    if(_ran)
        return()
    endif()
    set_property(GLOBAL PROPERTY NROS_CONFIGURE_HOOK_RAN TRUE)
    if("${_H_ROAD}" STREQUAL "")
        set(_H_ROAD "this host configure")
    endif()
    get_filename_component(_dir "${_H_DIR}" ABSOLUTE)
    if("${_H_PROJECT}" STREQUAL "")
        set(_project "${_dir}")
    else()
        get_filename_component(_project "${_H_PROJECT}" ABSOLUTE)
    endif()
    if("${_H_CLI}" STREQUAL "")
        nros_resolve_cli(_nros CONTEXT
            "nano-ros: ${_H_ROAD} runs `nros configure` first (RFC-0120 D5)")
    else()
        set(_nros "${_H_CLI}")
    endif()

    # The image(s): the caller's, else the one `nros build` builds. A single
    # image is LOCATED afterwards; several (IMAGES) or every one (ALL_IMAGES)
    # are configured and none is located.
    if(_H_ALL_IMAGES)
        set(_images "ALL")
    elseif(NOT "${_H_IMAGES}" STREQUAL "")
        set(_images ${_H_IMAGES})
    elseif(NOT "${_H_IMAGE}" STREQUAL "")
        set(_images "${_H_IMAGE}")
    else()
        execute_process(
            COMMAND "${_nros}" dev image leaf-system "${_dir}"
                    --nano-ros-path "${_NROS_CONFIGURE_HOOK_ROOT}"
            OUTPUT_VARIABLE _out ERROR_VARIABLE _err RESULT_VARIABLE _rc
            OUTPUT_STRIP_TRAILING_WHITESPACE)
        # Issue 1018 -- the answer (which image) is baked into this configure,
        # so a rebuilt `nros` must re-run it. Guarded: the Zephyr module hook
        # reaches this before NanoRosCodegenCore defines it.
        if(COMMAND nros_codegen_tool_reconfigure)
            nros_codegen_tool_reconfigure("${_nros}")
        endif()
        if(NOT _rc EQUAL 0)
            message(FATAL_ERROR "nano-ros: ${_dir}/system.toml: ${_err}\n"
                "(a leaf with several images builds the one `-DNROS_IMAGE=<id>` names)")
        endif()
        set(_images "")
        if(_out MATCHES "(^|\n)NROS_LEAF_IMAGE=([^\n]*)")
            set(_images "${CMAKE_MATCH_2}")
        endif()
        if("${_images}" STREQUAL "")
            message(FATAL_ERROR "nano-ros: ${_dir}/system.toml: no image to configure "
                "(name one with `-DNROS_IMAGE=<id>`)")
        endif()
    endif()

    foreach(_image IN LISTS _images)
        _nros_configure_hook_run("${_nros}" "${_project}" "${_image}" "${_H_ROAD}")
    endforeach()
    set_property(GLOBAL PROPERTY NROS_CONFIGURE_HOOK_PROJECT "${_project}")
    set_property(GLOBAL PROPERTY NROS_CONFIGURE_HOOK_IMAGE "${_images}")
    list(LENGTH _images _n)
    if(NOT _n EQUAL 1 OR "${_images}" STREQUAL "ALL")
        return()
    endif()
    nros_configured_locate("${_project}" "${_images}")
    # Current only when locate published a CURRENT answer (Ruling R33):
    # configure just ran, but its stamp is what says so.
    get_property(_located_current GLOBAL PROPERTY NROS_CONFIGURED_LOCATED_CURRENT)
    set_property(GLOBAL PROPERTY NROS_CONFIGURED_CURRENT "${_located_current}")
    get_property(_resolved GLOBAL PROPERTY NROS_CONFIGURED_RESOLVED_DIR)
    if(NOT "${_resolved}" STREQUAL "")
        message(STATUS "nano-ros: image `${_images}` answer -> ${_resolved}")
    endif()
endfunction()

# _nros_configure_hook_run(<cli> <project> <image|ALL> <road>) -- ONE
# `nros configure`, timed and reported; FATAL when it fails.
function(_nros_configure_hook_run _nros _project _image _road)
    set(_image_args "")
    set(_what "${_project}")
    if(NOT "${_image}" STREQUAL "ALL")
        set(_image_args --image "${_image}")
        set(_what "${_project} --image ${_image}")
    endif()
    # The wall time is reported, because "a current stamp is a digest
    # comparison" is a claim a log should let anyone check. `TIMESTAMP %s` is
    # portable (no GNU `date +%N`), so it is whole seconds.
    string(TIMESTAMP _t0 "%s" UTC)
    # From the project itself: the CLI's workspace check asks which checkout
    # the cwd is in, and the build dir may sit under another one.
    execute_process(
        COMMAND "${_nros}" configure "${_project}" ${_image_args}
        WORKING_DIRECTORY "${_project}"
        OUTPUT_VARIABLE _out ERROR_VARIABLE _err RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_STRIP_TRAILING_WHITESPACE)
    string(TIMESTAMP _t1 "%s" UTC)
    math(EXPR _s "${_t1} - ${_t0}")
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR
            "nano-ros: `nros configure ${_what}` failed "
            "(RFC-0120 D5: ${_road} runs it first):\n${_err}\n${_out}")
    endif()
    set(_said "configured")
    if(_out MATCHES "(^|\n)(configure: [^\n]*)")
        set(_said "${CMAKE_MATCH_2}")
    endif()
    message(STATUS "nano-ros: nros configure ${_what}: ${_said} (${_s} s)")
endfunction()
