# NanoRosCodegenCore.cmake — shared helpers for the two nros_generate_interfaces()
# implementations (canonical cmake/NanoRosGenerateInterfaces.cmake +
# zephyr/cmake/nros_generate_interfaces.cmake). Phase 246.
#
# These two generators target genuinely different deployment models (library
# target vs Zephyr `app`, build-time vs configure-time codegen), so they remain
# separate entry points — but the CONTEXT-FREE pieces below were copy-pasted and
# drifted into shipped bugs three times (issues 0052, 0056, Phase 214.B.1). They
# live here now, in one place. Include this from both generators.
#
# Scope note: a `function()`'s `PARENT_SCOPE` reaches only its immediate caller
# (the generator), not the generator's caller (the user). So the helpers that
# must publish a variable to the USER's scope RETURN their result via an out-var
# (landing in the generator's scope); the generator then does the final
# one-line `set(<pkg>_GENERATED_RS_FILES ... PARENT_SCOPE)`. Helpers only read
# enclosing-scope vars (which cascade up) and write the global CACHE (which does
# not).

include_guard(GLOBAL)

# _nros_collect_rs_closure(<out_var> DEPS <pkgs...> OWN <rs-files...>)
#
# Compute the de-duplicated transitive closure of generated `_ffi.rs` files:
# each dependency's `<dep>_GENERATED_RS_FILES` (or the `_NROS_PKG_<dep>_*` CACHE
# stash, for multi-level scope chains where PARENT_SCOPE didn't reach) PLUS the
# package's own files. De-dup is REQUIRED: a diamond dependency would otherwise
# carry the same leaf `_ffi.rs` twice → both the lib.rs `include!()` of it twice
# (Rust E0428, issue 0052) and a doubled closure export. Returns the list in
# <out_var> (in the CALLER's scope).
function(_nros_collect_rs_closure _out_var)
    cmake_parse_arguments(_C "" "" "DEPS;OWN" ${ARGN})
    set(_all "")
    foreach(_dep ${_C_DEPS})
        if(DEFINED ${_dep}_GENERATED_RS_FILES)
            list(APPEND _all ${${_dep}_GENERATED_RS_FILES})
        elseif(DEFINED CACHE{_NROS_PKG_${_dep}_GENERATED_RS_FILES})
            list(APPEND _all $CACHE{_NROS_PKG_${_dep}_GENERATED_RS_FILES})
        endif()
    endforeach()
    list(APPEND _all ${_C_OWN})
    if(_all)
        list(REMOVE_DUPLICATES _all)
    endif()
    set(${_out_var} "${_all}" PARENT_SCOPE)
endfunction()

# _nros_export_rs_closure(<target> <rs-closure-list>)
#
# Stash the (already de-duplicated) closure in the INTERNAL CACHE under
# `_NROS_PKG_<target>_GENERATED_RS_FILES` so deps generated in a sibling call
# tree can read it when PARENT_SCOPE re-export doesn't reach them (Phase
# 210.E.3). The CACHE write is global, so it is scope-safe to do here; the
# matching `set(<target>_GENERATED_RS_FILES ... PARENT_SCOPE)` must stay in the
# generator body (see the scope note above).
function(_nros_export_rs_closure _target _closure)
    set(_NROS_PKG_${_target}_GENERATED_RS_FILES "${_closure}"
        CACHE INTERNAL "nros cached GENERATED_RS_FILES closure for ${_target}" FORCE)
endfunction()

