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
