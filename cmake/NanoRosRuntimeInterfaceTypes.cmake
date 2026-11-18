# cmake/NanoRosRuntimeInterfaceTypes.cmake — issue 2304
#
# The interface types the RUNTIME creates entities for on its own, given to a
# backend's per-message typesupport hook the way a user's message packages are.
#
# A backend that resolves types through a descriptor registry (Cyclone DDS:
# `[rmw.codegen].per_message`) can create an entity only for a type whose
# descriptor some linked translation unit registered. On the cmake road the
# ONLY producer of those registrations is `nros_generate_interfaces()`, which
# runs for the packages the application names. The executor's own entities are
# typed from packages the application never names:
#
#   param_services  six `rcl_interfaces/srv/*` servers per node plus the
#                   `/parameter_events` publisher (`rcl_interfaces/msg/
#                   ParameterEvent`, which carries `builtin_interfaces/Time`).
#   lifecycle       five `lifecycle_msgs/srv/*` servers plus the
#                   `~/transition_event` publisher (`TransitionEvent`).
#
# Measured before this file (issue 2304, `examples/linux/c/parameters` on
# `rmw = "cyclonedds"`): `rclc_parameter_server_init_with_option` returned
# RCL_RET_ERROR, because `Executor::enable_parameter_events_on` failed
# `Transport(PublisherCreationFailed)` — the backend had no descriptor for
# `rcl_interfaces::msg::dds_::ParameterEvent_`. With the events opted out, the
# next call, `rclc_executor_add_parameter_server`, failed the same way on the
# first service. The cargo road never had this: there `nros-node` registers each
# type's descriptor from its schema at create time
# (`rmw_type_registry::register_type`), a seam the cmake road's C++ backend does
# not install.
#
# The packages go through the SAME hook a user package does, so a package both
# the runtime and the application need (`builtin_interfaces`, or an app that
# uses `rcl_interfaces` itself) has ONE typesupport library: whichever call
# comes second finds `<pkg>__cyclonedds_ts` and only attaches it. That is also
# why a package is generated WHOLE rather than as the subset the runtime uses —
# the first caller decides what the shared library holds.
#
# `lifecycle` was the same defect, measured the same way: a scratch copy of the
# example with `lifecycle` added and `rclc_executor_register_lifecycle_services`
# called got `Transport(Unsupported)` (RCLC_RET_NOT_ALLOWED) on Cyclone.

include_guard(GLOBAL)
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosDiagnostics.cmake")

# Issue 2704 -- the axis -> package table is not spelled here. It is the ONE
# runtime-interface table, `packages/core/nros-orchestration-ir/data/
# runtime-interface-types.json`, which the CLI (its configure-time refusal) and
# the cargo road (its descriptor-table count) read too, so a runtime-owned
# axis or package cannot land on one road only. This road reads its
# `packages` columns: it generates each package WHOLE, so the descriptor
# table records every type of it (the cargo road counts only `registers`,
# the types the executor creates).

# _nros_runtime_interface_packages(<features> <out>)
#
# <out>: `<pkg>[:<dep>...]` entries, in the table's dependency order (a
# package's IDL includes its dependencies', so theirs are generated first),
# for every package an axis named in <features> needs; empty when none.
function(_nros_runtime_interface_packages features out)
    # Resolved from this file, not a variable set at include time: the file is
    # include_guard(GLOBAL)ed, so a variable would exist only in the first
    # including directory's scope.
    set(_table "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../packages/core/nros-orchestration-ir/data/runtime-interface-types.json")
    if(NOT EXISTS "${_table}")
        message(FATAL_ERROR
            "nano-ros: the runtime interface table is missing at ${_table}; "
            "the nano-ros checkout is incomplete.")
    endif()
    file(READ "${_table}" _json)
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_table}")
    set(_wanted "")
    string(JSON _n_axes LENGTH "${_json}" axes)
    math(EXPR _last "${_n_axes} - 1")
    foreach(_i RANGE ${_last})
        string(JSON _axis GET "${_json}" axes ${_i} axis)
        if(NOT _axis IN_LIST features)
            continue()
        endif()
        string(JSON _n LENGTH "${_json}" axes ${_i} packages)
        if(_n EQUAL 0)
            continue()
        endif()
        math(EXPR _plast "${_n} - 1")
        foreach(_j RANGE ${_plast})
            string(JSON _p GET "${_json}" axes ${_i} packages ${_j})
            list(APPEND _wanted "${_p}")
        endforeach()
    endforeach()
    set(_pkgs "")
    string(JSON _n_pkgs LENGTH "${_json}" packages)
    math(EXPR _last "${_n_pkgs} - 1")
    foreach(_i RANGE ${_last})
        string(JSON _name GET "${_json}" packages ${_i} name)
        if(NOT _name IN_LIST _wanted)
            continue()
        endif()
        set(_entry "${_name}")
        string(JSON _n LENGTH "${_json}" packages ${_i} deps)
        if(_n GREATER 0)
            math(EXPR _dlast "${_n} - 1")
            foreach(_j RANGE ${_dlast})
                string(JSON _d GET "${_json}" packages ${_i} deps ${_j})
                string(APPEND _entry ":${_d}")
            endforeach()
        endif()
        list(APPEND _pkgs "${_entry}")
    endforeach()
    set(${out} "${_pkgs}" PARENT_SCOPE)
