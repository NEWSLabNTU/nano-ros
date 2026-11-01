# phase-206 W2 — bake a bringup's own RMW config into the image.
#
# THE EXPERIENCE THIS EXISTS FOR. On ROS 2 the OS owns the devices and the user
# configures the middleware in the middleware's own language: an XML file named
# by `CYCLONEDDS_URI`, naming an IP or a device. Application code knows nothing
# about NICs. nano-ros should be the same — but on an RTOS there is no
# environment to set and no filesystem to point at, so `env_lookup` returns
# `nullptr` on a freestanding target and that rung is structurally dead.
#
# So the FILE is baked. The user writes the same document they would write for
# ROS 2; the build turns its bytes into a string literal; the backend hands
# those bytes to its OWN parser. nano-ros parses nothing.
#
# WHERE THE FILE LIVES, and why not `config/`:
#
#     <bringup>/rmw/cyclonedds.xml     authored, tracked, reviewed
#     <bringup>/rmw/zenoh.json5        (RFC-0109 D8 — lowered by the CLI, not here)
#     <bringup>/config/*.yaml          GENERATED, gitignored (phase-330 W4.a)
#
# `<bringup>/config/` already means "build artifact, never committed" — putting
# an authored file there would put the one thing a user must edit inside the one
# directory the tree says is disposable. `rmw/` sits beside `launch/`, which is
# the existing precedent for authored, typed, per-bringup input.
#
# CONVENTION, THEN DECLARATION (RFC-0109 D6, phase-514 W3b). With no key the
# file is the one the backend's descriptor names (`rmw/cyclonedds.xml`,
# RFC-0087's "derived from convention"); `[system] rmw_config` names another
# for the bringup, `[image.<id>] rmw_config` one for an image. WHICH file is
# the CLI's answer (`nros ws rmw-config`), never spelled here: this module
# once encoded the convention itself, which is exactly the second reader a
# declared key would have silently bypassed.

# Add PATH to this directory's configure dependencies, CANONICAL and once.
# A bringup is often named as `${CMAKE_CURRENT_SOURCE_DIR}/../<bringup>`, and
# the entry already registers the same file by its real path
# (NanoRosEntry.cmake). Ninja normalizes both spellings into one output of the
# generated missing-inputs phony rule, and the pinned ninja 1.13 refuses that:
# "system.toml is defined as an output multiple times".
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCliRows.cmake")  # issue 2200

function(_nros_rmw_config_depend _path)
    file(REAL_PATH "${_path}" _real)
    get_property(_deps DIRECTORY PROPERTY CMAKE_CONFIGURE_DEPENDS)
    if(NOT "${_real}" IN_LIST _deps)
        set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_real}")
    endif()
endfunction()

# The bringup's default-session RMW file for BACKEND, and how it is carried:
# `<out>` gets the path ("" when there is none), `<out>_FORMAT` the
# descriptor's `[rmw.config] format` (`session-json5`, `verbatim-xml`). IMAGE
# names the image this build is (default `NROS_IMAGE`); without one the CLI
# answers the file every image on BACKEND agrees on, and refuses otherwise.
function(nros_rmw_user_config_file _out)
    cmake_parse_arguments(_NRF "" "BRINGUP;BACKEND;IMAGE" "" ${ARGN})
    set(${_out} "" PARENT_SCOPE)
    set(${_out}_FORMAT "" PARENT_SCOPE)
    if(NOT _NRF_BRINGUP OR NOT _NRF_BACKEND OR NOT EXISTS "${_NRF_BRINGUP}/system.toml")
        return()
    endif()
    if(NOT COMMAND nros_resolve_cli)
        include("${CMAKE_CURRENT_FUNCTION_LIST_DIR}/NanoRosCli.cmake")
    endif()
    nros_resolve_cli(_nros CONTEXT "the bringup's RMW file (RFC-0109 D6)")
    set(_img "${_NRF_IMAGE}")
    if(NOT _img AND DEFINED NROS_IMAGE)
        set(_img "${NROS_IMAGE}")
    endif()
    set(_img_arg "")
    if(_img)
        set(_img_arg --image "${_img}")
    endif()
    execute_process(
        COMMAND "${_nros}" ws rmw-config "${_NRF_BRINGUP}" --rmw "${_NRF_BACKEND}" ${_img_arg}
        OUTPUT_VARIABLE _o
        ERROR_VARIABLE _e
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    # The answer is baked, so a rebuilt `nros` and an edited `system.toml`
    # must both re-run this configure (issue 1018).
    if(COMMAND nros_codegen_tool_reconfigure)
        nros_codegen_tool_reconfigure("${_nros}")
    endif()
    _nros_rmw_config_depend("${_NRF_BRINGUP}/system.toml")
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR "nano-ros: ${_NRF_BRINGUP}/system.toml: ${_e}")
    endif()
    set(_file "")
    set(_fmt "")
    nros_cli_rows(_row "${_o}")  # issue 2200: never through a cmake list
    foreach(_i IN LISTS _row_INDICES)
        set(_l "${_row_${_i}}")
        if(_l MATCHES "^NROS_RMW_CONFIG_FILE=(.*)$")
            set(_file "${CMAKE_MATCH_1}")
        elseif(_l MATCHES "^NROS_RMW_CONFIG_FORMAT=(.*)$")
            set(_fmt "${CMAKE_MATCH_1}")
        endif()
    endforeach()
    if(_file)
        # CONFIGURE_DEPENDS is not available for `file(READ)`, so editing the
        # file must re-configure explicitly — the museum-binary class (0196).
        _nros_rmw_config_depend("${_file}")
    endif()
    set(${_out} "${_file}" PARENT_SCOPE)
    set(${_out}_FORMAT "${_fmt}" PARENT_SCOPE)
