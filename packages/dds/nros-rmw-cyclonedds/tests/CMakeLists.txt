# Phase 117.3: minimal smoke harness — verifies that the backend's
# register entry point links and returns NROS_RMW_RET_OK against a
# stub `nros_rmw_cffi_register` symbol.
#
# 117.12 swaps the stub for a real link against nros-rmw-cffi + adds
# pub/sub interop tests against a stock `rmw_cyclonedds` peer.

add_executable(nros_rmw_cyclonedds_register_smoke register_smoke.cpp)
target_link_libraries(nros_rmw_cyclonedds_register_smoke
    PRIVATE
        nros_rmw_cyclonedds
)
target_include_directories(nros_rmw_cyclonedds_register_smoke
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
)

# Prepend the in-tree Cyclone install dir so ld.so picks our
# pinned `libddsc.so` over any `ros-humble-cyclonedds` already on
# LD_LIBRARY_PATH (sourced by .envrc).
set(_nros_rmw_test_env
    "LD_LIBRARY_PATH=${CMAKE_PREFIX_PATH}/lib:$ENV{LD_LIBRARY_PATH}")

add_test(
    NAME nros_rmw_cyclonedds_register_smoke
    COMMAND nros_rmw_cyclonedds_register_smoke
)
set_tests_properties(nros_rmw_cyclonedds_register_smoke PROPERTIES
    ENVIRONMENT "${_nros_rmw_test_env}")

# Phase 117.4: session_open / session_close round-trip.
add_executable(nros_rmw_cyclonedds_session_smoke session_smoke.cpp)
target_link_libraries(nros_rmw_cyclonedds_session_smoke
    PRIVATE
        nros_rmw_cyclonedds
)
target_include_directories(nros_rmw_cyclonedds_session_smoke
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
)
add_test(
    NAME nros_rmw_cyclonedds_session_smoke
    COMMAND nros_rmw_cyclonedds_session_smoke
)

# Phase 117.2 / 117.5: IDL → descriptor → registry → topic round-trip.
# In the in-tree build the type-support helper hasn't been installed
# yet; load it from the source tree directly.
include("${CMAKE_CURRENT_SOURCE_DIR}/../cmake/NrosRmwCycloneddsTypeSupport.cmake")

nros_rmw_cyclonedds_idlc_compile(_test_string_sources
    IDL_FILE  "${CMAKE_CURRENT_SOURCE_DIR}/types/test_string.idl"
    OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/types"
    TYPE_NAME "nros_test::msg::TestString"
)

add_executable(nros_rmw_cyclonedds_codegen_smoke
    codegen_smoke.cpp
    ${_test_string_sources}
)
target_link_libraries(nros_rmw_cyclonedds_codegen_smoke
    PRIVATE
        nros_rmw_cyclonedds
)
target_include_directories(nros_rmw_cyclonedds_codegen_smoke
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
        "${CMAKE_CURRENT_BINARY_DIR}/types"
)
add_test(
    NAME nros_rmw_cyclonedds_codegen_smoke
    COMMAND nros_rmw_cyclonedds_codegen_smoke
)

# Phase 117.6 publisher / subscriber entity-creation smoke. Reuses
# the test_string descriptor from the codegen smoke above.
add_executable(nros_rmw_cyclonedds_pubsub_smoke
    pubsub_smoke.cpp
    ${_test_string_sources}
)
target_link_libraries(nros_rmw_cyclonedds_pubsub_smoke
    PRIVATE
        nros_rmw_cyclonedds
)
target_include_directories(nros_rmw_cyclonedds_pubsub_smoke
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
        "${CMAKE_CURRENT_BINARY_DIR}/types"
)
add_test(
    NAME nros_rmw_cyclonedds_pubsub_smoke
    COMMAND nros_rmw_cyclonedds_pubsub_smoke
)

# Phase 117.6.B raw-CDR data round-trip via publish_raw / try_recv_raw.
add_executable(nros_rmw_cyclonedds_data_roundtrip
    data_roundtrip.cpp
    ${_test_string_sources}
)
target_link_libraries(nros_rmw_cyclonedds_data_roundtrip
    PRIVATE
        nros_rmw_cyclonedds
)
target_include_directories(nros_rmw_cyclonedds_data_roundtrip
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
        "${CMAKE_CURRENT_BINARY_DIR}/types"
)
add_test(
    NAME nros_rmw_cyclonedds_data_roundtrip
    COMMAND nros_rmw_cyclonedds_data_roundtrip
)

