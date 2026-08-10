# Generated from cargo-nano-ros `resolve_rmw()` — DO NOT EDIT.
# Regenerate: `cargo test -p cargo-nano-ros rmw_cmake_dispatch_is_current -- --ignored`
# (or run the bin helper). The SSoT is rmw_resolver.rs; this is its CMake lowering.
#
# nros_rmw_dispatch(<rmw>) sets in the CALLER scope:
#   NROS_RMW_UMBRELLA_CFFI_FEATURE  the nros-c/nros-cpp cffi feature (e.g. rmw-zenoh-cffi)
#   NROS_RMW_RLIB_DEP               backend rlib crate bundled in the umbrella, or ""
#   NROS_RMW_EXTRA_LINK_LIBS        ;-list of extra link libs (cyclonedds C++ path), or ""
#   NROS_RMW_NEEDS_CXX_LINKER       ON/OFF — force the C++ linker driver (libstdc++)
#   NROS_RMW_CPP_DEFINE             the define nros-cpp puts on its INTERFACE
#   NROS_RMW_CMAKE_TARGET           a cmake target to link when present, or ""
function(nros_rmw_dispatch rmw)
    if(rmw STREQUAL "cyclonedds")
        set(NROS_RMW_UMBRELLA_CFFI_FEATURE "rmw-cyclonedds-cffi" PARENT_SCOPE)
        set(NROS_RMW_RLIB_DEP "" PARENT_SCOPE)
        set(NROS_RMW_EXTRA_LINK_LIBS "nros_rmw_cyclonedds;ddsc;stdc++" PARENT_SCOPE)
        set(NROS_RMW_NEEDS_CXX_LINKER ON PARENT_SCOPE)
        set(NROS_RMW_CPP_DEFINE "NROS_RMW_CYCLONEDDS" PARENT_SCOPE)
        set(NROS_RMW_CMAKE_TARGET "" PARENT_SCOPE)
    elseif(rmw STREQUAL "uorb")
        set(NROS_RMW_UMBRELLA_CFFI_FEATURE "rmw-uorb-cffi" PARENT_SCOPE)
        set(NROS_RMW_RLIB_DEP "" PARENT_SCOPE)
        set(NROS_RMW_EXTRA_LINK_LIBS "nros_rmw_uorb" PARENT_SCOPE)
        set(NROS_RMW_NEEDS_CXX_LINKER ON PARENT_SCOPE)
        set(NROS_RMW_CPP_DEFINE "NROS_RMW_UORB" PARENT_SCOPE)
        set(NROS_RMW_CMAKE_TARGET "nros_rmw_uorb" PARENT_SCOPE)
    elseif(rmw STREQUAL "xrce")
        set(NROS_RMW_UMBRELLA_CFFI_FEATURE "rmw-xrce-cffi" PARENT_SCOPE)
        set(NROS_RMW_RLIB_DEP "nros-rmw-xrce-cffi" PARENT_SCOPE)
        set(NROS_RMW_EXTRA_LINK_LIBS "" PARENT_SCOPE)
        set(NROS_RMW_NEEDS_CXX_LINKER OFF PARENT_SCOPE)
        set(NROS_RMW_CPP_DEFINE "NROS_RMW_XRCE_CFFI" PARENT_SCOPE)
        set(NROS_RMW_CMAKE_TARGET "" PARENT_SCOPE)
    elseif(rmw STREQUAL "zenoh")
        set(NROS_RMW_UMBRELLA_CFFI_FEATURE "rmw-zenoh-cffi" PARENT_SCOPE)
        set(NROS_RMW_RLIB_DEP "nros-rmw-zenoh" PARENT_SCOPE)
        set(NROS_RMW_EXTRA_LINK_LIBS "" PARENT_SCOPE)
        set(NROS_RMW_NEEDS_CXX_LINKER OFF PARENT_SCOPE)
        set(NROS_RMW_CPP_DEFINE "NROS_RMW_ZENOH_CFFI" PARENT_SCOPE)
        set(NROS_RMW_CMAKE_TARGET "" PARENT_SCOPE)
    else()
        message(FATAL_ERROR "nros_rmw_dispatch: unknown rmw '${rmw}' "
            "(known: cyclonedds uorb xrce zenoh)")
    endif()
endfunction()

# Every rmw a descriptor in this checkout provides. DERIVED — see above.
set(NROS_RMW_KNOWN "cyclonedds;uorb;xrce;zenoh" CACHE INTERNAL "rmw names provided by nros-rmw.toml descriptors")

# nros_rmw_is_known(<name> <out_var>) — TRUE when a descriptor claims <name>.
function(nros_rmw_is_known name out_var)
    if(name IN_LIST NROS_RMW_KNOWN)
        set(${out_var} TRUE PARENT_SCOPE)
    else()
        set(${out_var} FALSE PARENT_SCOPE)
    endif()
endfunction()
