# Issue 1663 -- `NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES` on the CMAKE road.
#
# `descriptors.cpp` keeps a static `Entry g_entries[N]` (its `register_descriptor`
# dedupes by type NAME), and over N a registration is DROPPED from a static
# constructor that cannot report -- the operator meets it later as
# `publisher_create` returning UNSUPPORTED (issue 0280's residual). The cargo
# road raises N from the SystemModel (`model_ingest::
# resolve_cyclonedds_max_descriptor_types` -> `nros-rmw-cyclonedds-sys/build.rs`),
# and nothing defined it on the cmake roads, so there N was always the header's
# 256.
#
# The model's type count is the wrong number here (issue 1663, measured: 1 model
# type against 36 linked `*_desc` symbols on `examples/workspaces/cpp`
# freertos_posix), because a cmake image registers every descriptor whose
# register TU it LINKS, and `nros_generate_interfaces` emits one for every
# message of every found package. So the single writer on this road is the one
# place every register TU is generated: `nros_rmw_cyclonedds_idlc_compile`
# records each type NAME it generates a registration for, and this file sizes
# the table from the DISTINCT names, once, after the configure has generated
# all of them. That is an UPPER bound on what the image registers (a generated
# TU that is never linked registers nothing), which is the safe direction for a
# table whose overflow is silent.
#
# Only ever RAISES the header's 256, exactly as the cargo road's writer does, so
# every image that fits keeps compiling byte-for-byte as before. A caller pin
# (`-DNROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES=<n>` or the environment variable of
# that name) wins when it is large enough and is REFUSED when it is not -- the
# cargo road's rule too.

include_guard(GLOBAL)

# The header default (`descriptors.cpp`, `heap_budget.hpp`), mirrored ONCE, as a
# function rather than a variable: the deferred applier runs in the TOP-LEVEL
# directory's scope, where a variable set in whichever directory included this
# file is not visible (measured: the first cut printed "keeps its default  rows").
function(nros_cyclonedds_descriptor_types_default _out)
    set(${_out} 256 PARENT_SCOPE)
endfunction()

# nros_cyclonedds_record_descriptor_types(<type-name>...)
#
# Record the registry keys a generated register TU will register, and schedule
# the one deferred sizing call. Called by `nros_rmw_cyclonedds_idlc_compile`.
function(nros_cyclonedds_record_descriptor_types)
    if(NOT ARGN)
        return()
    endif()
    set_property(GLOBAL APPEND PROPERTY NROS_CYCLONEDDS_GENERATED_DESCRIPTOR_TYPES ${ARGN})
    get_property(_scheduled GLOBAL PROPERTY NROS_CYCLONEDDS_DESCRIPTOR_CAP_SCHEDULED)
    if(_scheduled)
        return()
    endif()
    set_property(GLOBAL PROPERTY NROS_CYCLONEDDS_DESCRIPTOR_CAP_SCHEDULED TRUE)
    # At the end of the TOP-LEVEL directory: by then every
    # `nros_generate_interfaces()` of the configure has run, wherever it was
    # called from, and the target that compiles `descriptors.cpp` exists.
    cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
        CALL _nros_cyclonedds_apply_descriptor_cap)
endfunction()

# nros_cyclonedds_descriptor_cap(<out-var> <distinct-count> <pin>)
#
# The rule, as a pure function: empty when the header default holds, else the
# number to compile. FATAL_ERROR when a pin is smaller than the demand.
function(nros_cyclonedds_descriptor_cap _out _count _pin)
    nros_cyclonedds_descriptor_types_default(_default)
    if(NOT "${_pin}" STREQUAL "")
        if(NOT _pin MATCHES "^[0-9]+$" OR _pin LESS 1)
            message(FATAL_ERROR
                "NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES='${_pin}' is not a positive count")
        endif()
        if(_pin LESS _count)
            message(FATAL_ERROR
                "NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES is pinned to ${_pin}, but this "
                "configure generates registrations for ${_count} distinct Cyclone "
                "descriptor types. Over the cap `descriptors.cpp` DROPS a registration "
                "from a static constructor that cannot report it, and the type's "
                "publisher later fails UNSUPPORTED (issue 0280). Raise the pin to at "
                "least ${_count}, or unset it (issue 1663).")
        endif()
        set(${_out} "${_pin}" PARENT_SCOPE)
        return()
    endif()
    if(_count GREATER _default)
        set(${_out} "${_count}" PARENT_SCOPE)
    else()
        set(${_out} "" PARENT_SCOPE)
    endif()
endfunction()

# The deferred applier. The target is the one that compiles `descriptors.cpp`:
# `nros_rmw_cyclonedds` on the cmake road, the module's `nros` library on the
# Zephyr west road. An IMPORTED target (a prebuilt backend) compiles nothing.
function(_nros_cyclonedds_apply_descriptor_cap)
    get_property(_names GLOBAL PROPERTY NROS_CYCLONEDDS_GENERATED_DESCRIPTOR_TYPES)
    list(REMOVE_DUPLICATES _names)
    list(LENGTH _names _count)
    set(_pin "")
    if(DEFINED NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES)
        set(_pin "${NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES}")
    elseif(DEFINED ENV{NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES})
        set(_pin "$ENV{NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES}")
    endif()
    nros_cyclonedds_descriptor_cap(_n "${_count}" "${_pin}")

    # `nros` is a candidate only on the Zephyr road, detected by the module's
    # own resolver (the test `NanoRosEntry.cmake` uses): elsewhere a target of
    # that name is a Corrosion crate import, not the Cyclone TUs.
    set(_cands nros_rmw_cyclonedds)
    if(COMMAND nros_set_cargo_env_from_kconfig)
        list(APPEND _cands nros)
    endif()
    set(_target "")
    foreach(_cand IN LISTS _cands)
        if(_target STREQUAL "" AND TARGET ${_cand})
            get_target_property(_imported ${_cand} IMPORTED)
            get_target_property(_type ${_cand} TYPE)
            if(NOT _imported AND NOT _type STREQUAL "INTERFACE_LIBRARY")
                set(_target ${_cand})
            endif()
        endif()
    endforeach()
    if(_n STREQUAL "")
        nros_cyclonedds_descriptor_types_default(_default)
        message(STATUS
            "nano-ros: Cyclone descriptor table keeps its default "
            "${_default} rows "
            "(${_count} distinct type(s) generated in this configure, issue 1663)")
        return()
    endif()
    if(_target STREQUAL "")
        message(STATUS
            "nano-ros: ${_count} distinct Cyclone descriptor types generated, but no "
            "target here compiles `descriptors.cpp` (a prebuilt backend sizes its own "
            "table) -- NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES not applied (issue 1663)")
        return()
    endif()
    target_compile_definitions(${_target} PRIVATE NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES=${_n})
    message(STATUS
        "nano-ros: Cyclone descriptor table sized to ${_n} rows for ${_count} distinct "
        "generated type(s) -> ${_target} (issue 1663)")
endfunction()