# _nros_write_ffi_lib_rs(CRATE_SRC <dir> TEMPLATE <ffi_lib_rs.in> RS_FILES <list>
#                        PATH_MODE relative|absolute)
#
# Assemble the FFI crate's `src/lib.rs` from the shared `ffi_lib_rs.in` template:
# one `include!()` per unique generated `_ffi.rs` (skipping `mod.rs`), so all
# cross-package types share one flat module scope. PATH_MODE selects how the
# include path is spelled:
#   relative — emit `file(RELATIVE_PATH …)` from <CRATE_SRC>; portable across
#              clean clones / differing CI paths (Phase 214.B.1). Canonical path.
#   absolute — emit the path verbatim. The Zephyr path uses this (its crate dir
#              and outputs share a binary tree that always co-resolve).
# The template's `@NROS_CPP_FFI_INCLUDES@` placeholder is filled and the result
# written to <CRATE_SRC>/lib.rs. Pure file output — function-scope safe.
function(_nros_write_ffi_lib_rs)
    cmake_parse_arguments(_L "" "CRATE_SRC;TEMPLATE;PATH_MODE" "RS_FILES" ${ARGN})
    if(NOT _L_PATH_MODE STREQUAL "relative" AND NOT _L_PATH_MODE STREQUAL "absolute")
        message(FATAL_ERROR "_nros_write_ffi_lib_rs: PATH_MODE must be relative|absolute, got '${_L_PATH_MODE}'")
    endif()
    set(NROS_CPP_FFI_INCLUDES "")
    foreach(_rs_file ${_L_RS_FILES})
        get_filename_component(_rs_name "${_rs_file}" NAME)
        if(_rs_name STREQUAL "mod.rs")
            continue()
        endif()
        if(_L_PATH_MODE STREQUAL "relative")
            file(RELATIVE_PATH _rs_path "${_L_CRATE_SRC}" "${_rs_file}")
        else()
            set(_rs_path "${_rs_file}")
        endif()
        string(APPEND NROS_CPP_FFI_INCLUDES "include!(\"${_rs_path}\");\n")
    endforeach()
    configure_file("${_L_TEMPLATE}" "${_L_CRATE_SRC}/lib.rs" @ONLY)
endfunction()

# _nros_write_codegen_args_json(ARGS_FILE <path> PACKAGE <name> OUTPUT_DIR <dir>
#     ROS_EDITION <edition> [CODEGEN_CONFIG <path>]
#     INTERFACE_FILES <files...> DEPS <pkgs...>)
#
# Build the `nros codegen --args-file` JSON and write it ONLY when the content
# changed (so a re-configure doesn't perturb the file mtime → the codegen
# add_custom_command / mtime check sees its outputs already up to date,
# essential for the workspace-shared codegen cache). `CODEGEN_CONFIG` is the
# optional RFC-0033 per-field capacity config; omit it to emit no such field.
function(_nros_write_codegen_args_json)
    cmake_parse_arguments(_J ""
        "ARGS_FILE;PACKAGE;OUTPUT_DIR;ROS_EDITION;CODEGEN_CONFIG"
        "INTERFACE_FILES;DEPS" ${ARGN})
    set(_files_json "")
    set(_first TRUE)
    foreach(_f ${_J_INTERFACE_FILES})
        if(NOT _first)
            string(APPEND _files_json ",")
        endif()
        set(_first FALSE)
        string(APPEND _files_json "\n    \"${_f}\"")
    endforeach()
    set(_deps_json "")
    set(_first TRUE)
    foreach(_d ${_J_DEPS})
        if(NOT _first)
            string(APPEND _deps_json ",")
        endif()
        set(_first FALSE)
        string(APPEND _deps_json "\n    \"${_d}\"")
    endforeach()
    set(_cfg_json "")
    if(DEFINED _J_CODEGEN_CONFIG AND NOT _J_CODEGEN_CONFIG STREQUAL "")
        set(_cfg_json ",\n  \"codegen_config\": \"${_J_CODEGEN_CONFIG}\"")
    endif()
    set(_content "{
  \"package_name\": \"${_J_PACKAGE}\",
  \"output_dir\": \"${_J_OUTPUT_DIR}\",
  \"interface_files\": [${_files_json}
  ],
  \"dependencies\": [${_deps_json}
  ],
  \"ros_edition\": \"${_J_ROS_EDITION}\"${_cfg_json}
}
")
    set(_write TRUE)
    if(EXISTS "${_J_ARGS_FILE}")
        file(READ "${_J_ARGS_FILE}" _existing)
        if(_existing STREQUAL _content)
            set(_write FALSE)
        endif()
    endif()
    if(_write)
        file(WRITE "${_J_ARGS_FILE}" "${_content}")
    endif()