endfunction()

# Bake the bringup's default-session RMW file for BACKEND into
# `<out_dir>/nros_<backend>_user_config.h`, when the backend parses that file
# itself (`verbatim-xml`). FILE names it directly (the ctest's fixture);
# otherwise BRINGUP is asked through `nros_rmw_user_config_file`.
#
# Writes nothing when there is no such file — the consuming header guards on
# `__has_include`, so an image with no config compiles with no cmake
# participation at all. A `session-json5` file is not baked here: CMake cannot
# parse it, so `nros codegen entry --rmw-config` lowers it and the entry bakes
# the result (RFC-0109 D8).
function(nros_bake_rmw_user_config)
    cmake_parse_arguments(_NBR "" "BRINGUP;IMAGE;BACKEND;OUT_DIR;RESULT;FILE" "" ${ARGN})
    if((NOT _NBR_BRINGUP AND NOT _NBR_FILE) OR NOT _NBR_BACKEND OR NOT _NBR_OUT_DIR)
        message(FATAL_ERROR
            "nros_bake_rmw_user_config: BRINGUP or FILE, BACKEND and OUT_DIR are required")
    endif()
    if(_NBR_RESULT)
        set(${_NBR_RESULT} "" PARENT_SCOPE)
    endif()

    if(_NBR_FILE)
        set(_src "${_NBR_FILE}")
        set(_fmt "verbatim-xml")
        set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_src}")
    else()
        nros_rmw_user_config_file(_src
            BRINGUP "${_NBR_BRINGUP}" BACKEND "${_NBR_BACKEND}" IMAGE "${_NBR_IMAGE}")
        set(_fmt "${_src_FORMAT}")
    endif()
    # Not an error: a backend with no file, or one whose file is lowered.
    if(NOT _src OR NOT _fmt STREQUAL "verbatim-xml")
        return()
    endif()
    if(NOT EXISTS "${_src}")
        message(FATAL_ERROR "nros_bake_rmw_user_config: ${_src} does not exist")
    endif()

    file(READ "${_src}" _body)

    # The raw-string delimiter must not appear in the document, or the literal
    # ends early and the file after it becomes code. Refuse rather than mangle:
    # a config that silently half-applied is this phase's whole complaint.
    if(_body MATCHES "\\)NROSXML\"")
        message(FATAL_ERROR
            "nros_bake_rmw_user_config: ${_src} contains the raw-string "
            "terminator )NROSXML\" and cannot be baked verbatim. Rename the "
            "delimiter in cmake/NanoRosRmwUserConfig.cmake, or remove that "
            "byte sequence from the file.")
    endif()

    set(_out "${_NBR_OUT_DIR}/nros_${_NBR_BACKEND}_user_config.h")
    file(MAKE_DIRECTORY "${_NBR_OUT_DIR}")
    string(TOUPPER "${_NBR_BACKEND}" _BACKEND_UC)
    # A VARIABLE, not a `#define`. A raw string literal cannot span physical
    # lines inside a macro definition: the preprocessor ends the directive at
    # the first unescaped newline, leaving the literal unterminated. The first
    # version of this generator emitted a macro and every multi-line config
    # failed to compile with "unterminated raw string" pointing at the generated
    # file — caught by the W2 test, which is what it is for.
    #
    # The PRESENT marker is a macro because that is the part `#ifdef` must see;
    # only the payload has to be a variable.
    file(WRITE "${_out}"
"/* GENERATED by nros_bake_rmw_user_config() — DO NOT EDIT.
 *
 * SSoT: ${_src}
 *
 * The bytes below are the user's own ${_NBR_BACKEND} configuration, verbatim.
 * nano-ros does not parse, validate or re-spell them; they are handed to the
 * backend's own parser. Edit the SSoT above, not this file. */
#ifndef NROS_${_BACKEND_UC}_USER_CONFIG_H
#define NROS_${_BACKEND_UC}_USER_CONFIG_H
#define NROS_${_BACKEND_UC}_USER_CONFIG_PRESENT 1

namespace nros_rmw_user_config {
inline constexpr const char* k${_BACKEND_UC}_config = R\"NROSXML(${_body})NROSXML\";
}  // namespace nros_rmw_user_config

#endif
")
    message(STATUS "nano-ros: baked ${_src} -> ${_out}")
    if(_NBR_RESULT)
        set(${_NBR_RESULT} "${_out}" PARENT_SCOPE)
    endif()
endfunction()

# Issue 2123 — a bridged `[[domain]]`'s RMW session properties as a C header,
# for a hand-written multi-session C/C++ caller (`nros_cpp_init_multi` with
# `NrosCppSessionSpec::properties`, `rclc_init_multi` with
# `rclc_session_spec_t::properties`). The table is the CLI's answer
# (`nros ws session-properties`): the domain's RMW file lowered on the host
# (RFC-0109 D8), the same rows `nros sync` writes into `nros-bridge.toml` for a
# Rust image. Nothing about the file's format is spelled here.
#
#   nros_session_properties_header(
#       BRINGUP <dir>   DOMAIN <name>   SYMBOL <c_ident>   OUT <header>
#       [IMAGE <id>]    [ROW_TYPE nros_cpp_session_property_t|rclc_session_property_t])
#
# Writes `static const <ROW_TYPE> <SYMBOL>[N]` and `<SYMBOL>_LEN` to OUT at
# configure time; the CLI, the `system.toml` and the domain's file are
# re-configure edges, so editing the file rebakes the header (issue 1018).
function(nros_session_properties_header)
    cmake_parse_arguments(_NSP "" "BRINGUP;DOMAIN;SYMBOL;OUT;IMAGE;ROW_TYPE" "" ${ARGN})
    foreach(_req BRINGUP DOMAIN SYMBOL OUT)
        if(NOT _NSP_${_req})
            message(FATAL_ERROR "nros_session_properties_header: ${_req} is required")
        endif()
    endforeach()
    if(NOT _NSP_ROW_TYPE)
        set(_NSP_ROW_TYPE nros_cpp_session_property_t)
    endif()
    if(NOT COMMAND nros_resolve_cli)
        include("${CMAKE_CURRENT_FUNCTION_LIST_DIR}/NanoRosCli.cmake")
    endif()
    nros_resolve_cli(_nros CONTEXT "a bridged domain's session properties (issue 2123)")
    set(_img "${_NSP_IMAGE}")
    if(NOT _img AND DEFINED NROS_IMAGE)
        set(_img "${NROS_IMAGE}")
    endif()
    set(_img_arg "")
    if(_img)
        set(_img_arg --image "${_img}")
    endif()
    execute_process(
        COMMAND "${_nros}" ws session-properties "${_NSP_BRINGUP}"
                --domain "${_NSP_DOMAIN}" --symbol "${_NSP_SYMBOL}"
                --row-type "${_NSP_ROW_TYPE}" --out "${_NSP_OUT}" ${_img_arg}
        OUTPUT_VARIABLE _o
        ERROR_VARIABLE _e
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(COMMAND nros_codegen_tool_reconfigure)
        nros_codegen_tool_reconfigure("${_nros}")
    endif()
    _nros_rmw_config_depend("${_NSP_BRINGUP}/system.toml")
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR "nano-ros: ${_NSP_BRINGUP}/system.toml: ${_e}")
    endif()
    if(_o MATCHES "NROS_RMW_CONFIG_FILE=([^\n]*)")
        _nros_rmw_config_depend("${CMAKE_MATCH_1}")
    endif()
endfunction()
