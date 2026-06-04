function(nros_zephyr_configure_rmw_xrce)
# -------------------------------------------------------------------------
# XRCE-DDS platform support (compiled by Zephyr CMake)
# -------------------------------------------------------------------------
# xrce_zephyr.c provides L4 network readiness wait + uxr_millis/uxr_nanos.

zephyr_library_sources(
    ${NROS_REPO_DIR}/packages/xrce/xrce-zephyr/src/xrce_zephyr.c
)
zephyr_include_directories(
    ${NROS_REPO_DIR}/packages/xrce/xrce-zephyr/include
)

endfunction()
