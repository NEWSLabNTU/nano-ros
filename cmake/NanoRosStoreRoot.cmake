# NanoRosStoreRoot.cmake — where the nano-ros store is (RFC-0103 D6,
# phase-484 W1). SPDX-License-Identifier: MIT OR Apache-2.0
#
# ONE variable names the store root: `$ENV{NROS_HOME}`, else `$ENV{HOME}/.nros`.
# Its categories (`sdk/`, `sources/`, `workspaces/`, `bin/`, `fetch/`) are
# constructed under it. `NROS_STORE` and `NROS_SDK_STORE` are RETIRED — the
# cross toolchain read `NROS_SDK_STORE` first and Corrosion did not, so one
# configure could resolve two stores (issue 1767). A set one is a FATAL_ERROR
# naming its replacement.
#
# Twins: `nros_build_paths::store` (Rust) and `scripts/lib/store-root.sh`.
# Include guard: toolchain files are re-read by every try_compile.
include_guard(GLOBAL)

# nros_store_root(<out_var>) — the store root.
function(nros_store_root out_var)
    if(DEFINED ENV{NROS_STORE})
        message(FATAL_ERROR
            "nano-ros: $NROS_STORE is retired — set NROS_HOME to the same "
            "directory (RFC-0103 D6).")
    endif()
    if(DEFINED ENV{NROS_SDK_STORE})
        message(FATAL_ERROR
            "nano-ros: $NROS_SDK_STORE is retired — set NROS_HOME to its PARENT "
            "(it named <store>/sdk; RFC-0103 D6).")
    endif()
    if(DEFINED ENV{NROS_HOME} AND NOT "$ENV{NROS_HOME}" STREQUAL "")
        set(${out_var} "$ENV{NROS_HOME}" PARENT_SCOPE)
    else()
        set(${out_var} "$ENV{HOME}/.nros" PARENT_SCOPE)
    endif()
endfunction()
