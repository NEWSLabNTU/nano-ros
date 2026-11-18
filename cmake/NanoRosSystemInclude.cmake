# nros_system_toml_expanded(<system.toml> <out_var> <who>) -- issue 2587,
# phase-580 W3.
#
# A `system.toml` may name shared fragments with `include = [..]`
# (RFC-0024 §5). A CMake step that line-reads the file (`file(STRINGS)`)
# reads the path this sets instead:
#
#   * no `include` in the file: the file itself, and no CLI call;
#   * otherwise: the expansion, written by `nros dev image system-toml` into
#     this build's `nros/system-toml/` (rewritten only when it changes), with
#     the file and every fragment added to CMAKE_CONFIGURE_DEPENDS, so editing
#     a fragment re-runs the configure.
#
# An expansion the CLI refuses (a key stated twice, a cycle, ...) stops the
# configure with the CLI's message, naming the file at fault.
include_guard(GLOBAL)
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCli.cmake")

function(nros_system_toml_expanded system_toml out_var who)
    set(${out_var} "${system_toml}" PARENT_SCOPE)
    if(NOT EXISTS "${system_toml}")
        return()
    endif()
    file(STRINGS "${system_toml}" _nsi_rows REGEX "^[ \t]*include[ \t]*=")
    if(NOT _nsi_rows)
        return()
    endif()
    nros_resolve_cli(_nsi_nros CONTEXT "${who} (${system_toml})")
    # The expander is the CLI's code, so a rebuilt `nros` can change the
    # expansion: register it as a configure input (issue 1018).
    nros_codegen_tool_reconfigure("${_nsi_nros}")
    string(SHA1 _nsi_key "${system_toml}")
    string(SUBSTRING "${_nsi_key}" 0 12 _nsi_key)
    set(_nsi_dir "${CMAKE_BINARY_DIR}/nros/system-toml")
    set(_nsi_out "${_nsi_dir}/${_nsi_key}.toml")
    set(_nsi_files "${_nsi_dir}/${_nsi_key}.files")
    execute_process(
        COMMAND "${_nsi_nros}" dev image system-toml "${system_toml}"
                --out "${_nsi_out}" --files-out "${_nsi_files}"
        ERROR_VARIABLE _nsi_err
        RESULT_VARIABLE _nsi_rc)
    if(NOT _nsi_rc EQUAL 0)
        string(STRIP "${_nsi_err}" _nsi_err)
        message(FATAL_ERROR "${who}: ${_nsi_err}")
    endif()
    file(STRINGS "${_nsi_files}" _nsi_inputs)
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS ${_nsi_inputs})
    set(${out_var} "${_nsi_out}" PARENT_SCOPE)
endfunction()

# nros_system_toml_watch(<system.toml> <who>) -- a configure re-runs when the
# file changes, and when any fragment it includes does. The one spelling of
# "this configure reads that system.toml" (issue 2587, phase-580 W3): a bare
# CMAKE_CONFIGURE_DEPENDS on the file alone would leave a fragment edit
# unseen, and the image built from the old value while reading as fresh.
function(nros_system_toml_watch system_toml who)
    if(NOT EXISTS "${system_toml}")
        return()
    endif()
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${system_toml}")
    nros_system_toml_expanded("${system_toml}" _nsw_unused "${who}")
endfunction()