# Phase 117.7 service entity-creation smoke. Drives a multi-type
# IDL (Request + Response) through the codegen helper to register
# both descriptors, then exercises service_server_create /
# service_client_create.
nros_rmw_cyclonedds_idlc_compile(_addtwoints_sources
    IDL_FILE  "${CMAKE_CURRENT_SOURCE_DIR}/types/test_addtwoints.idl"
    OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/types"
    TYPE_NAMES
        "nros_test::srv::AddTwoInts_Request"
        "nros_test::srv::AddTwoInts_Response"
)
add_executable(nros_rmw_cyclonedds_service_smoke
    service_smoke.cpp
    ${_addtwoints_sources}
)
target_link_libraries(nros_rmw_cyclonedds_service_smoke
    PRIVATE
        nros_rmw_cyclonedds
)
target_include_directories(nros_rmw_cyclonedds_service_smoke
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
        "${CMAKE_CURRENT_BINARY_DIR}/types"
)
add_test(
    NAME nros_rmw_cyclonedds_service_smoke
    COMMAND nros_rmw_cyclonedds_service_smoke
)

# Phase 117.7 service request/reply data round-trip.
add_executable(nros_rmw_cyclonedds_service_roundtrip
    service_roundtrip.cpp
    ${_addtwoints_sources}
)
target_link_libraries(nros_rmw_cyclonedds_service_roundtrip
    PRIVATE
        nros_rmw_cyclonedds
        pthread
)
target_include_directories(nros_rmw_cyclonedds_service_roundtrip
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
        "${CMAKE_CURRENT_BINARY_DIR}/types"
)
add_test(
    NAME nros_rmw_cyclonedds_service_roundtrip
    COMMAND nros_rmw_cyclonedds_service_roundtrip
)

# Phase 117.7.B concurrent-call correlation. No IDL types needed —
# the backend's envelope wraps all service traffic.
add_executable(nros_rmw_cyclonedds_service_concurrent
    service_concurrent.cpp
)
target_link_libraries(nros_rmw_cyclonedds_service_concurrent
    PRIVATE
        nros_rmw_cyclonedds
        pthread
)
target_include_directories(nros_rmw_cyclonedds_service_concurrent
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
)
add_test(
    NAME nros_rmw_cyclonedds_service_concurrent
    COMMAND nros_rmw_cyclonedds_service_concurrent
)

# Phase 117.X.1: .msg → mangled IDL → idlc → registry pipeline.
# Drives the new helper through a synthetic std_msgs package; if
# the host lacks /opt/ros/humble's rosidl_adapter the helper emits
# a soft warning at configure time and we skip the test.
if(NROS_RMW_CYCLONEDDS_MSG_TO_IDL)
    nros_rmw_cyclonedds_generate_from_msg(_from_msg_sources
        PKG_NAME std_msgs
        PKG_DIR  "${CMAKE_CURRENT_SOURCE_DIR}/types/std_msgs"
        INTERFACES msg/String.msg
    )
    add_executable(nros_rmw_cyclonedds_from_msg_smoke
        from_msg_smoke.cpp
        ${_from_msg_sources}
    )
    target_link_libraries(nros_rmw_cyclonedds_from_msg_smoke
        PRIVATE
            nros_rmw_cyclonedds
    )
    target_include_directories(nros_rmw_cyclonedds_from_msg_smoke
        PRIVATE
            ${NROS_RMW_CFFI_DIR}
    )
    add_test(
        NAME nros_rmw_cyclonedds_from_msg_smoke
        COMMAND nros_rmw_cyclonedds_from_msg_smoke
    )
    set_tests_properties(nros_rmw_cyclonedds_from_msg_smoke
        PROPERTIES ENVIRONMENT "${_nros_rmw_test_env}")
endif()

# Apply the LD_LIBRARY_PATH prefix to every test added above.
set_tests_properties(
    nros_rmw_cyclonedds_session_smoke
    nros_rmw_cyclonedds_codegen_smoke
    nros_rmw_cyclonedds_pubsub_smoke
    nros_rmw_cyclonedds_data_roundtrip
    nros_rmw_cyclonedds_service_smoke
    nros_rmw_cyclonedds_service_roundtrip
    nros_rmw_cyclonedds_service_concurrent
    PROPERTIES
        ENVIRONMENT "${_nros_rmw_test_env}"
)
