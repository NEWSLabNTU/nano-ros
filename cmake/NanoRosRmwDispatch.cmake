# Generated from cargo-nano-ros `resolve_rmw()` — DO NOT EDIT.
# Regenerate: `cargo test -p cargo-nano-ros rmw_cmake_dispatch_is_current -- --ignored`
# (or run the bin helper). The SSoT is rmw_resolver.rs; this is its CMake lowering.
#
# nros_rmw_dispatch(<rmw>) sets in the CALLER scope:
#   NROS_RMW_UMBRELLA_CFFI_FEATURE  the nros-c/nros-cpp cffi feature (e.g. rmw-zenoh-cffi)
#   NROS_RMW_RLIB_DEP               backend rlib crate bundled in the umbrella, or ""
#   NROS_RMW_EXTRA_LINK_LIBS        ;-list of extra link libs (cyclonedds C++ path), or ""
#   NROS_RMW_NEEDS_CXX_LINKER       ON/OFF — force the C++ linker driver (libstdc++)
function(nros_rmw_dispatch rmw)
    if(rmw STREQUAL "zenoh")
        set(NROS_RMW_UMBRELLA_CFFI_FEATURE "rmw-zenoh-cffi" PARENT_SCOPE)
        set(NROS_RMW_RLIB_DEP "nros-rmw-zenoh" PARENT_SCOPE)
        set(NROS_RMW_EXTRA_LINK_LIBS "" PARENT_SCOPE)
        set(NROS_RMW_NEEDS_CXX_LINKER OFF PARENT_SCOPE)
    elseif(rmw STREQUAL "xrce")
        set(NROS_RMW_UMBRELLA_CFFI_FEATURE "rmw-xrce-cffi" PARENT_SCOPE)
        set(NROS_RMW_RLIB_DEP "nros-rmw-xrce-cffi" PARENT_SCOPE)
        set(NROS_RMW_EXTRA_LINK_LIBS "" PARENT_SCOPE)
        set(NROS_RMW_NEEDS_CXX_LINKER OFF PARENT_SCOPE)
    elseif(rmw STREQUAL "cyclonedds")
        set(NROS_RMW_UMBRELLA_CFFI_FEATURE "rmw-cyclonedds-cffi" PARENT_SCOPE)
        set(NROS_RMW_RLIB_DEP "" PARENT_SCOPE)
        set(NROS_RMW_EXTRA_LINK_LIBS "nros_rmw_cyclonedds;ddsc;stdc++" PARENT_SCOPE)
        set(NROS_RMW_NEEDS_CXX_LINKER ON PARENT_SCOPE)
    else()
        message(FATAL_ERROR "nros_rmw_dispatch: unknown rmw '${rmw}' "
            "(known: zenoh xrce cyclonedds)")
    endif()
endfunction()