endfunction()

# nros_runtime_interface_types(<features> [RMW <name>] [HOOK <command>]
#                               [TARGETS <umbrella>...])
#
# For each runtime-owned interface package the declared `<features>` imply,
# run the selected backend's per-message hook and link the result into each
# umbrella target. A backend with no hook (zenoh, xrce) pays nothing.
#
# Issue 2704 -- this is the ONE spelling for every road that builds an image
# on a descriptor-registry backend, not only the top-level one. The Zephyr
# module road calls it too (`zephyr/CMakeLists.txt`), and before it did, a
# Zephyr image on Cyclone with `param_services` booted without its parameter
# services (`Transport(Unsupported)` from `service_create`). A road names:
#
#   RMW   the backend, when the road does not set `NROS_RMW` (Zephyr derives
#         it from Kconfig);
#   HOOK  the command that generates one package's descriptors ON THAT ROAD,
#         when it is not the backend's top-level per-message hook. It is
#         called with the same arguments (TARGET, LIB_TARGET, INTERFACE_FILES,
#         DEPENDENCIES). Whether a hook runs at all is still the backend's
#         fact (`NROS_RMW_PER_MESSAGE_HOOK` non-empty), never the road's.
#
# `check-runtime-interface-types-roads` holds every road that generates
# per-message type support to a call of this function.
function(nros_runtime_interface_types features)
    cmake_parse_arguments(_RIT "" "RMW;HOOK" "TARGETS" ${ARGN})
    set(_rmw "${NROS_RMW}")
    if(_RIT_RMW)
        set(_rmw "${_RIT_RMW}")
    endif()
    if(NOT _rmw OR _rmw STREQUAL "none")
        return()
    endif()
    nros_rmw_dispatch("${_rmw}")
    if(NOT NROS_RMW_PER_MESSAGE_HOOK)
        return()
    endif()
    set(_hook "${NROS_RMW_PER_MESSAGE_HOOK}")
    if(_RIT_HOOK)
        set(_hook "${_RIT_HOOK}")
    endif()
    if(NOT COMMAND ${_hook})
        return()
    endif()

    _nros_runtime_interface_packages("${features}" _pkgs)
    if(_pkgs STREQUAL "")
        return()
    endif()

    if(NOT COMMAND _nros_find_msg_package_root)
        include("${CMAKE_CURRENT_FUNCTION_LIST_DIR}/find/_NrosFindRosMsgPackage.cmake")
    endif()

    set(_lib nros_runtime_interface_types)
    if(NOT TARGET ${_lib})
        add_library(${_lib} INTERFACE)
    endif()
    foreach(_entry IN LISTS _pkgs)
        string(REPLACE ":" ";" _parts "${_entry}")
        list(POP_FRONT _parts _pkg)
        set(_deps ${_parts})
        _nros_find_msg_package_root(${_pkg} _root)
        if(_root STREQUAL "NOTFOUND")
            # issue 2304.
            message(FATAL_ERROR
                "nano-ros: [system].features `${features}` on rmw '${_rmw}' "
                "needs the `${_pkg}` interface package: the runtime creates the "
                "parameter / lifecycle services and their event publishers with "
                "its types, and this backend creates an entity only for a type "
                "whose descriptor is generated into the image. Make "
                "${_pkg} discoverable (source ROS 2's setup.bash so "
                "AMENT_PREFIX_PATH names it), or drop the feature.")
        endif()
        file(GLOB _files CONFIGURE_DEPENDS
            "${_root}/msg/*.msg" "${_root}/srv/*.srv" "${_root}/action/*.action")
        list(SORT _files)
        if(NOT _files)
            # issue 2304.
            message(FATAL_ERROR
                "nano-ros: ${_pkg} at ${_root} holds no .msg/.srv/.action file "
                "to generate from. Point AMENT_PREFIX_PATH at a complete install of it.")
        endif()
        cmake_language(CALL ${_hook}
            TARGET          "${_pkg}"
            LIB_TARGET      "${_lib}"
            INTERFACE_FILES ${_files}
            DEPENDENCIES    ${_deps})
    endforeach()
    foreach(_t IN LISTS _RIT_TARGETS)
        if(TARGET ${_t})
            target_link_libraries(${_t} INTERFACE ${_lib})
        endif()
    endforeach()
    string(REGEX REPLACE ":[^;]*" "" _names "${_pkgs}")
    string(REPLACE ";" ", " _names "${_names}")
    message(STATUS "nano-ros: runtime interface types on rmw '${_rmw}': ${_names}")
    nros_message_detail(
        "nano-ros: the runtime creates the parameter / lifecycle services and their "
        "event publishers with these packages' types, which this backend resolves "
        "through generated descriptors (issue 2304)")
endfunction()
