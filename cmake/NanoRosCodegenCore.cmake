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