endfunction()

# _nros_predict_generated_outputs(<headers_var> <sources_var> <rs_var>
#     LANGUAGE C|CPP PACKAGE <name> OUTPUT_DIR <dir> INTERFACE_FILES <files...>)
#
# Predict the files `nros codegen` will emit for the given interfaces, returning
# three lists (headers / C sources / Rust `_ffi.rs`) in the caller's scope.
# CPP: `<pkg>_<kind>_<name>.hpp` + per-kind `_ffi.rs` (msg→1, srv→request+response,
# action→goal+result+feedback) + the `<pkg>.hpp` umbrella + `mod.rs`. C:
# `<pkg>_<kind>_<name>.{h,c}` + the `<pkg>.h` umbrella. Names are CamelCase→snake,
# package `-`→`_`. The canonical generator feeds these to add_custom_command
# OUTPUT (must match codegen exactly); the Zephyr generator concatenates them for
# its mtime "needs-regen" check.
function(_nros_predict_generated_outputs _hdr_var _src_var _rs_var)
    cmake_parse_arguments(_P "" "LANGUAGE;PACKAGE;OUTPUT_DIR" "INTERFACE_FILES" ${ARGN})
    set(_headers "")
    set(_sources "")
    set(_rs "")
    string(REPLACE "-" "_" _c_pkg "${_P_PACKAGE}")
    foreach(_file ${_P_INTERFACE_FILES})
        get_filename_component(_name "${_file}" NAME_WE)
        get_filename_component(_ext "${_file}" EXT)
        string(REGEX REPLACE "([a-z])([A-Z])" "\\1_\\2" _name_snake "${_name}")
        string(TOLOWER "${_name_snake}" _name_lower)
        if(_ext STREQUAL ".msg")
            set(_kind "msg")
        elseif(_ext STREQUAL ".srv")
            set(_kind "srv")
        elseif(_ext STREQUAL ".action")
            set(_kind "action")
        else()
            message(FATAL_ERROR "_nros_predict_generated_outputs: unknown interface extension '${_ext}' (${_file})")
        endif()
        set(_base "${_P_OUTPUT_DIR}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}")
        if(_P_LANGUAGE STREQUAL "CPP")
            list(APPEND _headers "${_base}.hpp")
            if(_kind STREQUAL "msg")
                list(APPEND _rs "${_base}_ffi.rs")
            elseif(_kind STREQUAL "srv")
                list(APPEND _rs "${_base}_request_ffi.rs" "${_base}_response_ffi.rs")
            elseif(_kind STREQUAL "action")
                list(APPEND _rs "${_base}_goal_ffi.rs" "${_base}_result_ffi.rs" "${_base}_feedback_ffi.rs")
            endif()
        else()
            list(APPEND _headers "${_base}.h")
            list(APPEND _sources "${_base}.c")
        endif()
    endforeach()
    if(_P_LANGUAGE STREQUAL "CPP")
        list(APPEND _headers "${_P_OUTPUT_DIR}/${_P_PACKAGE}.hpp")
        list(APPEND _rs "${_P_OUTPUT_DIR}/mod.rs")
    else()
        list(APPEND _headers "${_P_OUTPUT_DIR}/${_P_PACKAGE}.h")
    endif()
    set(${_hdr_var} "${_headers}" PARENT_SCOPE)
    set(${_src_var} "${_sources}" PARENT_SCOPE)
    set(${_rs_var} "${_rs}" PARENT_SCOPE)
endfunction()
