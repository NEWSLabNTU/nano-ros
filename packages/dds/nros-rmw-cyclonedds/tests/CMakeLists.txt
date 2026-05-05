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

# Phase 117.X.3 service test fixtures: drive `.srv` through the
# 117.X.1 codegen helper so the typed IDLs include the in-payload
# `cdds_request_header_t` (rmw_writer_guid + rmw_sequence_number)
# layout. The two registered descriptor type names are
# `nros_test::srv::dds_::AddTwoInts_Request_` and
# `nros_test::srv::dds_::AddTwoInts_Response_`; the backend looks
# them up via `<type_name>_Request_` / `_Response_` suffix from
# the consumer-supplied `nros_test::srv::dds_::AddTwoInts`.
if(NROS_RMW_CYCLONEDDS_MSG_TO_IDL)
    nros_rmw_cyclonedds_generate_from_msg(_addtwoints_sources
        PKG_NAME nros_test
        PKG_DIR  "${CMAKE_CURRENT_SOURCE_DIR}/types/nros_test"
        INTERFACES srv/AddTwoInts.srv
    )
else()
    set(_addtwoints_sources "")
endif()
if(_addtwoints_sources STREQUAL "")
    message(STATUS
        "rosidl_adapter unavailable: skipping service_smoke / "
        "service_roundtrip / service_concurrent (Phase 117.X.3 "
        "needs the codegen helper for the typed IDL).")
endif()
if(_addtwoints_sources)
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
    )
    add_test(
        NAME nros_rmw_cyclonedds_service_smoke
        COMMAND nros_rmw_cyclonedds_service_smoke
    )

    # Phase 117.7 service request/reply data round-trip — Phase
    # 117.X.3 reshapes wire to include cdds_request_header_t.
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
    )
    add_test(
        NAME nros_rmw_cyclonedds_service_roundtrip
        COMMAND nros_rmw_cyclonedds_service_roundtrip
    )

    # Phase 117.7.B concurrent-call correlation. With Phase 117.X.3
    # this now goes through the typed-IDL header path; the
    # AddTwoInts type must be registered.
    add_executable(nros_rmw_cyclonedds_service_concurrent
        service_concurrent.cpp
        ${_addtwoints_sources}
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
endif()

# Phase 117.X.2: topic-prefix helper unit test (no DDS interaction).
add_executable(nros_rmw_cyclonedds_topic_prefix_test
    topic_prefix_test.cpp
)
target_include_directories(nros_rmw_cyclonedds_topic_prefix_test
    PRIVATE
        "${CMAKE_CURRENT_SOURCE_DIR}/../src"
)
add_test(
    NAME nros_rmw_cyclonedds_topic_prefix_test
    COMMAND nros_rmw_cyclonedds_topic_prefix_test
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
    PROPERTIES
        ENVIRONMENT "${_nros_rmw_test_env}"
)
if(_addtwoints_sources)
    set_tests_properties(
        nros_rmw_cyclonedds_service_smoke
        nros_rmw_cyclonedds_service_roundtrip
        nros_rmw_cyclonedds_service_concurrent
        PROPERTIES
            ENVIRONMENT "${_nros_rmw_test_env}"
    )
endif()
