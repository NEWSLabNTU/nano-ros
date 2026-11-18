# nros_system_toml_refuse_include(<system.toml> <who>) -- issue 2587.
#
# A `system.toml` may name shared fragments with `include = [..]`
# (RFC-0024 §5). The `nros` CLI expands them; a CMake line read of the file
# cannot, and would see only what the file itself states, so a value stated in
# a fragment would silently go missing. Until this road reads the expanded
# system through the CLI, a file with an `include` is refused here, never read
# raw.
include_guard(GLOBAL)

function(nros_system_toml_refuse_include system_toml who)
    if(NOT EXISTS "${system_toml}")
        return()
    endif()
    file(STRINGS "${system_toml}" _nsi_rows REGEX "^[ \t]*include[ \t]*=")
    if(_nsi_rows)
        message(FATAL_ERROR
            "${who}: ${system_toml} names shared fragments with `include = [..]`, "
            "which this CMake step cannot read yet. Build it with `nros build`, or "
            "state the fragments' keys in this system.toml itself.")
    endif()
endfunction()
