# SPDX-License-Identifier: Apache-2.0

function(nros_zephyr_add_cyclonedds_action_descriptors target)
    if(NOT CONFIG_NROS_RMW_CYCLONEDDS)
        return()
    endif()

    set(_nros_repo "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../../..")
    set(IDLC_EXECUTABLE "${_nros_repo}/build/cyclonedds/bin/idlc"
        CACHE FILEPATH "Host Cyclone DDS idlc for descriptor generation")
    set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${_nros_repo}/scripts/cyclonedds")
    include("${_nros_repo}/packages/dds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake")

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
        PKG_DIR /opt/ros/humble/share/builtin_interfaces
        INTERFACES msg/Time.msg
        INCLUDE_ROOT "${_idl_root}"
        GEN_ROOT "${_gen_root}"
        OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-ts/builtin_interfaces")

    nros_rmw_cyclonedds_generate_from_msg(_uuid_types
        PKG_NAME unique_identifier_msgs
        PKG_DIR /opt/ros/humble/share/unique_identifier_msgs
        INTERFACES msg/UUID.msg
        INCLUDE_ROOT "${_idl_root}"
        GEN_ROOT "${_gen_root}"
        OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-ts/unique_identifier_msgs")

    nros_rmw_cyclonedds_generate_from_msg(_action_types
        PKG_NAME action_msgs
        PKG_DIR /opt/ros/humble/share/action_msgs
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
        PKG_DIR /opt/ros/humble/share/example_interfaces
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
