# SPDX-License-Identifier: Apache-2.0

function(nros_zephyr_add_cyclonedds_action_descriptors target)
    if(NOT CONFIG_NROS_RMW_CYCLONEDDS)
        return()
    endif()

    # Phase 180.B — copy-out clean: no repo-tree walk. The nano-ros Zephyr
    # module exports NROS_CYCLONE_IDLC / NROS_CYCLONE_SCRIPTS_DIR and puts
    # this dir on CMAKE_MODULE_PATH, so the descriptor codegen tooling is
    # discoverable by bare name regardless of where west cloned the module.
    set(IDLC_EXECUTABLE "${NROS_CYCLONE_IDLC}"
        CACHE FILEPATH "Host Cyclone DDS idlc for descriptor generation" FORCE)
    set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${NROS_CYCLONE_SCRIPTS_DIR}")
    include(NrosRmwCycloneddsTypeSupport)

    set(_idl_root "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-ts/_idlroot")
    set(_gen_root "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-ts/_genroot")

    set(_builtin_idls
        "${_idl_root}/builtin_interfaces/msg/Time.idl")
    set(_uuid_idls
        "${_idl_root}/unique_identifier_msgs/msg/UUID.idl")
    set(_action_idls
        "${_idl_root}/action_msgs/msg/GoalInfo.idl"
        "${_idl_root}/action_msgs/msg/GoalStatus.idl"
        "${_idl_root}/action_msgs/msg/GoalStatusArray.idl"
        "${_idl_root}/action_msgs/msg/CancelGoal.idl")

    nros_rmw_cyclonedds_generate_from_msg(_builtin_types
        PKG_NAME builtin_interfaces
        PKG_DIR "$ENV{NROS_BUILTIN_INTERFACES_DIR}"
        INTERFACES msg/Time.msg
        INCLUDE_ROOT "${_idl_root}"
        GEN_ROOT "${_gen_root}"
        OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-ts/builtin_interfaces")

    nros_rmw_cyclonedds_generate_from_msg(_uuid_types
        PKG_NAME unique_identifier_msgs
        PKG_DIR "$ENV{NROS_UNIQUE_IDENTIFIER_MSGS_DIR}"
        INTERFACES msg/UUID.msg
        INCLUDE_ROOT "${_idl_root}"
        GEN_ROOT "${_gen_root}"
        OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-ts/unique_identifier_msgs")

    nros_rmw_cyclonedds_generate_from_msg(_action_types
        PKG_NAME action_msgs
        PKG_DIR "$ENV{NROS_ACTION_MSGS_DIR}"
        INTERFACES
            msg/GoalInfo.msg
            msg/GoalStatus.msg
            msg/GoalStatusArray.msg
            srv/CancelGoal.srv
        IDL_DEPENDS ${_builtin_idls} ${_uuid_idls}
        INCLUDE_ROOT "${_idl_root}"
        GEN_ROOT "${_gen_root}"
        OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-ts/action_msgs")

    nros_rmw_cyclonedds_generate_from_msg(_example_types
        PKG_NAME example_interfaces
        PKG_DIR "$ENV{NROS_EXAMPLE_INTERFACES_DIR}"
        INTERFACES action/Fibonacci.action
        IDL_DEPENDS ${_builtin_idls} ${_uuid_idls} ${_action_idls}
        INCLUDE_ROOT "${_idl_root}"
        GEN_ROOT "${_gen_root}"
        OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-ts/example_interfaces")

    target_include_directories(${target} PRIVATE "${_gen_root}")
    if(TARGET nros_rmw_cyclonedds)
        target_include_directories(${target} PRIVATE
            "$<TARGET_PROPERTY:nros_rmw_cyclonedds,INTERFACE_INCLUDE_DIRECTORIES>")
    endif()
    target_sources(${target} PRIVATE
        ${_builtin_types}
        ${_uuid_types}
        ${_action_types}
        ${_example_types})
endfunction()
