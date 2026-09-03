# Phase 115.K.2 scaffold smoke test.
# Confirms the static library links and the register entry point
# returns NROS_RMW_RET_OK against a stub `nros_rmw_cffi_register`
# defined in the test TU itself (the real symbol lives in the
# nros-rmw-cffi Rust crate; the scaffold doesn't link against the
# Rust runtime).

add_executable(nros_rmw_xrce_c_smoke smoke.c)
target_link_libraries(nros_rmw_xrce_c_smoke PRIVATE nros_rmw_xrce)
target_include_directories(nros_rmw_xrce_c_smoke PRIVATE
    ${NROS_RMW_CFFI_DIR}
)

add_test(NAME nros_rmw_xrce_c_smoke COMMAND nros_rmw_xrce_c_smoke)

# Issue 0847 — an entity handle outliving its session must not fault.
#
# Needs `internal.h` (the session/entity states are private to the backend), and
# no agent: a CLOSED session's destructor must not touch the transport, which is
# the invariant under test. Before the fix this binary SEGFAULTED rather than
# reporting, so ctest's exit-status verdict is what makes it a test at all --
# the issue asks for exactly that, because a crash after the last line of output
# reads as a clean run to anything that greps stdout.
add_executable(nros_rmw_xrce_entity_lifetime entity_lifetime.c)
target_link_libraries(nros_rmw_xrce_entity_lifetime PRIVATE nros_rmw_xrce)
# `internal.h` includes <uxr/client/client.h>, so this target needs the same
# private include set the library uses -- they are PRIVATE on `nros_rmw_xrce`
# (correctly: they are not part of the installed surface), so linking the
# library does not inherit them and they are restated here.
target_include_directories(nros_rmw_xrce_entity_lifetime PRIVATE
    ${NROS_RMW_CFFI_DIR}
    ${NROS_PLATFORM_CFFI_INCLUDE}
    ${CMAKE_CURRENT_SOURCE_DIR}/../src
    ${MICROXRCEDDS_CLIENT_DIR}/include
    ${MICROXRCEDDS_CLIENT_DIR}/src/c
    ${MICROCDR_DIR}/include
    ${CMAKE_CURRENT_BINARY_DIR}/../uxr_generated
    ${CMAKE_CURRENT_BINARY_DIR}/../ucdr_generated
)

add_test(NAME nros_rmw_xrce_entity_lifetime COMMAND nros_rmw_xrce_entity_lifetime)
