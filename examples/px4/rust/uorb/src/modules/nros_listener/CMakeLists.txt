# Thin wrapper pulling the nros_listener example crate into the PX4 build.
# See ../nros_talker/CMakeLists.txt for the pattern explanation.

set(PX4_RS_DIR  ${CMAKE_CURRENT_LIST_DIR}/../../../../../../../third-party/px4/px4-rs)
set(CRATE_DIR   ${CMAKE_CURRENT_LIST_DIR}/../../../listener)
include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

px4_rust_module(
    NAME     nros_listener
    CRATE    px4_rs_listener
    MANIFEST ${CRATE_DIR}/Cargo.toml
    ENTRY    nros_listener_main
)
